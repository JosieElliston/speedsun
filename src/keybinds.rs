//! The keybind system: variables, folders of bindings, and the pass that turns
//! input into at most one command per frame.
//!
//! A binding fires on the **rising edge of one boolean variable** — its
//! trigger, always a variable and never an expression — while its **guard**
//! (an expression, cf. [`crate::expr`]) and every enclosing folder's guard are
//! true. Keeping the edge on a variable is what makes holding a key through a
//! guard change harmless: switching keybind sets while `key_r` is held gives
//! `key_r` no new edge, so nothing fires.
//!
//! Everything the input system knows is a variable, so mousebinds are just
//! keybinds: `mouse_left` and `hovered_grip` are builtins, and the default set
//! binds them exactly the way it binds keys.

use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use eframe::egui::{self, collapsing_header::CollapsingState};

use crate::{
    commands::{Command, Origin},
    expr::{self, ExprField, Type, Value},
    notation,
    puzzle_state::{LayerMask, Reorientation, Side, Twist},
};

/// What the rest of the app tells the keybind system about this frame, beyond
/// the raw keyboard and mouse state it reads from egui itself.
pub struct InputContext {
    /// the twist gizmo under the pointer, as the `hovered_grip` variable.
    pub hovered_grip: Option<Side>,
    /// `hovered_grip_inverted`: clicking that gizmo should twist the other way
    /// (it's a backface and the view reverses backface twists).
    pub hovered_grip_inverted: bool,
}

/// The persistent input state every builtin variable reads from. Both physical
/// events and reference clicks mutate it, so there is one source of truth for
/// "is this down" — a physical release event clears exactly the bit a click
/// set, and no separate clicked overlay has to be reconciled with the keyboard.
///
/// A clone taken at the end of each pass ([`Keybinds::prev_input`]) is what
/// rising edges are found against.
#[derive(Default, Clone)]
struct InputState {
    /// keys held down. press events add, release events remove, a reference
    /// click toggles.
    keys: HashSet<egui::Key>,
    /// keys that got a press event this pass, cleared each pass, so a key
    /// counts as down for the frame it was pressed even if the release already
    /// arrived — at speedsolving speed a tap can fit between two frames, and it
    /// mustn't be dropped. It's a real false -> true across the frame boundary,
    /// not an override of the edge rule.
    pressed: HashSet<egui::Key>,
    /// left, right, middle, held down.
    mouse: [bool; 3],
    /// mouse buttons pressed this pass, cleared each pass; the same
    /// within-a-frame reasoning as `pressed`.
    mouse_pressed: [bool; 3],
    modifiers: egui::Modifiers,
    hovered_grip: Option<Side>,
    hovered_grip_inverted: bool,
}
impl InputState {
    /// a key went down: held, and marked pressed-this-pass so it's an edge.
    fn press(&mut self, key: egui::Key) {
        self.keys.insert(key);
        self.pressed.insert(key);
    }

    /// a key went up.
    fn release(&mut self, key: egui::Key) {
        self.keys.remove(&key);
    }

    /// a mouse button went down, held and marked pressed-this-pass.
    fn mouse_press(&mut self, idx: usize) {
        self.mouse[idx] = true;
        self.mouse_pressed[idx] = true;
    }
}

/// the three mouse buttons the builtins expose, in `mouse` order.
fn mouse_index(button: egui::PointerButton) -> Option<usize> {
    match button {
        egui::PointerButton::Primary => Some(0),
        egui::PointerButton::Secondary => Some(1),
        egui::PointerButton::Middle => Some(2),
        _ => None,
    }
}

/// Fold physical modifier *changes* into the persistent state. Only a flag that
/// actually moved is written, so a reference-toggled modifier survives frames
/// where the physical key didn't move, while a real release still clears it —
/// the same hand-back a key gets, against egui's authoritative `modifiers`.
fn apply_modifier_delta(current: &mut egui::Modifiers, was: egui::Modifiers, now: egui::Modifiers) {
    if now.shift != was.shift {
        current.shift = now.shift;
    }
    if now.ctrl != was.ctrl {
        current.ctrl = now.ctrl;
    }
    if now.alt != was.alt {
        current.alt = now.alt;
    }
    if now.mac_cmd != was.mac_cmd {
        current.mac_cmd = now.mac_cmd;
    }
    if now.command != was.command {
        current.command = now.command;
    }
}

/// color for bindings outside any folder, and for a freshly added folder.
const ROOT_COLOR: egui::Color32 = egui::Color32::GRAY;

/// the builtin variables that have fixed names, for the reference list. every
/// key also has one, named `key_` + its egui name (`key_a`, `key_space`,
/// `key_arrowleft`), which `builtin` resolves on demand.
const BUILTINS: &[(&str, Type)] = &[
    ("hovered_grip", Type::Grip),
    ("hovered_grip_inverted", Type::Bool),
    ("mouse_left", Type::Bool),
    ("mouse_right", Type::Bool),
    ("mouse_middle", Type::Bool),
    ("key_shift", Type::Bool),
    ("key_ctrl", Type::Bool),
    ("key_alt", Type::Bool),
    ("key_command", Type::Bool),
];

/// The value of a builtin variable, read straight from the persistent input.
/// Reference clicks and the keyboard both write to that state, so there is no
/// overlay to consult first.
///
/// `key_shift` and friends are egui's modifier *state*, which doesn't say which
/// side was pressed; the individual modifier keys are ordinary keys, so
/// `key_shiftleft` and `key_shiftright` work too. `key_command` is cmd on macOS
/// and ctrl elsewhere, matching every other app on the platform.
fn read(name: &str, input: &InputState) -> Option<Value> {
    Some(match name {
        "hovered_grip" => Value::Grip(input.hovered_grip),
        "hovered_grip_inverted" => Value::Bool(input.hovered_grip_inverted),
        "mouse_left" => Value::Bool(input.mouse[0] || input.mouse_pressed[0]),
        "mouse_right" => Value::Bool(input.mouse[1] || input.mouse_pressed[1]),
        "mouse_middle" => Value::Bool(input.mouse[2] || input.mouse_pressed[2]),
        "key_shift" => Value::Bool(input.modifiers.shift),
        "key_ctrl" => Value::Bool(input.modifiers.ctrl),
        "key_alt" => Value::Bool(input.modifiers.alt),
        "key_command" => Value::Bool(input.modifiers.command),
        _ => {
            let key = key_by_name(name.strip_prefix("key_")?)?;
            // pressed as well as held: a key tapped inside one frame is down
            // for that frame even though the release already cleared it.
            Value::Bool(input.keys.contains(&key) || input.pressed.contains(&key))
        }
    })
}

/// Is this a builtin variable's name? Doesn't depend on the input, since the
/// set of names doesn't: every key has one whether or not it's down.
pub fn is_builtin(name: &str) -> bool {
    BUILTINS.iter().any(|(builtin, _)| *builtin == name)
        || name.strip_prefix("key_").and_then(key_by_name).is_some()
}

/// does this name resolve to anything at all? a trigger naming a variable
/// nobody declared is a typo, not an unbound key, so the editor says so.
fn is_known(name: &str, vars: &[UserVar]) -> bool {
    is_builtin(name) || vars.iter().any(|var| var.name == name)
}

/// the variable a key is read through: `key_` + its egui name, lowercased.
pub fn key_variable(key: egui::Key) -> String {
    format!("key_{}", format!("{key:?}").to_lowercase())
}

/// keyboard layout is assumed to be the one egui reports; the names are its
/// `Key` names lowercased.
fn key_by_name(name: &str) -> Option<egui::Key> {
    static KEYS: OnceLock<HashMap<String, egui::Key>> = OnceLock::new();
    KEYS.get_or_init(|| {
        egui::Key::ALL
            .iter()
            .map(|&key| (format!("{key:?}").to_lowercase(), key))
            .collect()
    })
    .get(name)
    .copied()
}

/// A user-declared variable. There is no separate declaration and current
/// value: the declaration holds the live value, so editing it here is how you
/// set it, and the pass reads whatever is in it.
#[derive(Debug, Clone)]
pub struct UserVar {
    pub name: String,
    pub value: Value,
    /// show in the bottom bar, where it can be toggled without opening the tab.
    pub pinned: bool,
}

/// Variable lookup for one pass: builtins shadow user variables, so builtin
/// names are effectively reserved.
///
/// The two forced fields are what make a preview possible — asking "what would
/// happen if this were pressed" is the same pass with one variable answered
/// differently, so nothing has to be mutated and put back.
struct Vars<'a> {
    input: &'a InputState,
    user: UserVars<'a>,
    /// this variable reads as this value, whatever the input says.
    forced: Option<(&'a str, bool)>,
    /// the pointer is over this grip, wherever it really is.
    hovered_grip: Option<Side>,
}
impl<'a> Vars<'a> {
    fn new(input: &'a InputState, user: UserVars<'a>) -> Self {
        Self {
            input,
            user,
            forced: None,
            hovered_grip: None,
        }
    }
}

/// Where a pass reads user variables: live off the declarations, or from the
/// snapshot of what they held during the last pass. Only rising edges need the
/// snapshot, so everything else reads the live values and can't go stale.
enum UserVars<'a> {
    Live(&'a [UserVar]),
    Snapshot(&'a HashMap<String, Value>),
}
impl UserVars<'_> {
    fn get(&self, name: &str) -> Option<Value> {
        match self {
            // a handful of variables, so a scan beats keeping a map in sync.
            UserVars::Live(vars) => vars
                .iter()
                .find(|var| var.name == name)
                .map(|var| var.value),
            UserVars::Snapshot(values) => values.get(name).copied(),
        }
    }
}
impl expr::Env for Vars<'_> {
    fn get(&self, name: &str) -> Option<Value> {
        if let Some((forced, value)) = self.forced
            && forced == name
        {
            return Some(Value::Bool(value));
        }
        if self.hovered_grip.is_some() && name == "hovered_grip" {
            return Some(Value::Grip(self.hovered_grip));
        }
        read(name, self.input).or_else(|| self.user.get(name))
    }
}

#[derive(Debug, Clone)]
pub struct Folder {
    pub name: String,
    /// colors this folder's bindings wherever they're reported.
    pub color: egui::Color32,
    /// the pass only explores the folder while this is true.
    pub guard: ExprField,
    pub children: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct Binding {
    /// the boolean variable whose rising edge fires this.
    pub trigger: String,
    pub guard: ExprField,
    pub command: BindCommand,
}

#[derive(Debug, Clone)]
pub enum Node {
    Folder(Folder),
    Binding(Binding),
}

/// What a binding does. Arguments are expressions, so one binding covers both
/// twist directions (`invert: key_shift`) and follows whatever the default
/// mask happens to be.
#[derive(Debug, Clone)]
pub enum BindCommand {
    Twist {
        grip: ExprField,
        layers: ExprField,
        multiplicity: ExprField,
        invert: ExprField,
    },
    /// whole-puzzle reorientation, named by the side it turns like: `U` turns
    /// the puzzle the way a `U` twist turns its layer.
    Reorient {
        grip: ExprField,
        multiplicity: ExprField,
        invert: ExprField,
    },
    Align,
    Undo,
    Redo,
    /// the one command that writes back into the keybind system.
    SetVar {
        name: String,
        value: ExprField,
    },
}
impl BindCommand {
    fn kind(&self) -> CommandKind {
        match self {
            BindCommand::Twist { .. } => CommandKind::Twist,
            BindCommand::Reorient { .. } => CommandKind::Reorient,
            BindCommand::Align => CommandKind::Align,
            BindCommand::Undo => CommandKind::Undo,
            BindCommand::Redo => CommandKind::Redo,
            BindCommand::SetVar { .. } => CommandKind::SetVar,
        }
    }

    /// unevaluated one-liner for the collapsed header.
    fn summary(&self) -> String {
        match self {
            BindCommand::Twist {
                grip,
                layers,
                multiplicity,
                invert,
            } => format!(
                "twist {} {} {} (invert {})",
                grip.src(),
                layers.src(),
                multiplicity.src(),
                invert.src()
            ),
            BindCommand::Reorient {
                grip,
                multiplicity,
                invert,
            } => format!(
                "reorient {} {} (invert {})",
                grip.src(),
                multiplicity.src(),
                invert.src()
            ),
            BindCommand::Align => "align".to_string(),
            BindCommand::Undo => "undo".to_string(),
            BindCommand::Redo => "redo".to_string(),
            BindCommand::SetVar { name, value } => format!("set {name} = {}", value.src()),
        }
    }

    /// evaluate the arguments into something ready to execute, plus how to
    /// describe it in the resolution set.
    fn eval(&self, env: &dyn expr::Env) -> Result<(String, Action), String> {
        Ok(match self {
            BindCommand::Twist {
                grip,
                layers,
                multiplicity,
                invert,
            } => {
                let side = grip.eval(env)?.grip()?.ok_or("the grip is null")?;
                let layers = layers.eval(env)?.mask()?;
                let mut multiplicity = multiplicity.eval(env)?.multiplicity()?;
                if invert.eval(env)?.bool()? {
                    multiplicity = -multiplicity;
                }
                let twist = Twist {
                    side,
                    layers,
                    multiplicity,
                };
                (
                    twist.to_string(),
                    Action::Command(Command::Twist {
                        twist,
                        origin: Origin::User,
                    }),
                )
            }
            BindCommand::Reorient {
                grip,
                multiplicity,
                invert,
            } => {
                let side = grip.eval(env)?.grip()?.ok_or("the grip is null")?;
                let mut multiplicity = multiplicity.eval(env)?.multiplicity()?;
                if invert.eval(env)?.bool()? {
                    multiplicity = -multiplicity;
                }
                // a 45 deg reorientation has no coherent face-key mapping
                // afterward, so there is no such thing.
                if multiplicity % 2 != 0 {
                    return Err(format!(
                        "a reorientation is a whole number of quarter turns, \
                         so its multiplicity must be even; got {multiplicity}"
                    ));
                }
                let (axis, sign) = side.axis();
                (
                    notation::reorientation(side, multiplicity),
                    Action::Command(Command::Reorient {
                        reorientation: Reorientation::new(axis, sign * multiplicity),
                        origin: Origin::User,
                    }),
                )
            }
            BindCommand::Align => ("align".to_string(), Action::Command(Command::Align)),
            BindCommand::Undo => ("undo".to_string(), Action::Command(Command::Undo)),
            BindCommand::Redo => ("redo".to_string(), Action::Command(Command::Redo)),
            BindCommand::SetVar { name, value } => {
                let value = value.eval(env)?;
                (
                    format!("set {name} = {value}"),
                    Action::SetVar {
                        name: name.clone(),
                        value,
                    },
                )
            }
        })
    }
}

/// `BindCommand` without its arguments, for the command dropdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandKind {
    Twist,
    Reorient,
    Align,
    Undo,
    Redo,
    SetVar,
}
impl CommandKind {
    const ALL: [CommandKind; 6] = [
        CommandKind::Twist,
        CommandKind::Reorient,
        CommandKind::Align,
        CommandKind::Undo,
        CommandKind::Redo,
        CommandKind::SetVar,
    ];

    fn name(self) -> &'static str {
        match self {
            CommandKind::Twist => "twist",
            CommandKind::Reorient => "reorient",
            CommandKind::Align => "align",
            CommandKind::Undo => "undo",
            CommandKind::Redo => "redo",
            CommandKind::SetVar => "set variable",
        }
    }

    fn default_command(self) -> BindCommand {
        match self {
            CommandKind::Twist => twist_command("hovered_grip", "false"),
            CommandKind::Reorient => reorient_command("U", "false"),
            CommandKind::Align => BindCommand::Align,
            CommandKind::Undo => BindCommand::Undo,
            CommandKind::Redo => BindCommand::Redo,
            CommandKind::SetVar => BindCommand::SetVar {
                name: String::new(),
                value: ExprField::new("true"),
            },
        }
    }
}

/// what executing a fired binding does. Commands go out to the hub; a variable
/// write stays here.
#[derive(Debug, Clone)]
enum Action {
    Command(Command),
    SetVar { name: String, value: Value },
}

/// What one key of the keybind reference (or one twist gizmo) would do, from
/// [`Keybinds::preview`].
pub struct Preview {
    /// notation for a twist, a short phrase for anything else, `!` for a
    /// binding that fires but can't be evaluated.
    pub label: String,
    /// the enclosing folder's color.
    pub color: egui::Color32,
    /// "twists / key_r", for a tooltip.
    pub location: String,
    /// what it would run, for a caller that wants to ask more of it: the
    /// gizmos ask whether the twist is blocked.
    pub command: Option<Command>,
    pub error: Option<String>,
}

/// one binding that fired: it made it into the resolution set, whether or not
/// it was the one that executed.
#[derive(Debug, Clone)]
struct Fired {
    /// enclosing folder names, then the trigger.
    location: String,
    /// the innermost folder's color.
    color: egui::Color32,
    /// the evaluated command, or why it couldn't be evaluated.
    action: Result<(String, Action), String>,
}

/// "twists / key_r": where a binding lives, for the report and the reference's
/// tooltips.
fn location(path: &[&str], name: &str) -> String {
    let mut location = String::new();
    for folder in path {
        location.push_str(folder);
        location.push_str(" / ");
    }
    location.push_str(name);
    location
}

/// one walk of the folder tree.
struct Pass<'a> {
    env: Vars<'a>,
    prev: Vars<'a>,
    /// the resolution set, in tree order.
    fired: Vec<Fired>,
    /// guards that couldn't be evaluated. not fatal: an unevaluatable guard is
    /// false, so the binding just doesn't fire.
    errors: Vec<String>,
}
impl<'a> Pass<'a> {
    /// `path` is the stack of enclosing folder names. it stays a stack rather
    /// than a formatted string because the keybind reference runs a pass per
    /// key every frame, and almost none of them fire.
    fn walk(&mut self, nodes: &'a [Node], path: &mut Vec<&'a str>, color: egui::Color32) {
        for node in nodes {
            match node {
                Node::Folder(folder) => match folder.guard.eval_bool(&self.env) {
                    Ok(true) => {
                        path.push(&folder.name);
                        self.walk(&folder.children, path, folder.color);
                        path.pop();
                    }
                    Ok(false) => (),
                    Err(e) => {
                        let location = location(path, &folder.name);
                        self.errors.push(format!("{location}: guard: {e}"));
                    }
                },
                Node::Binding(binding) => self.binding(binding, path, color),
            }
        }
    }

    fn binding(&mut self, binding: &Binding, path: &[&str], color: egui::Color32) {
        match self.rising(&binding.trigger) {
            Ok(false) => return,
            Err(e) => {
                let location = location(path, &binding.trigger);
                return self.errors.push(format!("{location}: {e}"));
            }
            Ok(true) => (),
        }
        match binding.guard.eval_bool(&self.env) {
            Ok(false) => return,
            Err(e) => {
                let location = location(path, &binding.trigger);
                return self.errors.push(format!("{location}: guard: {e}"));
            }
            Ok(true) => (),
        }
        self.fired.push(Fired {
            location: location(path, &binding.trigger),
            color,
            action: binding.command.eval(&self.env),
        });
    }

    /// did the trigger variable go from not-true to true this pass?
    fn rising(&self, trigger: &str) -> Result<bool, String> {
        use expr::Env as _;

        let now = self
            .env
            .get(trigger)
            .ok_or_else(|| format!("unknown variable `{trigger}`"))?
            .bool()?;
        if !now {
            return Ok(false);
        }
        // While previewing, the only edge is the hypothetical one being asked
        // about. Everything else is steady however it got to be down —
        // otherwise a key pressed for real would answer every cell of the
        // reference with its own twist for that frame.
        if let Some((forced, _)) = self.env.forced {
            return Ok(forced == trigger);
        }
        // a rising edge is purely the variable going false -> true. a press
        // while it's already true (held, or clicked down in the reference) is
        // not an edge, whatever the physical key did.
        // a variable that didn't exist last pass counts as not-true.
        let was = self
            .prev
            .get(trigger)
            .is_some_and(|v| v == Value::Bool(true));
        Ok(!was)
    }
}

/// The keybind component: the variables, the folder tree, and one pass per
/// frame over both.
pub struct Keybinds {
    vars: Vec<UserVar>,
    root: Vec<Node>,
    /// the persistent input: keys, mouse, and modifiers held down, written by
    /// both the keyboard and the reference. read live by guards and arguments.
    input: InputState,
    /// last pass's input and variable values, for finding rising edges. also
    /// what previews are evaluated against.
    prev_input: InputState,
    prev_vars: HashMap<String, Value>,
    /// last frame's physical modifier state, so only real modifier *changes*
    /// are folded into `input` (leaving reference toggles alone).
    physical_modifiers: egui::Modifiers,
    /// the last *non-empty* resolution set: a pass that fired nothing would
    /// wipe the display a frame after the twist it explains.
    last_fired: Vec<Fired>,
    /// this pass's errors. static mistakes (an unknown trigger, a guard that
    /// doesn't typecheck) recur every pass, so they stay on screen.
    errors: Vec<String>,
}
impl Default for Keybinds {
    /// The default set, which is also the worked example of what the system
    /// can say. `default_multiplicity` is 1 — one 45 deg step, the puzzle's
    /// fundamental twist — in the direction a plain keypress and a left click
    /// twist, so `invert` reads as "the other way" everywhere.
    fn default() -> Self {
        let twists = Side::ALL
            .map(|side| {
                let key = format!("key_{}", format!("{side:?}").to_lowercase());
                binding(
                    &key,
                    "true",
                    twist_command(&format!("{side:?}"), "key_shift"),
                )
            })
            .to_vec();
        Self {
            vars: vec![
                UserVar {
                    name: "default_mask".to_string(),
                    value: Value::Mask(LayerMask::OUTER),
                    pinned: true,
                },
                UserVar {
                    name: "default_multiplicity".to_string(),
                    value: Value::Multiplicity(1),
                    pinned: false,
                },
            ],
            root: vec![
                folder("twists", egui::Color32::from_rgb(120, 170, 255), twists),
                folder(
                    "reorientations",
                    egui::Color32::from_rgb(130, 210, 130),
                    vec![
                        binding("key_x", "true", reorient_command("R", "key_shift")),
                        binding("key_y", "true", reorient_command("U", "key_shift")),
                        // cmd+z is undo, so plain z is the only z left to
                        // reorient with.
                        binding("key_z", "!key_command", reorient_command("F", "key_shift")),
                    ],
                ),
                folder(
                    "view",
                    egui::Color32::from_rgb(230, 200, 120),
                    vec![binding("key_space", "true", BindCommand::Align)],
                ),
                folder(
                    "history",
                    egui::Color32::from_rgb(200, 150, 230),
                    vec![
                        binding("key_z", "key_command && !key_shift", BindCommand::Undo),
                        binding("key_z", "key_command && key_shift", BindCommand::Redo),
                    ],
                ),
                folder(
                    "mouse",
                    egui::Color32::from_rgb(240, 160, 110),
                    vec![
                        // the gizmo under the pointer is a grip like any
                        // other; shift-hover reports none, which is what keeps
                        // piece selection from twisting.
                        binding(
                            "mouse_left",
                            "hovered_grip != null",
                            twist_command("hovered_grip", "hovered_grip_inverted"),
                        ),
                        binding(
                            "mouse_right",
                            "hovered_grip != null",
                            twist_command("hovered_grip", "!hovered_grip_inverted"),
                        ),
                    ],
                ),
            ],
            input: InputState::default(),
            prev_input: InputState::default(),
            prev_vars: HashMap::new(),
            physical_modifiers: egui::Modifiers::default(),
            last_fired: Vec::new(),
            errors: Vec::new(),
        }
    }
}
impl Keybinds {
    /// One pass: fold this frame's input into the persistent state, walk the
    /// tree, and execute the resolution set. Only the first command executes
    /// for now, which also enforces the speedsolving rule of at most one twist
    /// per pass.
    pub fn collect(&mut self, ctx: &egui::Context, input: &InputContext) -> Vec<Command> {
        self.apply_input(ctx, input);
        self.run_pass()
    }

    /// Fold this frame's physical input into the persistent state. Keys and
    /// mouse buttons come from events, so a bit a reference click set isn't
    /// clobbered by re-sampling; modifiers come from the change in egui's
    /// authoritative state (see [`apply_modifier_delta`]).
    fn apply_input(&mut self, ctx: &egui::Context, context: &InputContext) {
        self.input.pressed.clear();
        self.input.mouse_pressed = [false; 3];
        self.input.hovered_grip = context.hovered_grip;
        self.input.hovered_grip_inverted = context.hovered_grip_inverted;

        // `egui_wants_keyboard_input` reads the input lock itself, so sample it
        // before taking that lock below.
        let typing = ctx.egui_wants_keyboard_input();
        ctx.input(|i| {
            // while a text field has focus, or the window is in the background,
            // everything reads as up: typing must never fire a binding, and
            // nothing stays stuck after a release the window never saw.
            if typing || !i.focused {
                self.input.keys.clear();
                self.input.mouse = [false; 3];
                self.input.modifiers = egui::Modifiers::default();
                self.physical_modifiers = i.modifiers;
                return;
            }
            for event in &i.events {
                match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        // auto-repeat isn't a new press.
                        repeat: false,
                        ..
                    } => self.input.press(*key),
                    egui::Event::Key {
                        key,
                        pressed: false,
                        ..
                    } => self.input.release(*key),
                    egui::Event::PointerButton {
                        button, pressed, ..
                    } => {
                        if let Some(idx) = mouse_index(*button) {
                            if *pressed {
                                self.input.mouse_press(idx);
                            } else {
                                self.input.mouse[idx] = false;
                            }
                        }
                    }
                    _ => {}
                }
            }
            apply_modifier_delta(&mut self.input.modifiers, self.physical_modifiers, i.modifiers);
            self.physical_modifiers = i.modifiers;
        });
    }

    /// Walk the tree against the current persistent input, execute the first
    /// fired command, and roll the state forward so next pass can find edges.
    fn run_pass(&mut self) -> Vec<Command> {
        let now_vars: HashMap<String, Value> = self
            .vars
            .iter()
            .map(|var| (var.name.clone(), var.value))
            .collect();

        let (fired, mut errors) = self.resolve(
            Vars::new(&self.input, UserVars::Live(&self.vars)),
            Vars::new(&self.prev_input, UserVars::Snapshot(&self.prev_vars)),
        );

        let mut commands = Vec::new();
        match fired.first().map(|f| f.action.clone()) {
            Some(Ok((_, Action::Command(command)))) => commands.push(command),
            Some(Ok((_, Action::SetVar { name, value }))) => {
                if let Err(e) = self.set_var(&name, value) {
                    errors.push(e);
                }
            }
            // the failure is already reported next to the binding in the
            // resolution set.
            Some(Err(_)) | None => (),
        }

        // the pre-execution values: a variable a binding just set reads as a
        // rising edge next pass, so bindings can chain.
        self.prev_input = self.input.clone();
        self.prev_vars = now_vars;
        if !fired.is_empty() {
            self.last_fired = fired;
        }
        self.errors = errors;
        commands
    }

    /// One walk of the tree against a given view of the variables. Pure: both
    /// the real pass and the reference's hypothetical ones go through here,
    /// and neither changes anything.
    fn resolve(&self, env: Vars<'_>, prev: Vars<'_>) -> (Vec<Fired>, Vec<String>) {
        let mut pass = Pass {
            env,
            prev,
            fired: Vec::new(),
            errors: Vec::new(),
        };
        pass.walk(&self.root, &mut Vec::new(), ROOT_COLOR);
        (pass.fired, pass.errors)
    }

    /// What the pass would do if `variable` were pressed right now — the label
    /// for one key of the reference, or the twist a gizmo click would input.
    /// Nothing is executed and nothing changes.
    ///
    /// Everything else keeps the state of the last real pass, so held
    /// modifiers and held variables are accounted for. `hovered_grip` asks the
    /// hypothetical "if the pointer were over this gizmo"; `None` uses
    /// wherever the pointer really is.
    pub fn preview(&self, variable: &str, hovered_grip: Option<Side>) -> Option<Preview> {
        let vars = |forced| Vars {
            input: &self.prev_input,
            // the variables as they are now, not as the last pass left them:
            // the reference should answer for the state you can see.
            user: UserVars::Live(&self.vars),
            forced: Some((variable, forced)),
            hovered_grip,
        };
        // forcing the variable off in the previous pass is what makes this a
        // press: it rises even if the key is genuinely held down already.
        let (fired, _) = self.resolve(vars(true), vars(false));
        let fired = fired.into_iter().next()?;
        Some(match fired.action {
            Ok((label, action)) => Preview {
                label,
                color: fired.color,
                location: fired.location,
                command: match action {
                    Action::Command(command) => Some(command),
                    Action::SetVar { .. } => None,
                },
                error: None,
            },
            Err(e) => Preview {
                label: "!".to_string(),
                color: fired.color,
                location: fired.location,
                command: None,
                error: Some(e),
            },
        })
    }

    /// Is this variable down — clicked down in the reference or pressed for
    /// real? There's one state, and bindings can't tell the difference. Read
    /// from the live persistent input, so a click made this frame (the
    /// reference draws before the pass runs) is answered immediately.
    pub fn is_down(&self, variable: &str) -> bool {
        read(variable, &self.input) == Some(Value::Bool(true))
    }

    /// Toggle a variable from the reference: a key that's down goes up and
    /// vice versa, however it got that way. The next pass sees the rising edge
    /// and the binding fires, and a later physical release clears the same bit.
    pub fn toggle(&mut self, variable: &str) {
        let down = self.is_down(variable);
        self.set_down(variable, !down);
    }

    /// Write a builtin bool into the persistent input. Keys, mouse buttons, and
    /// modifiers all live there, so a reference click and a keypress land in
    /// the same place. Names that aren't a toggleable builtin are ignored.
    fn set_down(&mut self, variable: &str, down: bool) {
        match variable {
            "mouse_left" => self.input.mouse[0] = down,
            "mouse_right" => self.input.mouse[1] = down,
            "mouse_middle" => self.input.mouse[2] = down,
            "key_shift" => self.input.modifiers.shift = down,
            "key_ctrl" => self.input.modifiers.ctrl = down,
            "key_alt" => self.input.modifiers.alt = down,
            "key_command" => self.input.modifiers.command = down,
            _ => {
                if let Some(key) = variable.strip_prefix("key_").and_then(key_by_name) {
                    if down {
                        self.input.keys.insert(key);
                    } else {
                        self.input.keys.remove(&key);
                    }
                }
            }
        }
    }

    /// variables are typed, so a set that doesn't match is a mistake worth
    /// reporting rather than a silent retype.
    fn set_var(&mut self, name: &str, value: Value) -> Result<(), String> {
        let var = self
            .vars
            .iter_mut()
            .find(|var| var.name == name)
            .ok_or_else(|| format!("no variable named `{name}`"))?;
        if var.value.ty() != value.ty() {
            return Err(format!(
                "`{name}` is a {}, not a {}",
                var.value.ty().name(),
                value.ty().name()
            ));
        }
        var.value = value;
        Ok(())
    }

    fn var(&self, name: &str) -> Option<Value> {
        self.vars
            .iter()
            .find(|var| var.name == name)
            .map(|var| var.value)
    }

    pub fn var_mut(&mut self, name: &str) -> Option<&mut Value> {
        self.vars
            .iter_mut()
            .find(|var| var.name == name)
            .map(|var| &mut var.value)
    }

    /// The mask the twist gizmos preview and the manual twist buttons use:
    /// whatever `default_mask` holds, since that's what the default bindings
    /// twist. Falls back to the outer layer if it's gone or retyped.
    pub fn default_mask(&self) -> LayerMask {
        match self.var("default_mask") {
            Some(Value::Mask(mask)) => mask,
            _ => LayerMask::OUTER,
        }
    }

    pub fn has_pinned(&self) -> bool {
        self.vars.iter().any(|var| var.pinned)
    }

    /// the bottom bar: pinned variables, toggleable without opening the tab.
    pub fn pinned_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            for var in &mut self.vars {
                if !var.pinned {
                    continue;
                }
                // ids come from the name, so two pinned grips don't share a
                // dropdown.
                ui.push_id(&var.name, |ui| match &mut var.value {
                    Value::Bool(b) => {
                        if ui.selectable_label(*b, &var.name).clicked() {
                            *b = !*b;
                        }
                    }
                    value => {
                        ui.label(format!("{}:", var.name));
                        ui_value(ui, value);
                    }
                });
                ui.separator();
            }
        });
    }

    /// `reference_settings` draws the keybind reference's own settings, which
    /// belong in this tab even though the reference is drawn elsewhere. The
    /// two components stay independent; the hub hands one to the other.
    pub fn ui(&mut self, ui: &mut egui::Ui, reference_settings: impl FnOnce(&mut egui::Ui)) {
        ui.heading("keybinds");
        ui.separator();
        reference_settings(ui);
        ui.separator();
        self.ui_variables(ui);
        ui.separator();
        ui.strong("bindings");
        // disjoint fields: the tree is edited while the variable list is
        // read, so a trigger can be checked against the declarations.
        let Self { vars, root, .. } = self;
        ui_nodes(ui, root, vars);
        ui.separator();
        self.ui_report(ui);
    }

    fn ui_variables(&mut self, ui: &mut egui::Ui) {
        ui.strong("variables");
        let mut remove = None;
        for (idx, var) in self.vars.iter_mut().enumerate() {
            ui.push_id(("var", idx), |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(var.pinned, "📌")
                        .on_hover_text("pin to the bottom bar")
                        .clicked()
                    {
                        var.pinned = !var.pinned;
                    }
                    ui.add(egui::TextEdit::singleline(&mut var.name).desired_width(110.0));
                    let ty = var.value.ty();
                    egui::ComboBox::from_id_salt("type")
                        .width(70.0)
                        .selected_text(ty.name())
                        .show_ui(ui, |ui| {
                            for other in Type::ALL {
                                if ui.selectable_label(other == ty, other.name()).clicked()
                                    && other != ty
                                {
                                    var.value = other.default_value();
                                }
                            }
                        });
                    ui_value(ui, &mut var.value);
                    if ui.button("🗑️").clicked() {
                        remove = Some(idx);
                    }
                });
            });
        }
        if let Some(idx) = remove {
            self.vars.remove(idx);
        }
        if ui.button("+ variable").clicked() {
            self.vars.push(UserVar {
                name: format!("var{}", self.vars.len()),
                value: Value::Bool(false),
                pinned: false,
            });
        }

        let state = CollapsingState::load_with_default_open(
            ui.ctx(),
            ui.make_persistent_id("builtins"),
            false,
        );
        state
            .show_header(ui, |ui| {
                ui.label("builtin variables");
            })
            .body(|ui| {
                // one pass stale: the sidebar is drawn before the pass that
                // samples the input.
                for (name, _) in BUILTINS {
                    let value = read(name, &self.prev_input)
                        .map_or_else(|| "?".to_string(), |value| value.to_string());
                    ui.label(format!("{name} = {value}"));
                }
                let held = self
                    .prev_input
                    .keys
                    .iter()
                    .map(|key| format!("key_{}", format!("{key:?}").to_lowercase()))
                    .collect::<Vec<_>>()
                    .join(" ");
                ui.label(format!("key_<name> for every key; held: {held}"));
            });
    }

    /// the resolution set of the last pass that fired anything, plus any
    /// standing errors.
    fn ui_report(&mut self, ui: &mut egui::Ui) {
        ui.strong("last resolution set");
        if self.last_fired.is_empty() {
            ui.weak("nothing has fired yet");
        }
        for (idx, fired) in self.last_fired.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.colored_label(fired.color, &fired.location);
                match &fired.action {
                    Ok((desc, _)) if idx == 0 => {
                        ui.label(desc);
                    }
                    // legal, but for now only the first command executes.
                    Ok((desc, _)) => {
                        ui.weak(format!("{desc} (not executed)"));
                    }
                    Err(e) => {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    }
                }
            });
        }
        for error in &self.errors {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }
}

// ---- default-set builders ----

fn folder(name: &str, color: egui::Color32, children: Vec<Node>) -> Node {
    Node::Folder(Folder {
        name: name.to_string(),
        color,
        guard: ExprField::new("true"),
        children,
    })
}

fn binding(trigger: &str, guard: &str, command: BindCommand) -> Node {
    Node::Binding(Binding {
        trigger: trigger.to_string(),
        guard: ExprField::new(guard),
        command,
    })
}

fn twist_command(grip: &str, invert: &str) -> BindCommand {
    BindCommand::Twist {
        grip: ExprField::new(grip),
        layers: ExprField::new("default_mask"),
        multiplicity: ExprField::new("default_multiplicity"),
        invert: ExprField::new(invert),
    }
}

fn reorient_command(grip: &str, invert: &str) -> BindCommand {
    BindCommand::Reorient {
        grip: ExprField::new(grip),
        // a quarter turn; reorientations can't be 45 deg.
        multiplicity: ExprField::new("2"),
        invert: ExprField::new(invert),
    }
}

// ---- widgets ----

/// editor for a variable's live value, by type.
pub fn ui_value(ui: &mut egui::Ui, value: &mut Value) {
    match value {
        Value::Bool(b) => {
            ui.checkbox(b, "");
        }
        Value::Grip(grip) => {
            egui::ComboBox::from_id_salt("grip")
                .width(50.0)
                .selected_text(match grip {
                    Some(side) => format!("{side:?}"),
                    None => "null".to_string(),
                })
                .show_ui(ui, |ui| {
                    if ui.selectable_label(grip.is_none(), "null").clicked() {
                        *grip = None;
                    }
                    for side in Side::ALL {
                        if ui
                            .selectable_label(*grip == Some(side), format!("{side:?}"))
                            .clicked()
                        {
                            *grip = Some(side);
                        }
                    }
                });
        }
        Value::Mask(mask) => {
            // layers are numbered from 1 for the user; layer 1 is the one
            // touching the twisted side.
            for layer in 0..LayerMask::N_LAYERS {
                let mut on = mask.contains(layer);
                if ui
                    .selectable_label(on, format!("{}", layer + 1))
                    .on_hover_text(format!("layer {}", layer + 1))
                    .clicked()
                {
                    on = !on;
                    mask.set(layer, on);
                }
            }
        }
        Value::Multiplicity(m) => {
            ui.add(egui::DragValue::new(m).range(-8..=8).speed(0.1));
        }
    }
}

/// text field for one expression, with its parse error under it.
fn ui_expr(ui: &mut egui::Ui, label: &str, field: &mut ExprField) {
    ui.horizontal(|ui| {
        ui.label(label);
        let response = ui.add(
            egui::TextEdit::singleline(field.src_mut())
                .desired_width(150.0)
                .font(egui::TextStyle::Monospace),
        );
        if response.changed() {
            field.reparse();
        }
    });
    if let Some(error) = field.error() {
        ui.colored_label(ui.visuals().error_fg_color, error);
    }
}

fn ui_nodes(ui: &mut egui::Ui, nodes: &mut Vec<Node>, vars: &[UserVar]) {
    let mut remove = None;
    for (idx, node) in nodes.iter_mut().enumerate() {
        ui.push_id(("node", idx), |ui| {
            let deleted = match node {
                Node::Folder(folder) => ui_folder(ui, folder, vars),
                Node::Binding(binding) => ui_binding(ui, binding, vars),
            };
            if deleted {
                remove = Some(idx);
            }
        });
    }
    if let Some(idx) = remove {
        nodes.remove(idx);
    }
    ui.horizontal(|ui| {
        if ui.button("+ folder").clicked() {
            nodes.push(folder("folder", ROOT_COLOR, Vec::new()));
        }
        if ui.button("+ binding").clicked() {
            nodes.push(binding(
                "key_a",
                "true",
                CommandKind::Twist.default_command(),
            ));
        }
    });
}

/// returns whether the folder was deleted.
fn ui_folder(ui: &mut egui::Ui, folder: &mut Folder, vars: &[UserVar]) -> bool {
    let mut deleted = false;
    let state =
        CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id("folder"), true);
    let header = state.show_header(ui, |ui| {
        ui.color_edit_button_srgba(&mut folder.color);
        ui.add(
            egui::TextEdit::singleline(&mut folder.name)
                .desired_width(110.0)
                .text_color(folder.color),
        );
        if ui.button("🗑️").clicked() {
            deleted = true;
        }
    });
    header.body(|ui| {
        ui_expr(ui, "guard", &mut folder.guard);
        ui_nodes(ui, &mut folder.children, vars);
    });
    deleted
}

/// returns whether the binding was deleted.
fn ui_binding(ui: &mut egui::Ui, binding: &mut Binding, vars: &[UserVar]) -> bool {
    let mut deleted = false;
    let state =
        CollapsingState::load_with_default_open(ui.ctx(), ui.make_persistent_id("binding"), false);
    let header = state.show_header(ui, |ui| {
        ui.label(format!(
            "{} → {}",
            binding.trigger,
            binding.command.summary()
        ));
        if ui.button("🗑️").clicked() {
            deleted = true;
        }
    });
    header.body(|ui| {
        ui.horizontal(|ui| {
            ui.label("on")
                .on_hover_text("the boolean variable whose rising edge fires this");
            ui.add(
                egui::TextEdit::singleline(&mut binding.trigger)
                    .desired_width(150.0)
                    .font(egui::TextStyle::Monospace),
            );
        });
        // a trigger nobody declared never fires, and nothing else would say so
        // until you pressed the key and nothing happened.
        if !is_known(&binding.trigger, vars) {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("no variable named `{}`", binding.trigger),
            );
        }
        ui_expr(ui, "guard", &mut binding.guard);
        ui.horizontal(|ui| {
            ui.label("do");
            let kind = binding.command.kind();
            egui::ComboBox::from_id_salt("kind")
                .selected_text(kind.name())
                .show_ui(ui, |ui| {
                    for other in CommandKind::ALL {
                        if ui.selectable_label(other == kind, other.name()).clicked()
                            && other != kind
                        {
                            binding.command = other.default_command();
                        }
                    }
                });
        });
        match &mut binding.command {
            BindCommand::Twist {
                grip,
                layers,
                multiplicity,
                invert,
            } => {
                ui_expr(ui, "grip", grip);
                ui_expr(ui, "layers", layers);
                ui_expr(ui, "multiplicity", multiplicity);
                ui_expr(ui, "invert", invert);
            }
            BindCommand::Reorient {
                grip,
                multiplicity,
                invert,
            } => {
                ui_expr(ui, "grip", grip);
                ui_expr(ui, "multiplicity", multiplicity);
                ui_expr(ui, "invert", invert);
            }
            BindCommand::Align | BindCommand::Undo | BindCommand::Redo => (),
            BindCommand::SetVar { name, value } => {
                ui.horizontal(|ui| {
                    ui.label("variable");
                    ui.add(egui::TextEdit::singleline(name).desired_width(150.0));
                });
                ui_expr(ui, "value", value);
            }
        }
    });
    deleted
}

#[cfg(test)]
mod tests {
    use super::*;

    /// stage one frame: clear the per-pass press marks, let `setup` mutate the
    /// persistent input the way this frame's events (or a reference click)
    /// would, then run the pass — exactly what `collect` does once it has
    /// applied egui's events.
    fn frame(keybinds: &mut Keybinds, setup: impl FnOnce(&mut InputState)) -> Vec<Command> {
        keybinds.input.pressed.clear();
        keybinds.input.mouse_pressed = [false; 3];
        setup(&mut keybinds.input);
        keybinds.run_pass()
    }

    /// the default variables, but a hand-written binding tree.
    fn keybinds_with(root: Vec<Node>) -> Keybinds {
        Keybinds {
            root,
            ..Keybinds::default()
        }
    }

    #[test]
    fn a_held_key_twists_once() {
        let mut keybinds = Keybinds::default();
        let commands = frame(&mut keybinds, |i| i.press(egui::Key::R));
        assert!(matches!(
            commands[..],
            [Command::Twist {
                twist: Twist {
                    side: Side::R,
                    layers: LayerMask::OUTER,
                    multiplicity: 1,
                },
                ..
            }]
        ));
        // still held: no new edge, no repeat.
        assert!(frame(&mut keybinds, |_| {}).is_empty());
        // released and pressed again: another twist.
        assert!(frame(&mut keybinds, |i| i.release(egui::Key::R)).is_empty());
        assert_eq!(frame(&mut keybinds, |i| i.press(egui::Key::R)).len(), 1);
    }

    #[test]
    fn a_tap_too_short_to_span_a_frame_still_fires() {
        let mut keybinds = Keybinds::default();
        // press and release inside one frame: the key is down for that frame
        // (a real false -> true across the boundary), so the twist isn't lost.
        assert_eq!(
            frame(&mut keybinds, |i| {
                i.press(egui::Key::R);
                i.release(egui::Key::R);
            })
            .len(),
            1
        );
        // the next frame clears the press mark: nothing stays held, and it
        // doesn't fire a second time.
        assert!(frame(&mut keybinds, |_| {}).is_empty());
        assert!(!keybinds.is_down("key_r"));
    }

    #[test]
    fn shift_inverts_through_the_invert_expression() {
        let mut keybinds = Keybinds::default();
        let commands = frame(&mut keybinds, |i| {
            i.modifiers.shift = true;
            i.press(egui::Key::R);
        });
        assert!(matches!(
            commands[..],
            [Command::Twist {
                twist: Twist {
                    multiplicity: -1,
                    ..
                },
                ..
            }]
        ));
    }

    #[test]
    fn guards_dont_fire_on_their_own_edge() {
        // press r with the guard false...
        let mut keybinds = keybinds_with(vec![binding(
            "key_r",
            "toggle",
            twist_command("R", "false"),
        )]);
        keybinds.vars.push(UserVar {
            name: "toggle".to_string(),
            value: Value::Bool(false),
            pinned: false,
        });
        assert!(frame(&mut keybinds, |i| i.press(egui::Key::R)).is_empty());
        // ...then make the guard true while r is still held. the guard's own
        // edge is not the trigger's, so nothing fires.
        *keybinds.var_mut("toggle").unwrap() = Value::Bool(true);
        assert!(frame(&mut keybinds, |_| {}).is_empty());
    }

    #[test]
    fn the_mouse_twists_the_hovered_grip() {
        let mut keybinds = Keybinds::default();
        // hovering without pressing does nothing.
        assert!(frame(&mut keybinds, |i| i.hovered_grip = Some(Side::U)).is_empty());
        let commands = frame(&mut keybinds, |i| {
            i.hovered_grip = Some(Side::U);
            i.mouse_press(0);
        });
        assert!(matches!(
            commands[..],
            [Command::Twist {
                twist: Twist {
                    side: Side::U,
                    multiplicity: 1,
                    ..
                },
                ..
            }]
        ));
        // pressing with no grip under the pointer is guarded out.
        let mut keybinds = Keybinds::default();
        assert!(frame(&mut keybinds, |i| i.mouse_press(0)).is_empty());
    }

    #[test]
    fn the_default_mask_variable_drives_the_twist() {
        let mut keybinds = Keybinds::default();
        *keybinds.var_mut("default_mask").unwrap() = Value::Mask(LayerMask(0b011));
        let commands = frame(&mut keybinds, |i| i.press(egui::Key::F));
        assert!(matches!(
            commands[..],
            [Command::Twist {
                twist: Twist {
                    layers: LayerMask(0b011),
                    ..
                },
                ..
            }]
        ));
    }

    #[test]
    fn preview_labels_a_key_without_doing_anything() {
        let mut keybinds = Keybinds::default();
        // the reference asks this of every key, every frame.
        let preview = keybinds.preview("key_r", None).expect("r is bound");
        assert_eq!(preview.label, "R/2");
        assert_eq!(preview.location, "twists / key_r");
        assert!(keybinds.preview("key_q", None).is_none());
        // nothing moved: a preview is a question, not a press.
        assert!(frame(&mut keybinds, |_| {}).is_empty());

        // it answers for the state as it is, so a held modifier changes it.
        frame(&mut keybinds, |i| i.modifiers.shift = true);
        assert_eq!(keybinds.preview("key_r", None).unwrap().label, "R/2'");
        // and a key already held down still previews, so the reference doesn't
        // blank out the key you're pressing. shift is persistent now, so
        // releasing it is its own event.
        frame(&mut keybinds, |i| {
            i.modifiers.shift = false;
            i.press(egui::Key::R);
        });
        assert_eq!(keybinds.preview("key_r", None).unwrap().label, "R/2");
    }

    #[test]
    fn preview_answers_what_clicking_a_gizmo_would_twist() {
        let keybinds = Keybinds::default();
        // the pointer is nowhere near a gizmo, so a click does nothing...
        assert!(keybinds.preview("mouse_left", None).is_none());
        // ...but the gizmos ask the hypothetical anyway.
        let preview = keybinds
            .preview("mouse_left", Some(Side::U))
            .expect("the default set twists on left click");
        assert_eq!(preview.label, "U/2");
        assert!(matches!(
            preview.command,
            Some(Command::Twist {
                twist: Twist { side: Side::U, .. },
                ..
            })
        ));
    }

    #[test]
    fn clicking_the_reference_toggles_a_key() {
        let mut keybinds = Keybinds::default();
        keybinds.toggle("key_r");
        // clicking it down is a press...
        assert_eq!(frame(&mut keybinds, |_| {}).len(), 1);
        assert!(keybinds.is_down("key_r"));
        // ...and staying down is not another one.
        assert!(frame(&mut keybinds, |_| {}).is_empty());
        // it reads as down everywhere, so a guard on it sees it too. previews
        // are against the last pass, which the app runs every frame.
        keybinds.toggle("key_shift");
        frame(&mut keybinds, |_| {});
        assert_eq!(keybinds.preview("key_u", None).unwrap().label, "U/2'");
        // clicking again releases it, and a release fires nothing.
        keybinds.toggle("key_r");
        assert!(!keybinds.is_down("key_r"));
        assert!(frame(&mut keybinds, |_| {}).is_empty());
    }

    #[test]
    fn a_physical_press_while_clicked_down_is_not_an_edge() {
        let mut keybinds = Keybinds::default();
        keybinds.toggle("key_r");
        assert_eq!(frame(&mut keybinds, |_| {}).len(), 1);

        // press it for real while it's already down: the variable was already
        // true, so there's no false -> true edge and nothing fires. an edge is
        // the variable's transition, not the physical key's.
        assert!(frame(&mut keybinds, |i| i.press(egui::Key::R)).is_empty());
        assert!(keybinds.is_down("key_r"));
        // releasing it clears the same bit the click set.
        assert!(frame(&mut keybinds, |i| i.release(egui::Key::R)).is_empty());
        assert!(!keybinds.is_down("key_r"));
        // now that it's up, a fresh press really is an edge.
        assert_eq!(frame(&mut keybinds, |i| i.press(egui::Key::R)).len(), 1);
    }

    #[test]
    fn a_real_press_doesnt_answer_for_other_keys() {
        let mut keybinds = Keybinds::default();
        // r is pressed for real this pass. asking what *q* would do must not
        // come back with r's twist, or the whole reference flashes at once.
        frame(&mut keybinds, |i| i.press(egui::Key::R));
        assert!(keybinds.preview("key_q", None).is_none());
        assert_eq!(keybinds.preview("key_r", None).unwrap().label, "R/2");
    }

    /// the editor is a lot of tree-walking UI; run it headlessly so a panic in
    /// it can't wait for someone to open the tab.
    #[test]
    fn the_editor_draws() {
        let mut keybinds = Keybinds::default();
        // something in the resolution set, so the report has rows to draw.
        frame(&mut keybinds, |i| i.press(egui::Key::R));
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            keybinds.ui(ui, |ui| {
                ui.label("the reference's settings go here");
            });
            keybinds.pinned_ui(ui);
        });
    }

    #[test]
    fn a_folder_guard_hides_its_bindings() {
        let mut keybinds = Keybinds::default();
        let Node::Folder(twists) = &mut keybinds.root[0] else {
            panic!("the first default folder is the twists");
        };
        twists.guard = ExprField::new("false");
        assert!(frame(&mut keybinds, |i| i.press(egui::Key::R)).is_empty());
    }

    #[test]
    fn setting_a_variable_is_a_command() {
        let mut keybinds = keybinds_with(vec![binding(
            "key_a",
            "true",
            BindCommand::SetVar {
                name: "default_mask".to_string(),
                value: ExprField::new("{1,2}"),
            },
        )]);
        frame(&mut keybinds, |i| i.press(egui::Key::A));
        assert_eq!(keybinds.default_mask(), LayerMask(0b011));
    }

    #[test]
    fn a_reorientation_cant_be_45_degrees() {
        let mut keybinds = keybinds_with(vec![binding(
            "key_a",
            "true",
            reorient_command("U", "false"),
        )]);
        let Node::Binding(b) = &mut keybinds.root[0] else {
            unreachable!()
        };
        let BindCommand::Reorient { multiplicity, .. } = &mut b.command else {
            unreachable!()
        };
        *multiplicity = ExprField::new("1");
        assert!(frame(&mut keybinds, |i| i.press(egui::Key::A)).is_empty());
        // and it says why, rather than failing silently.
        assert!(keybinds.last_fired[0].action.is_err());
    }
}
