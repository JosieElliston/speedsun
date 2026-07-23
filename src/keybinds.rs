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

/// Every input the builtin variables read, sampled once per pass so a binding
/// can't see the state change halfway through, and kept until the next pass to
/// find rising edges.
#[derive(Default, Clone)]
struct InputSnapshot {
    keys: HashSet<egui::Key>,
    /// keys that got a press event this pass. a press is an edge even if the
    /// key already looked down: macOS drops key-up events while cmd is held,
    /// which would otherwise leave the key stuck and swallow the next press.
    pressed: HashSet<egui::Key>,
    modifiers: egui::Modifiers,
    /// left, right, middle.
    mouse: [bool; 3],
    hovered_grip: Option<Side>,
    hovered_grip_inverted: bool,
}
impl InputSnapshot {
    fn capture(ctx: &egui::Context, input: &InputContext) -> Self {
        // while a text field has focus, every key reads as up: typing must
        // never fire a binding, and a key held across the focus change ends
        // its press instead of firing on the way out.
        let typing = ctx.egui_wants_keyboard_input();
        ctx.input(|i| {
            let pressed: HashSet<egui::Key> = if typing {
                HashSet::new()
            } else {
                i.events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::Key {
                            key,
                            pressed: true,
                            // auto-repeat isn't a new press.
                            repeat: false,
                            ..
                        } => Some(*key),
                        _ => None,
                    })
                    .collect()
            };
            Self {
                // a key pressed and released inside one frame is still down
                // for that frame: at speedsolving speed a tap can fit between
                // two frames, and dropping it would drop the twist.
                keys: if typing {
                    HashSet::new()
                } else {
                    i.keys_down.union(&pressed).copied().collect()
                },
                pressed,
                modifiers: if typing {
                    egui::Modifiers::NONE
                } else {
                    i.modifiers
                },
                mouse: [
                    egui::PointerButton::Primary,
                    egui::PointerButton::Secondary,
                    egui::PointerButton::Middle,
                ]
                // same for a click too short to span a frame.
                .map(|button| i.pointer.button_down(button) || i.pointer.button_pressed(button)),
                hovered_grip: input.hovered_grip,
                hovered_grip_inverted: input.hovered_grip_inverted,
            }
        })
    }

    /// did this variable get a fresh press event this pass? only keys have
    /// one; see `pressed`.
    fn repressed(&self, name: &str) -> bool {
        name.strip_prefix("key_")
            .and_then(key_by_name)
            .is_some_and(|key| self.pressed.contains(&key))
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

/// egui exposes modifiers separately from keys and doesn't say which side was
/// pressed, so there's no `key_lshift` yet; `key_command` is cmd on macOS and
/// ctrl elsewhere, matching every other app on the platform.
fn builtin(name: &str, input: &InputSnapshot) -> Option<Value> {
    Some(match name {
        "hovered_grip" => Value::Grip(input.hovered_grip),
        "hovered_grip_inverted" => Value::Bool(input.hovered_grip_inverted),
        "mouse_left" => Value::Bool(input.mouse[0]),
        "mouse_right" => Value::Bool(input.mouse[1]),
        "mouse_middle" => Value::Bool(input.mouse[2]),
        "key_shift" => Value::Bool(input.modifiers.shift),
        "key_ctrl" => Value::Bool(input.modifiers.ctrl),
        "key_alt" => Value::Bool(input.modifiers.alt),
        "key_command" => Value::Bool(input.modifiers.command),
        _ => {
            let key = key_by_name(name.strip_prefix("key_")?)?;
            Value::Bool(input.keys.contains(&key))
        }
    })
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

/// variable lookup for one pass: builtins shadow user variables, so builtin
/// names are effectively reserved.
struct Vars<'a> {
    input: &'a InputSnapshot,
    user: &'a HashMap<String, Value>,
}
impl expr::Env for Vars<'_> {
    fn get(&self, name: &str) -> Option<Value> {
        builtin(name, self.input).or_else(|| self.user.get(name).copied())
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
                (
                    format!("twist {side:?} {layers} {multiplicity}"),
                    Action::Command(Command::Twist {
                        twist: Twist {
                            side,
                            layers,
                            multiplicity,
                        },
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
                    format!("reorient {side:?} {multiplicity}"),
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
impl Pass<'_> {
    fn walk(&mut self, nodes: &[Node], path: &str, color: egui::Color32) {
        for node in nodes {
            match node {
                Node::Folder(folder) => match folder.guard.eval_bool(&self.env) {
                    Ok(true) => self.walk(
                        &folder.children,
                        &format!("{path}{} / ", folder.name),
                        folder.color,
                    ),
                    Ok(false) => (),
                    Err(e) => self
                        .errors
                        .push(format!("{path}{}: guard: {e}", folder.name)),
                },
                Node::Binding(binding) => self.binding(binding, path, color),
            }
        }
    }

    fn binding(&mut self, binding: &Binding, path: &str, color: egui::Color32) {
        let location = format!("{path}{}", binding.trigger);
        match self.rising(&binding.trigger) {
            Ok(false) => return,
            Err(e) => return self.errors.push(format!("{location}: {e}")),
            Ok(true) => (),
        }
        match binding.guard.eval_bool(&self.env) {
            Ok(false) => return,
            Err(e) => return self.errors.push(format!("{location}: guard: {e}")),
            Ok(true) => (),
        }
        self.fired.push(Fired {
            location,
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
        // a variable that didn't exist last pass counts as not-true.
        let was = self
            .prev
            .get(trigger)
            .is_some_and(|v| v == Value::Bool(true));
        Ok(!was || self.env.input.repressed(trigger))
    }
}

/// The keybind component: the variables, the folder tree, and one pass per
/// frame over both.
pub struct Keybinds {
    vars: Vec<UserVar>,
    root: Vec<Node>,
    /// last pass's input and variable values, for finding rising edges.
    prev_input: InputSnapshot,
    prev_vars: HashMap<String, Value>,
    /// the last *non-empty* resolution set: a pass that fired nothing would
    /// wipe the display a frame after the twist it explains.
    last_fired: Vec<Fired>,
    /// this pass's errors. static mistakes (an unknown trigger, a guard that
    /// doesn't typecheck) recur every pass, so they stay on screen.
    errors: Vec<String>,
}
impl Default for Keybinds {
    /// The default set, which is also the worked example of what the system
    /// can say. Note `default_multiplicity` is -1: one 45 deg step in the
    /// direction a plain keypress and a left click twist, so `invert` reads as
    /// "the other way" everywhere.
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
                    value: Value::Multiplicity(-1),
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
            prev_input: InputSnapshot::default(),
            prev_vars: HashMap::new(),
            last_fired: Vec::new(),
            errors: Vec::new(),
        }
    }
}
impl Keybinds {
    /// One pass: sample the input, walk the tree, and execute the resolution
    /// set. Only the first command executes for now, which also enforces the
    /// speedsolving rule of at most one twist per pass.
    pub fn collect(&mut self, ctx: &egui::Context, input: &InputContext) -> Vec<Command> {
        let now_input = InputSnapshot::capture(ctx, input);
        let now_vars: HashMap<String, Value> = self
            .vars
            .iter()
            .map(|var| (var.name.clone(), var.value))
            .collect();

        let mut pass = Pass {
            env: Vars {
                input: &now_input,
                user: &now_vars,
            },
            prev: Vars {
                input: &self.prev_input,
                user: &self.prev_vars,
            },
            fired: Vec::new(),
            errors: Vec::new(),
        };
        pass.walk(&self.root, "", ROOT_COLOR);
        let Pass {
            fired, mut errors, ..
        } = pass;

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
        self.prev_input = now_input;
        self.prev_vars = now_vars;
        if !fired.is_empty() {
            self.last_fired = fired;
        }
        self.errors = errors;
        commands
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

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("keybinds");
        ui.separator();
        self.ui_variables(ui);
        ui.separator();
        ui.strong("bindings");
        ui_nodes(ui, &mut self.root);
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
                    let value = builtin(name, &self.prev_input)
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
            for layer in 0..LayerMask::N_LAYERS {
                let mut on = mask.contains(layer);
                if ui
                    .selectable_label(on, format!("{layer}"))
                    .on_hover_text(format!("layer {layer}"))
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

fn ui_nodes(ui: &mut egui::Ui, nodes: &mut Vec<Node>) {
    let mut remove = None;
    for (idx, node) in nodes.iter_mut().enumerate() {
        ui.push_id(("node", idx), |ui| {
            let deleted = match node {
                Node::Folder(folder) => ui_folder(ui, folder),
                Node::Binding(binding) => ui_binding(ui, binding),
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
fn ui_folder(ui: &mut egui::Ui, folder: &mut Folder) -> bool {
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
        ui_nodes(ui, &mut folder.children);
    });
    deleted
}

/// returns whether the binding was deleted.
fn ui_binding(ui: &mut egui::Ui, binding: &mut Binding) -> bool {
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

    /// drive a pass without egui: `input` is this frame's state.
    fn pass(keybinds: &mut Keybinds, input: InputSnapshot) -> Vec<Command> {
        let now_vars: HashMap<String, Value> = keybinds
            .vars
            .iter()
            .map(|var| (var.name.clone(), var.value))
            .collect();
        let mut pass = Pass {
            env: Vars {
                input: &input,
                user: &now_vars,
            },
            prev: Vars {
                input: &keybinds.prev_input,
                user: &keybinds.prev_vars,
            },
            fired: Vec::new(),
            errors: Vec::new(),
        };
        pass.walk(&keybinds.root, "", egui::Color32::WHITE);
        let Pass {
            fired, mut errors, ..
        } = pass;
        let mut commands = Vec::new();
        match fired.first().map(|f| f.action.clone()) {
            Some(Ok((_, Action::Command(command)))) => commands.push(command),
            Some(Ok((_, Action::SetVar { name, value }))) => {
                if let Err(e) = keybinds.set_var(&name, value) {
                    errors.push(e);
                }
            }
            Some(Err(_)) | None => (),
        }
        keybinds.prev_input = input;
        keybinds.prev_vars = now_vars;
        if !fired.is_empty() {
            keybinds.last_fired = fired;
        }
        keybinds.errors = errors;
        commands
    }

    /// the default variables, but a hand-written binding tree.
    fn keybinds_with(root: Vec<Node>) -> Keybinds {
        Keybinds {
            root,
            ..Keybinds::default()
        }
    }

    fn holding(keys: &[egui::Key]) -> InputSnapshot {
        InputSnapshot {
            keys: keys.iter().copied().collect(),
            ..InputSnapshot::default()
        }
    }

    fn hovering(grip: Side, left: bool) -> InputSnapshot {
        InputSnapshot {
            mouse: [left, false, false],
            hovered_grip: Some(grip),
            ..InputSnapshot::default()
        }
    }

    #[test]
    fn a_held_key_twists_once() {
        let mut keybinds = Keybinds::default();
        let commands = pass(&mut keybinds, holding(&[egui::Key::R]));
        assert!(matches!(
            commands[..],
            [Command::Twist {
                twist: Twist {
                    side: Side::R,
                    layers: LayerMask::OUTER,
                    multiplicity: -1,
                },
                ..
            }]
        ));
        // still held: no new edge, no repeat.
        assert!(pass(&mut keybinds, holding(&[egui::Key::R])).is_empty());
        // released and pressed again: another twist.
        assert!(pass(&mut keybinds, holding(&[])).is_empty());
        assert_eq!(pass(&mut keybinds, holding(&[egui::Key::R])).len(), 1);
    }

    /// what `capture` builds for a press: the key is down for this frame even
    /// if the release already arrived.
    fn tapping(key: egui::Key) -> InputSnapshot {
        InputSnapshot {
            keys: HashSet::from([key]),
            pressed: HashSet::from([key]),
            ..InputSnapshot::default()
        }
    }

    #[test]
    fn a_press_fires_even_when_the_key_never_looked_up() {
        let mut keybinds = Keybinds::default();
        assert_eq!(pass(&mut keybinds, tapping(egui::Key::R)).len(), 1);
        // a tap too short to span a frame, or a release macOS swallowed while
        // cmd was held: either way the press event is a new edge.
        assert_eq!(pass(&mut keybinds, tapping(egui::Key::R)).len(), 1);
        // merely staying down is not.
        assert!(pass(&mut keybinds, holding(&[egui::Key::R])).is_empty());
    }

    #[test]
    fn shift_inverts_through_the_invert_expression() {
        let mut keybinds = Keybinds::default();
        let shift = InputSnapshot {
            modifiers: egui::Modifiers::SHIFT,
            ..holding(&[egui::Key::R])
        };
        let commands = pass(&mut keybinds, shift);
        assert!(matches!(
            commands[..],
            [Command::Twist {
                twist: Twist {
                    multiplicity: 1,
                    ..
                },
                ..
            }]
        ));
    }

    #[test]
    fn guards_dont_fire_on_their_own_edge() {
        // hold r with the guard false...
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
        assert!(pass(&mut keybinds, holding(&[egui::Key::R])).is_empty());
        // ...then make the guard true while r is still held. the guard's own
        // edge is not the trigger's, so nothing fires.
        *keybinds.var_mut("toggle").unwrap() = Value::Bool(true);
        assert!(pass(&mut keybinds, holding(&[egui::Key::R])).is_empty());
    }

    #[test]
    fn the_mouse_twists_the_hovered_grip() {
        let mut keybinds = Keybinds::default();
        // hovering without pressing does nothing.
        assert!(pass(&mut keybinds, hovering(Side::U, false)).is_empty());
        let commands = pass(&mut keybinds, hovering(Side::U, true));
        assert!(matches!(
            commands[..],
            [Command::Twist {
                twist: Twist {
                    side: Side::U,
                    multiplicity: -1,
                    ..
                },
                ..
            }]
        ));
        // pressing with no grip under the pointer is guarded out.
        let mut keybinds = Keybinds::default();
        let clicking_nothing = InputSnapshot {
            mouse: [true, false, false],
            ..InputSnapshot::default()
        };
        assert!(pass(&mut keybinds, clicking_nothing).is_empty());
    }

    #[test]
    fn the_default_mask_variable_drives_the_twist() {
        let mut keybinds = Keybinds::default();
        *keybinds.var_mut("default_mask").unwrap() = Value::Mask(LayerMask(0b011));
        let commands = pass(&mut keybinds, holding(&[egui::Key::F]));
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

    /// the editor is a lot of tree-walking UI; run it headlessly so a panic in
    /// it can't wait for someone to open the tab.
    #[test]
    fn the_editor_draws() {
        let mut keybinds = Keybinds::default();
        // something in the resolution set, so the report has rows to draw.
        pass(&mut keybinds, holding(&[egui::Key::R]));
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            keybinds.ui(ui);
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
        assert!(pass(&mut keybinds, holding(&[egui::Key::R])).is_empty());
    }

    #[test]
    fn setting_a_variable_is_a_command() {
        let mut keybinds = keybinds_with(vec![binding(
            "key_a",
            "true",
            BindCommand::SetVar {
                name: "default_mask".to_string(),
                value: ExprField::new("{0,1}"),
            },
        )]);
        pass(&mut keybinds, holding(&[egui::Key::A]));
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
        assert!(pass(&mut keybinds, holding(&[egui::Key::A])).is_empty());
        // and it says why, rather than failing silently.
        assert!(keybinds.last_fired[0].action.is_err());
    }
}
