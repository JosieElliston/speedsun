use crate::puzzle_state::{Rotation, Twist};

/// Who issued a command. Decides policy downstream: whether a twist animates
/// and whether it's recorded in the undo history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    User,
    Undo,
    Redo,
}

/// The inter-component vocabulary: everything keybindable, loggable, or
/// crossing component boundaries is a `Command`, routed by the hub in
/// `App::ui`. (Component-local settings — sliders, checkboxes — mutate their
/// component directly instead.)
#[derive(Debug, Clone, Copy)]
pub enum Command {
    /// A twist, always in puzzle space. Keybinds are absolute (F is always
    /// `Side::F`); looking around with the mouse never changes what a key
    /// means. Only `Align` re-bases the state onto the view.
    Twist {
        twist: Twist,
        origin: Origin,
    },
    /// Rotate the whole puzzle state (every piece), animated through the
    /// twist queue like a twist of all layers; undoable.
    Rotate {
        rotation: Rotation,
        origin: Origin,
    },
    /// Make the puzzle state agree with the view: the state is rotated by
    /// the axis-aligned part of the home-relative view orientation, and the
    /// view snaps (animated) to the tuned home orientation (the view tab's
    /// home pitch/yaw sliders). Visually net-zero at the instant it applies
    /// — afterward, face keybinds match what you see. Recorded in the undo
    /// history as the induced rotation, so undoing past an align rotates
    /// the puzzle back in view like any other rotation. The only command
    /// whose meaning depends on the view.
    Align,
    Undo,
    Redo,
    /// Toggle a piece's membership in the view's selection.
    TogglePieceSelection(usize),
    ClearSelection,
}
