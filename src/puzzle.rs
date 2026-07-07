use cgmath::{InnerSpace, One, Rotation, Rotation3};
use eframe::egui;

// in [-1, 1]^3
// cut depth is in (0, 1)
pub type Vec3 = cgmath::Vector3<f32>;
pub type Rot = cgmath::Quaternion<f32>;
const ROT_ID: Rot = Rot::new(1.0, 0.0, 0.0, 0.0);

#[derive(Debug, Clone, Copy)]
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

    /// `plane_norm` is the plane's normal vector,
    /// `length` is the distance from the origin to the plane along the normal vector.
    /// inside the cut is the part farther to the origin
    /// (and thus inside the grip).
    fn cut(&self, plane_norm: Vec3, length: f32) -> CutResult {
        const EPSILON: f32 = 1e-6;

        let plane = self.rot.invert() * (plane_norm * length);

        let threshold = plane.dot(plane);
        let signed_dist = |v: Vec3| -> f32 { v.dot(plane) - threshold };

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

            // The cut surface has no side color (it's an internal face).
            inside_stickers.push(Sticker {
                verts: cut_verts.clone(),
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

    // /// `plane_norm` is the plane's normal vector,
    // /// `length` is the distance from the origin to the plane along the normal vector.
    // /// inside the cut is the part farther to the origin
    // /// (and thus inside the grip).
    // fn cut(&self, plane_norm: Vec3, length: f32) -> CutResult {
    //     let plane = self.rot.invert() * (plane_norm * length);

    //     let sd = |v: Vec3| -> f32 { v.dot(plane) - plane.dot(plane) };

    //     let mut new_verts = Vec::new();

    //     for sticker in &self.stickers {
    //         // if the stickers are all on the same side, continue
    //         {
    //             let mut any_inside = false;
    //             let mut any_outside = false;
    //             for &v in &sticker.verts {
    //                 let d = sd(v);
    //                 if d > 0.0 {
    //                     any_inside = true;
    //                 } else if d < 0.0 {
    //                     any_outside = true;
    //                 }
    //             }
    //             if !any_inside || !any_outside {
    //                 continue;
    //             }
    //         }

    //     }
    // }

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
    pub blocked: Vec<Piece>,
}

#[derive(Debug)]
pub struct MixupCube {
    pub pieces: Vec<Piece>,
}
impl MixupCube {
    const CUT_DEPTH: f32 = std::f32::consts::SQRT_2 - 1.0;

    /// the full primordial shape.
    /// used for eg gizmos for mouse twists.
    pub fn uncut() -> Self {
        Self {
            pieces: vec![Piece::full_cube()],
        }
    }

    pub fn new() -> Self {
        fn unbandage(slf: &mut MixupCube) {
            // do the 3^3 cuts
            for side in Side::ALL {
                let plane_norm = side.plane();
                slf.pieces = slf
                    .pieces
                    .iter()
                    .flat_map(|piece| piece.cut(plane_norm, MixupCube::CUT_DEPTH).flatten())
                    .collect();
            }
        }

        fn discard_internal_pieces(slf: &mut MixupCube) {
            slf.pieces
                .retain(|piece| piece.stickers.iter().any(|sticker| sticker.side.is_some()));
        }

        let mut slf = Self::uncut();

        unbandage(&mut slf);
        // unbandage(&mut slf);

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

        discard_internal_pieces(&mut slf);
        slf.twist(m).unwrap();
        unbandage(&mut slf);
        discard_internal_pieces(&mut slf);
        slf.twist(m.inv()).unwrap();

        // slf.twist(e).unwrap();
        // unbandage(&mut slf);
        // slf.twist(e.inv()).unwrap();

        // slf.twist(s).unwrap();
        // unbandage(&mut slf);
        // slf.twist(s.inv()).unwrap();

        discard_internal_pieces(&mut slf);

        slf
    }

    pub fn twist(&mut self, twist: Twist) -> Result<(), TwistError> {
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
            return Err(TwistError {
                blocked: blocked
                    .into_iter()
                    .map(|i| self.pieces[i].clone())
                    .collect(),
            });
        }

        for piece_idx in inside {
            let piece = &mut self.pieces[piece_idx];
            let angle = -twist.multiplicity as f32 * std::f32::consts::FRAC_PI_4;
            let rot = Rot::from_axis_angle(twist.side.plane(), cgmath::Rad(angle));
            piece.rot = rot * piece.rot;
        }

        Ok(())
    }
}
