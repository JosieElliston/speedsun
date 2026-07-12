use cgmath::Rotation3;

use crate::puzzle_state::{Rot, Twist, Vec3};

/// Who issued a command. Decides policy downstream: whether a twist animates
/// and whether it's recorded in the undo history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    User,
    Undo,
    Redo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}
impl Axis {
    pub fn unit(self) -> Vec3 {
        match self {
            Axis::X => Vec3::unit_x(),
            Axis::Y => Vec3::unit_y(),
            Axis::Z => Vec3::unit_z(),
        }
    }
}

/// A whole-puzzle rotation (of the puzzle state: every piece rotates).
/// `multiplicity` is in 45° increments, matching `Twist`'s convention, but it
/// must always be even: a 45°-rotated puzzle has no coherent face-key
/// mapping, so keybind/input handling for odd multiples is unsolved. (Kept
/// in 45° units rather than dividing by 2 so the two multiplicities stay
/// consistent.)
#[derive(Debug, Clone, Copy)]
pub struct Rotation {
    pub axis: Axis,
    pub multiplicity: i8,
}
impl Rotation {
    pub fn new(axis: Axis, multiplicity: i8) -> Self {
        debug_assert!(
            multiplicity % 2 == 0,
            "view rotations must be multiples of 90°"
        );
        Self { axis, multiplicity }
    }

    pub fn inv(self) -> Self {
        Self {
            axis: self.axis,
            multiplicity: -self.multiplicity,
        }
    }

    /// The rotation applied to the whole puzzle, matching `Twist`'s sign
    /// convention (multiplicity 1 = -45° about the axis).
    pub fn quat(self) -> Rot {
        let angle = -self.multiplicity as f32 * std::f32::consts::FRAC_PI_4;
        Rot::from_axis_angle(self.axis.unit(), cgmath::Rad(angle))
    }
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
    /// the nearest axis-aligned view orientation (recorded in the undo
    /// history), the view keeps only the sub-90° residual and snaps it to
    /// identity (animated). Visually net-zero at the instant it applies —
    /// afterward, face keybinds match what you see. The only command whose
    /// meaning depends on the view.
    Align,
    /// Compose a rotation onto the view without touching the puzzle state:
    /// the cosmetic compensation emitted when undoing/redoing an `Align`,
    /// keeping the re-basing visually net-zero.
    RotateView(Rot),
    Undo,
    Redo,
    /// Toggle a piece's membership in the view's selection.
    TogglePieceSelection(usize),
    ClearSelection,
}
