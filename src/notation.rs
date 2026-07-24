//! How a twist is written down — in the keybind reference, in the resolution
//! set, and eventually in log files.
//!
//! speedsun only ever has three layers, so this uses the compact cuber
//! notation the whole way rather than the general layer-mask prefixes of
//! <https://hypercubing.xyz/drafts/hyper-puzzle-notation/>: `Rw`, not
//! `{1,2}R`; `M`, not `{2}L`. The two masks left over are written with that
//! notation's `~` (invert the layer mask) instead: `~M` and `~x`.
//!
//! A quarter turn is the unit, so the 45 deg jumbling twist is a half of one:
//! `R/2`, and multiplicity 3 is `R3/2`. An inverse primes the whole thing:
//! `R3/2'`.

use std::fmt;

use crate::puzzle_state::{Axis, LayerMask, Side, Twist};

impl fmt::Display for Twist {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = name(self.side, self.layers, self.multiplicity) {
            return write!(f, "{name}");
        }
        // The two masks with no name of their own are the complements of two
        // that have one, and `~` inverts a layer mask: {1,3}L is ~M, and the
        // empty twist is ~x.
        let complement = LayerMask(!self.layers.0 & LayerMask::ALL.0);
        let name = name(self.side, complement, self.multiplicity)
            .expect("every layer mask is named, or is the complement of one that is");
        write!(f, "~{name}")
    }
}

/// The standard name for this twist, or `None` for the two layer masks that
/// only have one via `~`.
fn name(side: Side, layers: LayerMask, multiplicity: i8) -> Option<String> {
    let (side, layers, multiplicity) = canonical(side, layers, multiplicity);
    Some(match layers.0 {
        0b001 => format!("{side:?}{}", amount(multiplicity)),
        0b011 => format!("{side:?}w{}", amount(multiplicity)),
        0b010 => format!("{}{}", slice_letter(side), amount(multiplicity)),
        0b111 => {
            // a twist of every layer is a whole-puzzle rotation, named after
            // the positive end of its axis.
            let (axis, sign) = side.axis();
            format!("{}{}", axis_letter(axis), amount(sign * multiplicity))
        }
        _ => return None,
    })
}

/// how a whole-puzzle reorientation is written: the same as a twist of every
/// layer about that grip, since that's the same motion.
pub fn reorientation(side: Side, multiplicity: i8) -> String {
    Twist {
        side,
        layers: LayerMask::ALL,
        multiplicity,
    }
    .to_string()
}

/// The same twist named from whichever side gives it a standard name: the far
/// layers of `R` are `L'`, and the middle one is `M'` — slices are named after
/// L, D and F. Naming it from the other side reverses the mask and the
/// direction.
fn canonical(side: Side, layers: LayerMask, multiplicity: i8) -> (Side, LayerMask, i8) {
    let flip = match layers.0 {
        // far-heavy: {3}R is L', {2,3}R is Lw'.
        0b100 | 0b110 => true,
        0b010 => !matches!(side, Side::L | Side::D | Side::F),
        // every layer is a rotation, whose sign `side.axis()` sorts out.
        _ => false,
    };
    if flip {
        (side.opposite(), layers.reversed(), -multiplicity)
    } else {
        (side, layers, multiplicity)
    }
}

/// M follows L, E follows D, S follows F; `canonical` has already flipped a
/// slice onto one of those.
fn slice_letter(side: Side) -> &'static str {
    match side {
        Side::L => "M",
        Side::D => "E",
        Side::F => "S",
        _ => unreachable!("`canonical` names a slice from L, D or F"),
    }
}

fn axis_letter(axis: Axis) -> &'static str {
    match axis {
        Axis::X => "x",
        Axis::Y => "y",
        Axis::Z => "z",
    }
}

/// Everything after the move letter. `multiplicity` counts 45 deg steps while
/// notation counts quarter turns, so it halves: 2 is the bare move, 4 is `2`,
/// and the odd ones keep a `/2`.
fn amount(multiplicity: i8) -> String {
    let prime = if multiplicity < 0 { "'" } else { "" };
    let steps = multiplicity.unsigned_abs();
    if steps.is_multiple_of(2) {
        match steps / 2 {
            1 => prime.to_string(),
            quarters => format!("{quarters}{prime}"),
        }
    } else {
        match steps {
            1 => format!("/2{prime}"),
            _ => format!("{steps}/2{prime}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn twist(side: Side, layers: u8, multiplicity: i8) -> String {
        Twist {
            side,
            layers: LayerMask(layers),
            multiplicity,
        }
        .to_string()
    }

    #[test]
    fn layer_masks_get_their_standard_names() {
        // a quarter turn of each mask on R, and the same mask on L.
        assert_eq!(twist(Side::R, 0b001, 2), "R");
        assert_eq!(twist(Side::L, 0b001, 2), "L");
        assert_eq!(twist(Side::R, 0b011, 2), "Rw");
        assert_eq!(twist(Side::L, 0b011, 2), "Lw");
        // the slice is named after L, D or F, so R's is M the other way.
        assert_eq!(twist(Side::L, 0b010, 2), "M");
        assert_eq!(twist(Side::R, 0b010, 2), "M'");
        assert_eq!(twist(Side::D, 0b010, 2), "E");
        assert_eq!(twist(Side::U, 0b010, 2), "E'");
        assert_eq!(twist(Side::F, 0b010, 2), "S");
        assert_eq!(twist(Side::B, 0b010, 2), "S'");
        // far layers are the other side's near layers, turning the other way.
        assert_eq!(twist(Side::R, 0b100, 2), "L'");
        assert_eq!(twist(Side::R, 0b110, 2), "Lw'");
        // every layer is a rotation, named after the positive axis end.
        assert_eq!(twist(Side::R, 0b111, 2), "x");
        assert_eq!(twist(Side::L, 0b111, 2), "x'");
        assert_eq!(twist(Side::U, 0b111, 2), "y");
        assert_eq!(twist(Side::D, 0b111, 2), "y'");
        assert_eq!(twist(Side::F, 0b111, 2), "z");
        assert_eq!(twist(Side::B, 0b111, 2), "z'");
        // the two masks with no name of their own are written as the inverse
        // of one that has: {1,3} is everything but the slice, {} is everything
        // but the whole puzzle.
        assert_eq!(twist(Side::L, 0b101, 2), "~M");
        assert_eq!(twist(Side::R, 0b101, 2), "~M'");
        assert_eq!(twist(Side::R, 0b000, 2), "~x");
        assert_eq!(twist(Side::L, 0b000, 2), "~x'");
    }

    #[test]
    fn every_twist_has_a_name() {
        // 6 sides x 8 masks x a few amounts: nothing panics, and no two masks
        // on a side ever collide.
        for side in Side::ALL {
            for multiplicity in [-4, -1, 1, 2, 3] {
                let mut names = Vec::new();
                for layers in 0..8 {
                    let name = twist(side, layers, multiplicity);
                    assert!(!name.is_empty());
                    names.push(name);
                }
                names.sort();
                let count = names.len();
                names.dedup();
                assert_eq!(names.len(), count, "two masks share a name: {names:?}");
            }
        }
    }

    #[test]
    fn multiplicity_counts_quarter_turns() {
        assert_eq!(twist(Side::R, 0b001, 2), "R");
        assert_eq!(twist(Side::R, 0b001, -2), "R'");
        assert_eq!(twist(Side::R, 0b001, 4), "R2");
        assert_eq!(twist(Side::R, 0b001, -4), "R2'");
        assert_eq!(twist(Side::R, 0b001, 6), "R3");
        // the jumbling twist is half a quarter turn.
        assert_eq!(twist(Side::R, 0b001, 1), "R/2");
        assert_eq!(twist(Side::R, 0b001, -1), "R/2'");
        assert_eq!(twist(Side::R, 0b001, 3), "R3/2");
        assert_eq!(twist(Side::R, 0b001, -3), "R3/2'");
    }

    #[test]
    fn the_amount_composes_with_every_name() {
        assert_eq!(twist(Side::R, 0b011, -3), "Rw3/2'");
        assert_eq!(twist(Side::L, 0b010, 1), "M/2");
        // flipping to name the twist negates the amount too.
        assert_eq!(twist(Side::R, 0b100, 1), "L/2'");
        assert_eq!(twist(Side::R, 0b111, -4), "x2'");
        assert_eq!(reorientation(Side::U, 2), "y");
        assert_eq!(reorientation(Side::F, -4), "z2'");
    }
}
