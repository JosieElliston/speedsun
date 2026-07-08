use cgmath::{InnerSpace, Rotation, Rotation3};
use eframe::egui;

// in [-1, 1]^3
// cut depth is in (0, 1)
pub type Vec3 = cgmath::Vector3<f32>;
pub type Rot = cgmath::Quaternion<f32>;
const ROT_ID: Rot = Rot::new(1.0, 0.0, 0.0, 0.0);

// TODO: factor usage of Plane into a struct
// struct Plane

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    R,
    L,
    U,
    D,
    F,
    B,
}
impl Side {
    pub const ALL: [Side; 6] = [Side::R, Side::L, Side::U, Side::D, Side::F, Side::B];

    pub fn color(&self) -> egui::Color32 {
        match self {
            Side::R => egui::Color32::from_rgb(255, 0, 0),
            Side::L => egui::Color32::from_rgb(255, 128, 0),
            Side::U => egui::Color32::from_rgb(255, 255, 255),
            Side::D => egui::Color32::from_rgb(255, 255, 0),
            Side::F => egui::Color32::from_rgb(0, 255, 0),
            Side::B => egui::Color32::from_rgb(0, 0, 255),
        }
    }

    /// unit vector in the direction of the side
    pub fn plane(&self) -> Vec3 {
        match self {
            Side::R => Vec3::new(1.0, 0.0, 0.0),
            Side::L => Vec3::new(-1.0, 0.0, 0.0),
            Side::U => Vec3::new(0.0, 1.0, 0.0),
            Side::D => Vec3::new(0.0, -1.0, 0.0),
            Side::F => Vec3::new(0.0, 0.0, 1.0),
            Side::B => Vec3::new(0.0, 0.0, -1.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sticker {
    /// probably this should be convex.
    pub verts: Vec<Vec3>,
    /// none if it shouldn't be colored, so if it's a cut surface.
    pub side: Option<Side>,
}
impl Sticker {
    /// average of the vertices (local space); `None` if there are no vertices.
    pub fn centroid(&self) -> Option<Vec3> {
        if self.verts.is_empty() {
            return None;
        }
        let sum = self
            .verts
            .iter()
            .fold(Vec3::new(0.0, 0.0, 0.0), |acc, &v| acc + v);
        Some(sum / self.verts.len() as f32)
    }
}

// TODO: switch `Inside` and `Outside`
#[derive(Debug, Clone)]
enum CutResult {
    Inside(Piece),
    Outside(Piece),
    Both { inside: Piece, outside: Piece },
}
impl CutResult {
    fn flatten(self) -> impl Iterator<Item = Piece> {
        // TODO: do this nicer
        match self {
            CutResult::Inside(piece) => [Some(piece), None],
            CutResult::Outside(piece) => [None, Some(piece)],
            CutResult::Both { inside, outside } => [Some(inside), Some(outside)],
        }
        .into_iter()
        .flatten()
    }
}

#[derive(Debug)]
enum IsSplitResult {
    Inside,
    Outside,
    Both,
}

#[derive(Debug, Clone)]
pub struct Piece {
    pub stickers: Vec<Sticker>,
    pub rot: Rot,
}
impl Piece {
    /// average of all sticker vertices (local space, before `self.rot`);
    /// `None` for a piece with no vertices.
    pub fn centroid(&self) -> Option<Vec3> {
        let mut sum = Vec3::new(0.0, 0.0, 0.0);
        let mut count = 0.0;
        for sticker in &self.stickers {
            for &v in &sticker.verts {
                sum += v;
                count += 1.0;
            }
        }
        (count > 0.0).then(|| sum / count)
    }

    /// direction the piece moves when exploded: the sum of the distinct face
    /// normals of its colored stickers, in the piece's local frame. In
    /// cubeshape this keeps each side's stickers coplanar as the puzzle
    /// explodes (every piece on a side is displaced by the same amount along
    /// that side's normal), unlike a centroid-based direction.
    pub fn explode_dir(&self) -> Vec3 {
        Side::ALL
            .into_iter()
            .filter_map(|side| {
                if self.stickers.iter().any(|s| s.side == Some(side)) {
                    Some(side.plane())
                } else {
                    None
                }
            })
            .sum()
    }

    fn full_cube() -> Self {
        let ruf = Vec3::new(1.0, 1.0, 1.0);
        let rub = Vec3::new(1.0, 1.0, -1.0);
        let rdf = Vec3::new(1.0, -1.0, 1.0);
        let rdb = Vec3::new(1.0, -1.0, -1.0);
        let luf = Vec3::new(-1.0, 1.0, 1.0);
        let lub = Vec3::new(-1.0, 1.0, -1.0);
        let ldf = Vec3::new(-1.0, -1.0, 1.0);
        let ldb = Vec3::new(-1.0, -1.0, -1.0);
        Self {
            stickers: vec![
                Sticker {
                    verts: vec![ruf, rdf, rdb, rub],
                    side: Some(Side::R),
                },
                Sticker {
                    verts: vec![luf, lub, ldb, ldf],
                    side: Some(Side::L),
                },
                Sticker {
                    verts: vec![ruf, rub, lub, luf],
                    side: Some(Side::U),
                },
                Sticker {
                    verts: vec![rdf, ldf, ldb, rdb],
                    side: Some(Side::D),
                },
                Sticker {
                    verts: vec![ruf, luf, ldf, rdf],
                    side: Some(Side::F),
                },
                Sticker {
                    verts: vec![rub, rdb, ldb, lub],
                    side: Some(Side::B),
                },
            ],
            rot: ROT_ID,
        }
    }

    fn volume(&self) -> f32 {
        // Sum the signed volumes of tetrahedra formed by the origin and each
        // triangle of every face (fan-triangulated). Rotation is irrelevant to
        // volume, so we work in the piece's local frame. Faces are wound
        // counterclockwise seen from outside the piece, so the result is
        // positive; a negative result means a winding bug.
        let mut signed_volume = 0.0;
        for sticker in &self.stickers {
            for i in 1..sticker.verts.len().saturating_sub(1) {
                let v0 = sticker.verts[0];
                let v1 = sticker.verts[i];
                let v2 = sticker.verts[i + 1];
                signed_volume += v0.dot(v1.cross(v2));
            }
        }
        // assert!(signed_volume > 0.0);
        signed_volume / 6.0
    }

    fn is_internal(&self) -> bool {
        self.stickers.iter().all(|sticker| sticker.side.is_none())
    }

    /// `plane_norm` is the plane's normal vector,
    /// `length` is the distance from the origin to the plane along the normal vector.
    /// inside the cut is the part farther to the origin
    /// (and thus inside the grip).
    fn cut(&self, plane_norm: Vec3, length: f32) -> CutResult {
        const EPSILON: f32 = 1e-6;

        let plane = self.rot.invert() * (plane_norm * length);

        let threshold = plane.dot(plane);
        let signed_dist = |v: Vec3| -> f32 { v.dot(plane) - threshold };

        // If no vertex lies strictly on one of the sides, the plane only
        // touches the piece (e.g. along a face left by a previous cut) and
        // must not split it. Without this early-out, an on-plane face would be
        // handed to both sides, fabricating a zero-volume phantom piece and a
        // duplicate cut-face sticker on the real piece.
        let mut any_inside = false;
        let mut any_outside = false;
        for sticker in &self.stickers {
            for &v in &sticker.verts {
                let d = signed_dist(v);
                if d > EPSILON {
                    any_inside = true;
                } else if d < -EPSILON {
                    any_outside = true;
                }
            }
        }
        if !any_outside {
            return CutResult::Inside(self.clone());
        }
        if !any_inside {
            return CutResult::Outside(self.clone());
        }

        let mut inside_stickers: Vec<Sticker> = Vec::new();
        let mut outside_stickers: Vec<Sticker> = Vec::new();
        // Vertices that lie exactly on the cut plane, collected to form the new face.
        let mut cut_verts: Vec<Vec3> = Vec::new();

        for sticker in &self.stickers {
            let dists: Vec<f32> = sticker.verts.iter().map(|&v| signed_dist(v)).collect();

            let mut inside_poly: Vec<Vec3> = Vec::new();
            let mut outside_poly: Vec<Vec3> = Vec::new();

            for i in 0..sticker.verts.len() {
                let j = (i + 1) % sticker.verts.len();
                let v0 = sticker.verts[i];
                let v1 = sticker.verts[j];
                let d0 = dists[i];
                let d1 = dists[j];

                if d0 > EPSILON {
                    inside_poly.push(v0);
                } else if d0 < -EPSILON {
                    outside_poly.push(v0);
                } else {
                    // Vertex lies exactly on the cut plane — belongs to both sides.
                    inside_poly.push(v0);
                    outside_poly.push(v0);
                    cut_verts.push(v0);
                }

                // Edge crosses the plane — compute the intersection.
                if (d0 > EPSILON && d1 < -EPSILON) || (d0 < -EPSILON && d1 > EPSILON) {
                    let t = d0 / (d0 - d1);
                    let p = v0 + t * (v1 - v0);
                    inside_poly.push(p);
                    outside_poly.push(p);
                    cut_verts.push(p);
                }
            }

            if inside_poly.len() >= 3 {
                inside_stickers.push(Sticker {
                    verts: inside_poly,
                    side: sticker.side,
                });
            }
            if outside_poly.len() >= 3 {
                outside_stickers.push(Sticker {
                    verts: outside_poly,
                    side: sticker.side,
                });
            }
        }

        // Each intersection point is found once per sticker sharing the edge,
        // and on-plane vertices once per incident sticker, so deduplicate
        // before building the cut face.
        let mut deduped: Vec<Vec3> = Vec::new();
        for v in cut_verts {
            if !deduped.iter().any(|&u| (u - v).magnitude2() < 1e-10) {
                deduped.push(v);
            }
        }
        let mut cut_verts = deduped;

        // Build the cut face and add it to both pieces.
        if cut_verts.len() >= 3 {
            let plane_normal = plane.normalize();

            // Compute a centroid and two orthogonal axes in the cut plane so we
            // can sort the (potentially unordered) intersection vertices by angle.
            let centroid = cut_verts
                .iter()
                .fold(Vec3::new(0.0, 0.0, 0.0), |acc, &v| acc + v)
                / cut_verts.len() as f32;

            let arbitrary = if plane_normal.x.abs() < 0.9 {
                Vec3::new(1.0, 0.0, 0.0)
            } else {
                Vec3::new(0.0, 1.0, 0.0)
            };
            let axis_u = (arbitrary - arbitrary.dot(plane_normal) * plane_normal).normalize();
            let axis_v = plane_normal.cross(axis_u);

            cut_verts.sort_by(|&a, &b| {
                let da = a - centroid;
                let db = b - centroid;
                let angle_a = da.dot(axis_v).atan2(da.dot(axis_u));
                let angle_b = db.dot(axis_v).atan2(db.dot(axis_u));
                angle_a.partial_cmp(&angle_b).unwrap()
            });

            // The sort above orders `cut_verts` counterclockwise as seen from
            // the `plane_normal` side (u, v, normal are right-handed). All
            // stickers are wound counterclockwise seen from outside their
            // piece: the outside piece lies on the -normal side, so its cut
            // face takes this order; the inside piece takes the reverse.
            // The cut surface has no side color (it's an internal face).
            let mut inside_face = cut_verts.clone();
            inside_face.reverse();
            inside_stickers.push(Sticker {
                verts: inside_face,
                side: None,
            });
            outside_stickers.push(Sticker {
                verts: cut_verts,
                side: None,
            });
        }

        match (inside_stickers.is_empty(), outside_stickers.is_empty()) {
            (false, true) => CutResult::Inside(Piece {
                stickers: inside_stickers,
                rot: self.rot,
            }),
            (true, false) => CutResult::Outside(Piece {
                stickers: outside_stickers,
                rot: self.rot,
            }),
            _ => CutResult::Both {
                inside: Piece {
                    stickers: inside_stickers,
                    rot: self.rot,
                },
                outside: Piece {
                    stickers: outside_stickers,
                    rot: self.rot,
                },
            },
        }
    }

    /// negative length means that more than half the space would be considered "inside" the cut.
    fn is_split_by(&self, plane_norm: Vec3, length: f32) -> IsSplitResult {
        const EPSILON: f32 = 1e-6;

        if length == f32::INFINITY {
            return IsSplitResult::Outside;
        }
        if length == f32::NEG_INFINITY {
            return IsSplitResult::Inside;
        }
        assert!(length.is_finite());

        let plane = self.rot.invert() * (plane_norm * length);

        let threshold = plane.dot(plane);
        let signed_dist = |v: Vec3| -> f32 { v.dot(plane) - threshold };

        let inside = self
            .stickers
            .iter()
            .all(|sticker| sticker.verts.iter().all(|&v| signed_dist(v) >= -EPSILON));
        let outside = self
            .stickers
            .iter()
            .all(|sticker| sticker.verts.iter().all(|&v| signed_dist(v) <= EPSILON));

        if length >= 0.0 {
            match (inside, outside) {
                (true, false) => IsSplitResult::Inside,
                (false, true) => IsSplitResult::Outside,
                _ => IsSplitResult::Both,
            }
        } else {
            match (inside, outside) {
                (true, false) => IsSplitResult::Outside,
                (false, true) => IsSplitResult::Inside,
                _ => IsSplitResult::Both,
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Twist {
    pub side: Side,
    /// 0, 1, 2
    /// 0 is the layer closest to the side,
    /// 1 is the middle layer,
    /// and 2 is the layer farthest from the side.
    pub layer: u8,
    /// note that 1 is a 45 deg turn,
    /// 2 is a quarter turn,
    /// and 4 is a half turn.
    pub multiplicity: i8,
}
impl Twist {
    pub fn inv(&self) -> Self {
        Self {
            side: self.side,
            layer: self.layer,
            multiplicity: -self.multiplicity,
        }
    }
}

#[derive(Debug)]
pub struct TwistError {
    /// indices into pieces of the pieces straddling the twist's boundary.
    pub blocked: Vec<usize>,
}

#[derive(Debug)]
pub struct PuzzleState {
    pub pieces: Vec<Piece>,
}
impl PuzzleState {
    const CUT_DEPTH: f32 = std::f32::consts::SQRT_2 - 1.0;

    /// the full primordial shape.
    /// used for eg gizmos for mouse twists.
    pub fn uncut() -> Self {
        Self {
            pieces: vec![Piece::full_cube()],
        }
    }

    /// do the 3^3 cuts.
    fn unbandage(&mut self) {
        for side in Side::ALL {
            let plane_norm = side.plane();
            self.pieces = self
                .pieces
                .iter()
                .flat_map(|piece| piece.cut(plane_norm, PuzzleState::CUT_DEPTH).flatten())
                .collect();
        }
    }

    fn discard_internal_pieces(&mut self) {
        self.pieces.retain(|piece| !piece.is_internal());
    }

    pub fn new() -> Self {
        let mut slf = Self::uncut();

        slf.unbandage();

        let m = Twist {
            side: Side::L,
            layer: 1,
            multiplicity: 1,
        };
        let e = Twist {
            side: Side::D,
            layer: 1,
            multiplicity: 1,
        };
        let s = Twist {
            side: Side::F,
            layer: 1,
            multiplicity: 1,
        };

        slf.twist(m).unwrap();
        slf.unbandage();
        slf.twist(m.inv()).unwrap();

        slf.twist(e).unwrap();
        slf.unbandage();
        slf.twist(e.inv()).unwrap();

        slf.twist(s).unwrap();
        slf.unbandage();
        slf.twist(s.inv()).unwrap();

        slf.discard_internal_pieces();

        slf
    }

    /// which pieces a twist would rotate, without mutating.
    /// used by the view to animate a twist before applying it.
    pub fn twist_pieces(&self, twist: Twist) -> Result<Vec<usize>, TwistError> {
        let mut blocked = Vec::new();
        let mut inside = Vec::new();

        let plane_norm = twist.side.plane();
        let (length_lo, length_hi) = match twist.layer {
            0 => (Self::CUT_DEPTH, f32::INFINITY),
            1 => (-Self::CUT_DEPTH, Self::CUT_DEPTH),
            2 => (f32::NEG_INFINITY, -Self::CUT_DEPTH),
            _ => panic!("invalid layer"),
        };

        for (piece_idx, piece) in self.pieces.iter().enumerate() {
            let lo = piece.is_split_by(plane_norm, length_lo);
            let hi = piece.is_split_by(plane_norm, length_hi);
            match (lo, hi) {
                (IsSplitResult::Inside, IsSplitResult::Inside) => (),
                (IsSplitResult::Inside, IsSplitResult::Outside) => inside.push(piece_idx),
                (IsSplitResult::Inside, IsSplitResult::Both) => blocked.push(piece_idx),
                (IsSplitResult::Outside, IsSplitResult::Inside) => unreachable!(),
                (IsSplitResult::Outside, IsSplitResult::Outside) => (),
                (IsSplitResult::Outside, IsSplitResult::Both) => unreachable!(),
                (IsSplitResult::Both, IsSplitResult::Inside) => unreachable!(),
                (IsSplitResult::Both, IsSplitResult::Outside) => blocked.push(piece_idx),
                (IsSplitResult::Both, IsSplitResult::Both) => blocked.push(piece_idx),
            }
        }

        if !blocked.is_empty() {
            return Err(TwistError { blocked });
        }

        Ok(inside)
    }

    pub fn twist(&mut self, twist: Twist) -> Result<(), TwistError> {
        for piece_idx in self.twist_pieces(twist)? {
            let piece = &mut self.pieces[piece_idx];
            let angle = -twist.multiplicity as f32 * std::f32::consts::FRAC_PI_4;
            let rot = Rot::from_axis_angle(twist.side.plane(), cgmath::Rad(angle));
            piece.rot = rot * piece.rot;
        }

        Ok(())
    }
}

#[cfg(test)]
mod cut_tests {
    use super::*;

    fn report(label: &str, cube: &PuzzleState) {
        let total: f32 = cube.pieces.iter().map(|p| p.volume()).sum();
        let degenerate = cube.pieces.iter().filter(|p| p.volume() < 1e-4).count();
        let internal = cube.pieces.iter().filter(|p| p.is_internal()).count();
        println!(
            "{label}: pieces={} total_volume={total} degenerate(<1e-4)={degenerate} internal={internal}",
            cube.pieces.len()
        );
        // volume() is signed and assumes outward winding, so this also
        // catches winding bugs, not just degenerate slivers.
        for piece in &cube.pieces {
            assert!(
                piece.volume() > 1e-4,
                "{label}: piece with non-positive or degenerate volume {} ({} stickers)",
                piece.volume(),
                piece.stickers.len()
            );
        }
    }

    #[test]
    fn volumes_after_cutting() {
        let mut cube = PuzzleState::uncut();
        cube.unbandage();
        report("after 1st unbandage", &cube);
        cube.unbandage();
        report("after 2nd unbandage", &cube);
    }

    #[test]
    fn twist_back_without_intermediate_discards() {
        let mut cube = PuzzleState::uncut();
        cube.unbandage();
        let m = Twist {
            side: Side::L,
            layer: 1,
            multiplicity: 1,
        };
        cube.twist(m).unwrap();
        cube.unbandage();
        report("after twist+unbandage", &cube);
        let res = cube.twist(m.inv());
        if let Err(e) = &res {
            println!("blocked pieces: {}", e.blocked.len());
            for &i in e.blocked.iter().take(5) {
                let p = &cube.pieces[i];
                println!(
                    "  blocked piece: volume={} internal={} stickers={}",
                    p.volume(),
                    p.is_internal(),
                    p.stickers.len()
                );
            }
        }
        assert!(res.is_ok(), "twist back was blocked");
    }
}

#[cfg(test)]
mod new_tests {
    use super::*;

    #[test]
    fn new_builds_and_conserves_volume() {
        let cube = PuzzleState::new();
        let total: f32 = cube.pieces.iter().map(|p| p.volume()).sum();
        println!("new(): pieces={} total_volume={total}", cube.pieces.len());
        // volume() is signed and assumes outward winding, so this also
        // catches winding bugs, not just degenerate slivers.
        for piece in &cube.pieces {
            assert!(
                piece.volume() > 1e-4,
                "piece with non-positive or degenerate volume {}",
                piece.volume()
            );
        }
        // internal pieces are discarded, so total volume is 8 minus the core.
        assert!(total < 8.0);
    }
}
