//! Coarse progress for a running scan.
//!
//! The weights below come from a measured run on a 2.2M-line project: parsing
//! and emitting cards 36%, resolving edges 3%, writing tables 43%, commit 17%.
//! They are only used to turn "3 of 8 phases" into a fraction the UI can draw.

#[derive(Debug, Clone, Copy)]
pub struct Progress {
    /// Human readable phase name, shown next to the bar.
    pub phase: &'static str,
    /// Items done within the current phase.
    pub done: u64,
    /// Items in the current phase, 0 when the phase has no countable items.
    pub total: u64,
    /// Overall fraction in 0.0..=1.0.
    pub fraction: f32,
}

impl Default for Progress {
    fn default() -> Self {
        Self {
            phase: "准备",
            done: 0,
            total: 0,
            fraction: 0.0,
        }
    }
}

/// Where each phase starts and ends on the 0..1 axis.
pub mod weight {
    pub const WALK: (f32, f32) = (0.0, 0.01);
    pub const PARSE: (f32, f32) = (0.01, 0.37);
    pub const RESOLVE: (f32, f32) = (0.37, 0.40);
    pub const SYMBOLS: (f32, f32) = (0.40, 0.46);
    pub const CARDS: (f32, f32) = (0.46, 0.56);
    pub const CALLS: (f32, f32) = (0.56, 0.82);
    pub const MISC: (f32, f32) = (0.82, 0.83);
    pub const COMMIT: (f32, f32) = (0.83, 1.0);

    /// Map `done / total` inside a phase onto the global 0..1 axis.
    pub fn lerp(span: (f32, f32), done: u64, total: u64) -> f32 {
        if total == 0 {
            return span.0;
        }
        let t = (done as f32 / total as f32).clamp(0.0, 1.0);
        span.0 + (span.1 - span.0) * t
    }
}
