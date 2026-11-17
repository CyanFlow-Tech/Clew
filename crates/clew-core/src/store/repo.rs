use std::time::{SystemTime, UNIX_EPOCH};

mod call;
mod card;
mod class;
mod module;
mod note;
mod project;
mod symbol;

pub use call::*;
pub use card::*;
pub use class::*;
pub use module::*;
pub use note::*;
pub use project::*;
pub use symbol::*;

pub(crate) fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}
