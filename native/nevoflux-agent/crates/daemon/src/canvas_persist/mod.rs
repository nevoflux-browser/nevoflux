//! My Canvas persistence module.
//!
//! Provides listing (and, in subsequent tasks, saving, renaming, and deleting)
//! of artifacts that the user has pinned to their personal canvas library
//! (`is_persistent = 1`).

pub mod handlers;
pub mod service;

pub use handlers::handle;
pub use service::CanvasPersistService;
