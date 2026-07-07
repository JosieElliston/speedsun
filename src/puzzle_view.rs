// use std::time::Instant;

// use crate::puzzle::*;

// struct PuzzleView {
//     twist: Anim<Twist>,
//     twist_start: Instant,
//     selected_pieces: Vec<PieceId>,
//     rot: Rot,
// }
// impl PuzzleView {
//     fn sticker_of_pos(&self, pos: egui::Pos2) -> Option<PieceId> {}
//     fn piece_of_pos(&self, pos: egui::Pos2) -> Option<PieceId> {}
// }

// except i want to have both "exactly 1/2 frames, with this far along each frame"
// and "use this easing curve, properly synced with the frame rate"
// struct Anim<T> {
//     t: T,
//     start: Instant,
// }
