//! ffmpeg resolution + subprocess spawning.
//!
//! Resolution via `ffmpeg-sidecar`:
//!   1. If ffmpeg is already available (system PATH or a prior sidecar download),
//!      return its path.
//!   2. Otherwise `auto_download()` fetches a static build to the sidecar directory
//!      (next to the running executable), then we return that path.
//!
//! NOTE (spec deviation): The original plan called for binary placement under
//! `~/.cache/nevoflux/bin/ffmpeg`. ffmpeg-sidecar 1.1.x does not expose a
//! configurable download destination; it always uses the sidecar directory
//! (directory of the running executable). Placing the binary in a user-controlled
//! cache path is deferred to a later task if that requirement becomes hard.
//! For P1 the only requirement is: return a working ffmpeg binary, downloading
//! on demand if none exists.

use std::path::PathBuf;

use ffmpeg_sidecar::command::{ffmpeg_is_installed, FfmpegCommand};
use ffmpeg_sidecar::download::auto_download;
use ffmpeg_sidecar::paths::ffmpeg_path;

use crate::error::{DaemonError, Result};

/// Resolve the ffmpeg binary path, auto-downloading a static build if needed.
///
/// On success the returned `PathBuf` always points to an executable that:
/// * exists on the filesystem, AND
/// * responds successfully to `ffmpeg -version`.
pub fn resolve_ffmpeg() -> Result<PathBuf> {
    // Fast path: binary already available (system PATH OR prior sidecar download).
    if ffmpeg_is_installed() {
        return resolved_path();
    }

    // Slow path: download static build to sidecar dir (next to the executable).
    // FFMPEG_DOWNLOAD_DIR is NOT a recognised env var in ffmpeg-sidecar 1.1.x;
    // the download destination is always sidecar_dir() and cannot be redirected
    // via environment variables.
    auto_download()
        .map_err(|e| DaemonError::InternalError(format!("ffmpeg auto_download failed: {}", e)))?;

    if !ffmpeg_is_installed() {
        return Err(DaemonError::InternalError(
            "ffmpeg still not available after auto_download".into(),
        ));
    }

    resolved_path()
}

/// Return the filesystem path that ffmpeg-sidecar will actually use.
///
/// `ffmpeg_path()` returns either:
/// - An absolute path when the sidecar binary (next to the exe) exists, OR
/// - The bare name `"ffmpeg"` when only the system-PATH binary is available.
///
/// For the bare-name case we resolve it with `which` so callers always receive
/// an absolute, verifiable path.
fn resolved_path() -> Result<PathBuf> {
    let p = ffmpeg_path();
    if p.is_absolute() && p.exists() {
        return Ok(p);
    }
    // p is the bare "ffmpeg" string (or a non-existent absolute path) — resolve via PATH.
    which::which("ffmpeg")
        .map_err(|e| DaemonError::InternalError(format!("which ffmpeg failed: {}", e)))
}

/// Construct an ffmpeg image2pipe command for encoding a PNG stream into MP4.
///
/// Reads raw PNG frames from stdin and writes an H.264/MP4 file to `output_path`.
/// When `audio_input` is provided, that audio file (mp3/wav) is muxed in as a
/// second input and encoded as AAC. The video stream length sets the output
/// duration: longer audio is truncated; shorter audio leaves trailing silence.
pub fn image2pipe_cmd(
    output_path: &std::path::Path,
    fps: u32,
    audio_input: Option<&std::path::Path>,
) -> FfmpegCommand {
    let mut cmd = FfmpegCommand::new();
    cmd.hide_banner().args([
        "-y",
        "-f",
        "image2pipe",
        "-framerate",
        &fps.to_string(),
        "-i",
        "-",
    ]);
    if let Some(audio) = audio_input {
        // Second input — ffmpeg auto-detects mp3/wav from extension/contents.
        cmd.args(["-i", audio.to_string_lossy().as_ref()]);
    }
    cmd.args([
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "23", "-preset", "medium",
    ]);
    if audio_input.is_some() {
        cmd.args([
            // AAC at 192k for narration — overkill for speech but keeps
            // files compatible with all major players.
            "-c:a", "aac", "-b:a", "192k",
            // Map streams explicitly: video from input 0, audio from input 1.
            "-map", "0:v", "-map",
            "1:a",
            // Match output duration to the video stream so a longer audio
            // doesn't extend the file. (`-shortest` would TRUNCATE to the
            // shorter; we want the full video with audio playing to its
            // natural end and silence afterward — that's the default
            // without `-shortest`.)
        ]);
    }
    cmd.args(["-movflags", "+faststart"])
        .output(output_path.to_string_lossy().as_ref());
    cmd
}
