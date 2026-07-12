use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use cgmath::Rotation3;

use crate::{
    commands::{Command, Origin, Rotation as PuzzleRotation},
    puzzle_state::*,
};

/// below this many frames per twist, animation progress is frame-indexed
/// instead of clock-based: each drawn frame advances by exactly 1/n_frames, so
/// the intermediate fractions are deterministic and dt jitter can neither skip
/// a twist's animation entirely nor change which fraction gets shown.
const FAST_MODE_MAX_FRAMES: f32 = 3.0;

pub fn ease(t: f32) -> f32 {
    // cosine interpolation
    0.5 - 0.5 * (t * std::f32::consts::PI).cos()
}

/// a state-changing move: a layer twist or a whole-puzzle rotation. separate
/// variants because rotations grip every piece and can never be blocked.
/// doubles as the undo-history entry (an Align is recorded as the rotations
/// it induced, so the history is nothing but moves).
#[derive(Debug, Clone, Copy)]
enum Move {
    Twist(Twist),
    Rotate(PuzzleRotation),
}
impl Move {
    /// rotation axis (unit) and 45°-multiplicity; both kinds share `Twist`'s
    /// angle convention (multiplicity 1 = -45° about the axis).
    fn axis_multiplicity(self) -> (Vec3, i8) {
        match self {
            Move::Twist(twist) => (twist.side.plane(), twist.multiplicity),
            Move::Rotate(rotation) => (rotation.axis.unit(), rotation.multiplicity),
        }
    }

    fn inv(self) -> Self {
        match self {
            Move::Twist(twist) => Move::Twist(twist.inv()),
            Move::Rotate(rotation) => Move::Rotate(rotation.inv()),
        }
    }
}

struct ActiveMove {
    mv: Move,
    /// indices into puzzle.pieces of the pieces the move rotates.
    /// validated when the move started.
    pieces: Vec<usize>,
    mode: AnimMode,
}
impl ActiveMove {
    /// progress through the move in [0, 1); >= 1 means finished.
    fn progress(&self, now: Instant, duration: f32) -> f32 {
        match self.mode {
            AnimMode::Frame { progress, .. } => progress,
            AnimMode::Time { start } => {
                now.saturating_duration_since(start).as_secs_f32() / duration
            }
        }
    }
}

/// the timing regime, chosen per twist when it starts and then frozen so a
/// single twist can't flicker between regimes.
#[derive(Clone, Copy)]
enum AnimMode {
    /// slow twists: dt-aware, sampled at real elapsed time. `start` may lie
    /// before "now" by carried-over time from the previous twist.
    Time { start: Instant },
    /// fast twists (~1-2 frames): each drawn frame adds 1/n_frames.
    Frame { progress: f32, n_frames: f32 },
}

/// tints the pieces that blocked a twist red, fading out.
struct BlockedFlash {
    /// indices into puzzle.pieces of the blocking pieces.
    pieces: Vec<usize>,
    start: Instant,
}
impl BlockedFlash {
    /// red tint strength in [0, 1]; 0 means expired.
    fn strength(&self, now: Instant, duration: f32) -> f32 {
        let t = now.saturating_duration_since(self.start).as_secs_f32() / duration;
        (1.0 - t).max(0.0)
    }
}

/// how a twist's progress begins: fresh from the queue, or carrying the
/// previous twist's overshoot so back-to-back twists keep a steady cadence.
enum AnimStart {
    Fresh,
    /// leftover fraction of a twist (can exceed 1 at very fast speeds).
    CarryFrames(f32),
    /// the instant the previous twist nominally finished.
    CarryTime(Instant),
}

/// The puzzle simulation: latest puzzle state, twist queue + animation,
/// blocked-twist feedback, and undo/redo history. Sole writer of PuzzleState.
/// (Modeled on HSC2's PuzzleSimulation; view-specific state — camera,
/// filters, selection — lives in PuzzleView instead.)
pub struct PuzzleSimulation {
    puzzle: PuzzleState,
    move_queue: VecDeque<(Move, Origin)>,
    active_move: Option<ActiveMove>,
    blocked_flash: Option<BlockedFlash>,
    undo_stack: Vec<Move>,
    redo_stack: Vec<Move>,
}
impl PuzzleSimulation {
    pub fn new(puzzle: PuzzleState) -> Self {
        Self {
            puzzle,
            move_queue: VecDeque::new(),
            active_move: None,
            blocked_flash: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// the latest puzzle state, not including the in-flight animation.
    pub fn puzzle(&self) -> &PuzzleState {
        &self.puzzle
    }

    /// Handle a command routed here by the hub.
    pub fn handle(&mut self, command: Command, now: Instant) {
        match command {
            Command::Twist { twist, origin } => {
                self.move_queue.push_back((Move::Twist(twist), origin));
            }
            Command::Rotate { rotation, origin } => {
                self.move_queue.push_back((Move::Rotate(rotation), origin));
            }
            Command::Undo => {
                self.finish_queued_moves(now);
                if let Some(mv) = self.undo_stack.pop() {
                    self.redo_stack.push(mv);
                    // the inverse of an applied move grips the same pieces,
                    // so it can't be blocked.
                    self.move_queue.push_back((mv.inv(), Origin::Undo));
                }
            }
            Command::Redo => {
                self.finish_queued_moves(now);
                if let Some(mv) = self.redo_stack.pop() {
                    self.undo_stack.push(mv);
                    self.move_queue.push_back((mv, Origin::Redo));
                }
            }
            _ => unreachable!("the hub routes only twist/rotate/undo/redo commands here"),
        }
    }

    /// Re-base the puzzle state onto the view's axis-aligned orientation:
    /// the hub computes `orientation` from the view (which keeps only the
    /// sub-90° residual), and this rotates the state to match — visually
    /// net-zero. After this, face keybinds mean what they look like.
    /// Recorded in the history as the induced rotation(s), so undoing past
    /// an align rotates the puzzle back in view like any other rotation.
    pub fn align(&mut self, orientation: Rot, now: Instant) {
        // an already-agreeing view is a no-op; don't pollute the history.
        if orientation.s.abs() > 1.0 - 1e-6 {
            return;
        }
        // pending moves were queued in the old frame's coordinates; finish
        // them there before re-basing.
        self.finish_queued_moves(now);
        self.rotate_state(orientation);
        for rotation in PuzzleRotation::decompose(orientation) {
            self.undo_stack.push(Move::Rotate(rotation));
        }
        self.redo_stack.clear();
    }

    /// rotate every piece: a whole-puzzle rotation of the latest state.
    fn rotate_state(&mut self, rot: Rot) {
        for piece in &mut self.puzzle.pieces {
            piece.rot = rot * piece.rot;
        }
    }

    /// which pieces a move grips, or the blocking pieces. rotations grip
    /// everything and can't be blocked.
    fn move_pieces(&self, mv: Move) -> Result<Vec<usize>, TwistError> {
        match mv {
            Move::Twist(twist) => self.puzzle.twist_pieces(twist),
            Move::Rotate(_) => Ok((0..self.puzzle.pieces.len()).collect()),
        }
    }

    /// apply a move to the latest state. the twist must have been validated.
    fn apply_move(&mut self, mv: Move) {
        match mv {
            Move::Twist(twist) => self
                .puzzle
                .twist(twist)
                .expect("twist was validated when its animation started"),
            Move::Rotate(rotation) => self.rotate_state(rotation.quat()),
        }
    }

    /// Apply the active and queued moves immediately, skipping animation.
    /// Used before undo/redo/align so the history, the state, and the
    /// coordinate frame agree.
    fn finish_queued_moves(&mut self, now: Instant) {
        if let Some(active) = self.active_move.take() {
            self.apply_move(active.mv);
        }
        while let Some((mv, origin)) = self.move_queue.pop_front() {
            match self.move_pieces(mv) {
                Ok(_) => {
                    self.apply_move(mv);
                    self.record_applied(mv, origin);
                }
                Err(e) => {
                    self.blocked_flash = Some(BlockedFlash {
                        pieces: e.blocked,
                        start: now,
                    });
                }
            }
        }
    }

    /// history bookkeeping for a move that passed validation. only user
    /// moves are recorded: undo/redo replays are already accounted for by
    /// the stack manipulation in `handle`.
    fn record_applied(&mut self, mv: Move, origin: Origin) {
        if origin == Origin::User {
            self.undo_stack.push(mv);
            self.redo_stack.clear();
        }
    }

    /// per-frame animation tick: advance the active move by one drawn frame,
    /// apply it to the puzzle once finished, and chain into the queue.
    pub fn tick(&mut self, now: Instant, stable_dt: f32, twist_duration: f32) {
        match &mut self.active_move {
            None => self.start_next_move(AnimStart::Fresh, now, stable_dt, twist_duration),
            Some(active) => {
                // Time mode derives progress from the clock instead.
                if let AnimMode::Frame { progress, n_frames } = &mut active.mode {
                    *progress += 1.0 / *n_frames;
                }
            }
        }

        // Apply finished moves. Loops because at very fast speeds
        // (n_frames < 1) several moves can complete in one drawn frame.
        loop {
            let Some(active) = &self.active_move else {
                return;
            };
            let p = active.progress(now, twist_duration);
            if p < 1.0 {
                return;
            }
            let mv = active.mv;
            let carry = match active.mode {
                AnimMode::Frame { progress, .. } => AnimStart::CarryFrames(progress - 1.0),
                AnimMode::Time { start } => {
                    AnimStart::CarryTime(start + Duration::from_secs_f32(twist_duration))
                }
            };
            self.apply_move(mv);
            self.active_move = None;
            self.start_next_move(carry, now, stable_dt, twist_duration);
        }
    }

    fn start_next_move(
        &mut self,
        start: AnimStart,
        now: Instant,
        stable_dt: f32,
        twist_duration: f32,
    ) {
        while let Some((mv, origin)) = self.move_queue.pop_front() {
            let pieces = match self.move_pieces(mv) {
                Ok(pieces) => pieces,
                // blocked: drop the twist and flash the blocking pieces.
                Err(e) => {
                    self.blocked_flash = Some(BlockedFlash {
                        pieces: e.blocked,
                        start: now,
                    });
                    continue;
                }
            };
            self.record_applied(mv, origin);
            let n_frames = twist_duration / stable_dt.clamp(1e-4, 1.0);
            let mode = if n_frames < FAST_MODE_MAX_FRAMES {
                let progress = match start {
                    // this drawn frame is the twist's first frame.
                    AnimStart::Fresh | AnimStart::CarryTime(_) => 1.0 / n_frames,
                    // the previous twist's overshoot already includes this
                    // frame's share.
                    AnimStart::CarryFrames(carry) => carry,
                };
                AnimMode::Frame { progress, n_frames }
            } else {
                let start = match start {
                    // backdate by one frame so the first drawn frame already
                    // shows motion (matching Frame mode's 1/n_frames start);
                    // starting at rest reads as input lag.
                    AnimStart::Fresh | AnimStart::CarryFrames(_) => {
                        now - Duration::from_secs_f32(stable_dt)
                    }
                    AnimStart::CarryTime(t) => t,
                };
                AnimMode::Time { start }
            };
            self.active_move = Some(ActiveMove { mv, pieces, mode });
            return;
        }
    }

    /// Partial rotation of the animating move's pieces, as a piece mask and
    /// the rotation to compose onto them. The angle formula must match the
    /// applied move exactly so progress 1 converges to the applied state.
    pub fn anim(&self, now: Instant, twist_duration: f32) -> Option<(Vec<bool>, Rot)> {
        self.active_move.as_ref().map(|active| {
            let p = ease(active.progress(now, twist_duration));
            let (axis, multiplicity) = active.mv.axis_multiplicity();
            let angle = -multiplicity as f32 * std::f32::consts::FRAC_PI_4 * p;
            let rot = Rot::from_axis_angle(axis, cgmath::Rad(angle));
            let mut mask = vec![false; self.puzzle.pieces.len()];
            for &i in &active.pieces {
                mask[i] = true;
            }
            (mask, rot)
        })
    }

    /// Red tint mask and strength for the pieces that blocked a rejected
    /// twist; None once expired.
    pub fn flash(&self, now: Instant, flash_duration: f32) -> Option<(Vec<bool>, f32)> {
        let flash = self.blocked_flash.as_ref()?;
        let strength = flash.strength(now, flash_duration);
        (strength > 0.0).then(|| {
            let mut mask = vec![false; self.puzzle.pieces.len()];
            for &i in &flash.pieces {
                mask[i] = true;
            }
            (mask, strength)
        })
    }
}

#[cfg(test)]
mod tests {
    use cgmath::AbsDiffEq;

    use super::*;
    use crate::commands::Axis;

    const ID: Rot = Rot::new(1.0, 0.0, 0.0, 0.0);

    /// tick with a tiny duration so every queued move applies immediately.
    fn settle(sim: &mut PuzzleSimulation, now: Instant) {
        for _ in 0..4 {
            sim.tick(now, 1.0, 1e-3);
        }
    }

    #[test]
    fn rotate_then_undo_restores_orientation() {
        let now = Instant::now();
        let mut sim = PuzzleSimulation::new(PuzzleState::uncut());
        let y90 = PuzzleRotation::new(Axis::Y, 2);

        sim.handle(
            Command::Rotate {
                rotation: y90,
                origin: Origin::User,
            },
            now,
        );
        settle(&mut sim, now);
        assert!(sim.puzzle().pieces[0].rot.abs_diff_eq(&y90.quat(), 1e-6));

        sim.handle(Command::Undo, now);
        settle(&mut sim, now);
        assert!(sim.puzzle().pieces[0].rot.abs_diff_eq(&ID, 1e-6));
    }

    /// equality up to the quaternion double cover (q and -q are the same
    /// rotation; align records rotations recomposed from `decompose`, whose
    /// hemisphere is unspecified).
    fn same_rot(a: Rot, b: Rot) -> bool {
        a.abs_diff_eq(&b, 1e-6) || a.abs_diff_eq(&-b, 1e-6)
    }

    #[test]
    fn align_records_its_induced_rotation() {
        let now = Instant::now();
        let mut sim = PuzzleSimulation::new(PuzzleState::uncut());
        let a = PuzzleRotation::new(Axis::X, 2).quat();

        sim.align(a, now);
        assert!(sim.puzzle().pieces[0].rot.abs_diff_eq(&a, 1e-6));

        // undo rotates the puzzle back in view, like undoing an ordinary
        // whole-puzzle rotation; the view is not involved.
        sim.handle(Command::Undo, now);
        settle(&mut sim, now);
        assert!(same_rot(sim.puzzle().pieces[0].rot, ID));

        // an align at an already-agreeing orientation is a history no-op
        // (in particular it must not clear the redo stack).
        sim.align(ID, now);
        sim.handle(Command::Redo, now);
        settle(&mut sim, now);
        assert!(same_rot(sim.puzzle().pieces[0].rot, a));
    }

    #[test]
    fn two_step_alignment_undoes_in_two_steps() {
        let now = Instant::now();
        let mut sim = PuzzleSimulation::new(PuzzleState::uncut());
        // a vertex orientation: not a rotation about a single coordinate
        // axis, so align records two rotations.
        let a = PuzzleRotation::new(Axis::X, 2).quat() * PuzzleRotation::new(Axis::Y, 2).quat();

        sim.align(a, now);
        assert!(sim.puzzle().pieces[0].rot.abs_diff_eq(&a, 1e-6));

        sim.handle(Command::Undo, now);
        settle(&mut sim, now);
        assert!(!same_rot(sim.puzzle().pieces[0].rot, ID));

        sim.handle(Command::Undo, now);
        settle(&mut sim, now);
        assert!(same_rot(sim.puzzle().pieces[0].rot, ID));
    }
}
