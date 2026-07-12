use cgmath::{InnerSpace, Rotation3};

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

/// A whole-puzzle rotation (of the puzzle state: every piece rotates). Holds
/// an arbitrary rotation: keybinds construct 90° multiples via `new`, and
/// Align records the (axis-aligned) re-basing it induced via `from_quat`.
#[derive(Debug, Clone, Copy)]
pub struct Rotation(Rot);
impl Rotation {
    /// `multiplicity` is in 45° increments, matching `Twist`'s convention
    /// (multiplicity 1 = -45° about the axis).
    pub fn new(axis: Axis, multiplicity: i8) -> Self {
        let angle = -multiplicity as f32 * std::f32::consts::FRAC_PI_4;
        Self::from_quat(Rot::from_axis_angle(axis.unit(), cgmath::Rad(angle)))
    }

    /// Canonicalized to the s >= 0 hemisphere (q and -q are the same
    /// rotation) so `axis_angle` animates the short way (<= 180°).
    pub fn from_quat(rot: Rot) -> Self {
        Self(if rot.s < 0.0 { -rot } else { rot })
    }

    pub fn inv(self) -> Self {
        // the conjugate of a unit quaternion is its inverse; s is unchanged,
        // so the result stays canonical.
        Self(self.0.conjugate())
    }

    /// the rotation as applied to the whole puzzle.
    pub fn quat(self) -> Rot {
        self.0
    }

    /// rotation axis (unit) and angle in radians, for pacing the animation.
    pub fn axis_angle(self) -> (Vec3, f32) {
        let sin_half = self.0.v.magnitude();
        if sin_half < 1e-9 {
            return (Vec3::unit_x(), 0.0);
        }
        (self.0.v / sin_half, 2.0 * sin_half.atan2(self.0.s))
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

#[cfg(test)]
mod tests {
    use cgmath::AbsDiffEq;

    use super::*;

    #[test]
    fn from_quat_canonicalizes_and_round_trips() {
        // 270° about X lands in the s < 0 hemisphere; canonicalization flips
        // it so the extracted animation runs 90° the short way.
        let q = Rot::from_axis_angle(Vec3::unit_x(), cgmath::Rad(1.5 * std::f32::consts::PI));
        assert!(q.s < 0.0);
        let r = Rotation::from_quat(q);
        assert!(r.quat().s >= 0.0);
        assert!(r.quat().abs_diff_eq(&-q, 1e-6));

        let (axis, angle) = r.axis_angle();
        assert!(angle <= std::f32::consts::PI + 1e-6);
        assert!(Rot::from_axis_angle(axis, cgmath::Rad(angle)).abs_diff_eq(&r.quat(), 1e-6));
    }
}
