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

    /// every 90°-multiple rotation about one axis: the building blocks of
    /// `decompose`.
    fn all_single() -> impl Iterator<Item = Rotation> {
        [Axis::X, Axis::Y, Axis::Z]
            .into_iter()
            .flat_map(|axis| [-2, 2, 4].map(|multiplicity| Rotation { axis, multiplicity }))
    }

    /// Decompose one of the 24 axis-aligned orientations into the shortest
    /// sequence (at most two) of single-axis rotations whose in-order
    /// application composes to `rot`. Lets an Align record its induced
    /// rotation in the undo history as ordinary rotations.
    pub fn decompose(rot: Rot) -> Vec<Rotation> {
        // quaternions double-cover rotations: q and -q are the same rotation.
        let eq = |a: Rot, b: Rot| a.dot(b).abs() > 1.0 - 1e-4;
        if rot.s.abs() > 1.0 - 1e-4 {
            return vec![];
        }
        for r in Self::all_single() {
            if eq(r.quat(), rot) {
                return vec![r];
            }
        }
        for r1 in Self::all_single() {
            for r2 in Self::all_single() {
                // applying r1 then r2 composes to q(r2) · q(r1).
                if eq(r2.quat() * r1.quat(), rot) {
                    return vec![r1, r2];
                }
            }
        }
        unreachable!("not one of the 24 axis-aligned orientations: {rot:?}")
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
    /// the nearest axis-aligned view orientation, the view keeps only the
    /// sub-90° residual and snaps it to identity (animated). Visually
    /// net-zero at the instant it applies — afterward, face keybinds match
    /// what you see. Recorded in the undo history as the induced rotation(s)
    /// (`Rotation::decompose`), so undoing past an align rotates the puzzle
    /// back in view like any other rotation. The only command whose meaning
    /// depends on the view.
    Align,
    Undo,
    Redo,
    /// Toggle a piece's membership in the view's selection.
    TogglePieceSelection(usize),
    ClearSelection,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompose_covers_the_24_orientations() {
        let q = |axis: Axis| Rotation::new(axis, 2).quat();
        for i in 0..4 {
            for j in 0..4 {
                for k in 0..4 {
                    let mut rot = Rot::new(1.0, 0.0, 0.0, 0.0);
                    for _ in 0..i {
                        rot = q(Axis::X) * rot;
                    }
                    for _ in 0..j {
                        rot = q(Axis::Y) * rot;
                    }
                    for _ in 0..k {
                        rot = q(Axis::Z) * rot;
                    }
                    let parts = Rotation::decompose(rot);
                    assert!(parts.len() <= 2);
                    let mut recomposed = Rot::new(1.0, 0.0, 0.0, 0.0);
                    for part in parts {
                        recomposed = part.quat() * recomposed;
                    }
                    assert!(recomposed.dot(rot).abs() > 1.0 - 1e-4, "{rot:?}");
                }
            }
        }
    }
}
