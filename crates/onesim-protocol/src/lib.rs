//! Serde types for the three OneSim JSON boundaries described in `docs/protocol.md`.

pub mod api;
pub mod paint;
pub mod view;
pub mod wire;

pub use api::*;
pub use paint::*;
pub use view::*;
pub use wire::*;

pub const PROTOCOL_VERSION: u32 = 1;
