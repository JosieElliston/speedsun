use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use cgmath::Rotation3;

use crate::{
    commands::{Command, Origin, Rotation},
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

struct ActiveTwist {
    twist: Twist,
    /// indices into puzzle.pieces of the pieces the twist rotates.
    /// validated by twist_pieces() when the twist started.
    pieces: Vec<usize>,
    mode: AnimMode,
}
impl ActiveTwist {
    /// progress through the twist in [0, 1); >= 1 means finished.
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

/// an entry in the undo history: the state-relevant moves, as applied.
/// (blocked twists are dropped before they get here.)
#[derive(Debug, Clone, Copy)]
pub enum Action {
    Twist(Twist),
    Rotate(Rotation),
}

/// The puzzle simulation: latest puzzle state, twist queue + animation,
/// blocked-twist feedback, and undo/redo history. Sole writer of PuzzleState.
/// (Modeled on HSC2's PuzzleSimulation; view-specific state — camera,
/// filters, selection — lives in PuzzleView instead.)
pub struct PuzzleSimulation {
    puzzle: PuzzleState,
    twist_queue: VecDeque<(Twist, Origin)>,
    active_twist: Option<ActiveTwist>,
    blocked_flash: Option<BlockedFlash>,
    undo_stack: Vec<Action>,
    redo_stack: Vec<Action>,
}
impl PuzzleSimulation {
    pub fn new(puzzle: PuzzleState) -> Self {
        Self {
            puzzle,
            twist_queue: VecDeque::new(),
            active_twist: None,
            blocked_flash: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// the latest puzzle state, not including the in-flight animation.
    pub fn puzzle(&self) -> &PuzzleState {
        &self.puzzle
    }

    /// Handle a command routed here by the hub. May return follow-up commands
    /// for the hub to route elsewhere (e.g. undoing a rotation, which this
    /// component records but the camera applies).
    pub fn handle(&mut self, command: Command, now: Instant) -> Vec<Command> {
        match command {
            Command::Twist { twist, origin } => {
                self.twist_queue.push_back((twist, origin));
                vec![]
            }
            Command::Undo => {
                self.finish_queued_twists(now);
                match self.undo_stack.pop() {
                    Some(Action::Twist(twist)) => {
                        self.redo_stack.push(Action::Twist(twist));
                        // the inverse of an applied twist grips the same
                        // pieces, so it can't be blocked.
                        self.twist_queue.push_back((twist.inv(), Origin::Undo));
                        vec![]
                    }
                    Some(Action::Rotate(rotation)) => {
                        self.redo_stack.push(Action::Rotate(rotation));
                        vec![Command::Rotate {
                            rotation: rotation.inv(),
                            origin: Origin::Undo,
                        }]
                    }
                    None => vec![],
                }
            }
            Command::Redo => {
                self.finish_queued_twists(now);
                match self.redo_stack.pop() {
                    Some(Action::Twist(twist)) => {
                        self.undo_stack.push(Action::Twist(twist));
                        self.twist_queue.push_back((twist, Origin::Redo));
                        vec![]
                    }
                    Some(Action::Rotate(rotation)) => {
                        self.undo_stack.push(Action::Rotate(rotation));
                        vec![Command::Rotate {
                            rotation,
                            origin: Origin::Redo,
                        }]
                    }
                    None => vec![],
                }
            }
            _ => unreachable!("the hub routes only twist/undo/redo commands to the simulation"),
        }
    }

    /// Record a user rotation in the undo history. The camera applies
    /// rotations; the hub calls this alongside.
    pub fn record_rotation(&mut self, rotation: Rotation) {
        self.undo_stack.push(Action::Rotate(rotation));
        self.redo_stack.clear();
    }

    /// Apply the active and queued twists immediately, skipping animation.
    /// Used before undo/redo so the history and the displayed state agree.
    fn finish_queued_twists(&mut self, now: Instant) {
        if let Some(active) = self.active_twist.take() {
            self.puzzle
                .twist(active.twist)
                .expect("twist was validated when its animation started");
        }
        while let Some((twist, origin)) = self.twist_queue.pop_front() {
            match self.puzzle.twist(twist) {
                Ok(()) => self.record_applied(twist, origin),
                Err(e) => {
                    self.blocked_flash = Some(BlockedFlash {
                        pieces: e.blocked,
                        start: now,
                    });
                }
            }
        }
    }

    /// history bookkeeping for a twist that passed validation. only user
    /// twists are recorded: undo/redo replays are already accounted for by
    /// the stack manipulation in `handle`.
    fn record_applied(&mut self, twist: Twist, origin: Origin) {
        if origin == Origin::User {
            self.undo_stack.push(Action::Twist(twist));
            self.redo_stack.clear();
        }
    }

    /// per-frame animation tick: advance the active twist by one drawn frame,
    /// apply it to the puzzle once finished, and chain into the queue.
    pub fn tick(&mut self, now: Instant, stable_dt: f32, twist_duration: f32) {
        match &mut self.active_twist {
            None => self.start_next_twist(AnimStart::Fresh, now, stable_dt, twist_duration),
            Some(active) => {
                // Time mode derives progress from the clock instead.
                if let AnimMode::Frame { progress, n_frames } = &mut active.mode {
                    *progress += 1.0 / *n_frames;
                }
            }
        }

        // Apply finished twists. Loops because at very fast speeds
        // (n_frames < 1) several twists can complete in one drawn frame.
        loop {
            let Some(active) = &self.active_twist else {
                return;
            };
            let p = active.progress(now, twist_duration);
            if p < 1.0 {
                return;
            }
            let twist = active.twist;
            let carry = match active.mode {
                AnimMode::Frame { progress, .. } => AnimStart::CarryFrames(progress - 1.0),
                AnimMode::Time { start } => {
                    AnimStart::CarryTime(start + Duration::from_secs_f32(twist_duration))
                }
            };
            self.puzzle
                .twist(twist)
                .expect("twist was validated when its animation started");
            self.active_twist = None;
            self.start_next_twist(carry, now, stable_dt, twist_duration);
        }
    }

    fn start_next_twist(
        &mut self,
        start: AnimStart,
        now: Instant,
        stable_dt: f32,
        twist_duration: f32,
    ) {
        while let Some((twist, origin)) = self.twist_queue.pop_front() {
            let pieces = match self.puzzle.twist_pieces(twist) {
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
            self.record_applied(twist, origin);
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
            self.active_twist = Some(ActiveTwist {
                twist,
                pieces,
                mode,
            });
            return;
        }
    }

    /// Partial rotation of the animating layer's pieces, as a piece mask and
    /// the rotation to compose onto them. The angle formula must match
    /// PuzzleState::twist exactly so progress 1 converges to the applied
    /// state.
    pub fn anim(&self, now: Instant, twist_duration: f32) -> Option<(Vec<bool>, Rot)> {
        self.active_twist.as_ref().map(|active| {
            let p = ease(active.progress(now, twist_duration));
            let angle = -active.twist.multiplicity as f32 * std::f32::consts::FRAC_PI_4 * p;
            let rot = Rot::from_axis_angle(active.twist.side.plane(), cgmath::Rad(angle));
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
