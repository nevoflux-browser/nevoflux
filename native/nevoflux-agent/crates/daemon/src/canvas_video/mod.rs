//! `canvas.video.*` — composition authoring + render pipeline.
//!
//! See `docs/superpowers/specs/2026-04-19-video-skill-design.md`.

pub mod asset_inline;
pub mod asset_resize;
pub mod create;
pub mod design;
pub mod ffmpeg;
pub mod frame_chunks;
pub mod handlers;
pub mod job;
pub mod render;
pub mod reveal;
pub mod service;
pub mod tool;
pub mod vi_to_design;

pub use service::{CanvasVideoService, RenderControlEvent};
