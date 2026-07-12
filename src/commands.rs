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

/// A whole-puzzle *view* rotation. `multiplicity` is in 45° increments,
/// matching `Twist`'s convention, but it must always be even: a 45°-rotated
/// view has no coherent face-key mapping, so keybind/input handling for odd
/// multiples is unsolved. (Kept in 45° units rather than dividing by 2 so the
/// two multiplicities stay consistent.)
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
    /// A twist, always in puzzle space: producers (keybinds, gizmo clicks)
    /// resolve view-relative input before emitting, so a future command log
    /// needs no mouse or view data.
    Twist { twist: Twist, origin: Origin },
    /// Rotate the whole puzzle (a view rotation, but logically part of the
    /// solve: it changes what later view-relative input means, and is
    /// undoable).
    Rotate { rotation: Rotation, origin: Origin },
    /// Snap the view to the nearest of the 24 axis-aligned orientations
    /// (animated). Cosmetic: not undoable, doesn't change the face-key
    /// mapping (which always goes through the nearest alignment).
    Align,
    Undo,
    Redo,
    /// Toggle a piece's membership in the view's selection.
    TogglePieceSelection(usize),
    ClearSelection,
}
