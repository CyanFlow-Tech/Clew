//! Coarse progress for a running scan.
//!

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
    pub const PARSE: (f32, f32) = (0.01, 0.85);
    pub const RESOLVE: (f32, f32) = (0.85, 0.86);
    pub const SYMBOLS: (f32, f32) = (0.86, 0.92);
    pub const MISC: (f32, f32) = (0.92, 0.95);
    pub const COMMIT: (f32, f32) = (0.95, 1.0);

    /// Map `done / total` inside a phase onto the global 0..1 axis.
    pub fn lerp(span: (f32, f32), done: u64, total: u64) -> f32 {
        if total == 0 {
            return span.0;
        }
        let t = (done as f32 / total as f32).clamp(0.0, 1.0);
        span.0 + (span.1 - span.0) * t
    }
}
