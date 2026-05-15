//! End-to-end tests for canvas.video.* bridge handlers.

use std::sync::Arc;

use nevoflux_daemon::canvas_video::CanvasVideoService;
use nevoflux_protocol::canvas_video::{CreateCompositionRequest, CreateCompositionResponse};

fn fresh_service() -> Arc<CanvasVideoService> {
    // Test service uses in-memory artifact repo + temp dir for outputs.
    Arc::new(CanvasVideoService::new_for_tests())
}

const SAMPLE_DESIGN_MD: &str = r##"---
name: "test-orange"
colors:
  primary: "#ff6600"
  secondary: "#cc4400"
  background: "#000000"
  foreground: "#ffffff"
typography:
  hero:
    family: "Inter, sans-serif"
    weight: 800
spacing:
  lg: "24px"
---

## Overview
test brand
"##;

#[tokio::test]
async fn test_create_composition_returns_artifact_id() {
    let svc = fresh_service();
    let req = CreateCompositionRequest {
        title: "demo".into(),
        width: 1920,
        height: 1080,
        duration_sec: 5.0,
        fps: 30,
        bg: None,
        html: None,
        template: None,
        design_md: None,
        session_id: None,
    };
    let resp: CreateCompositionResponse = svc.create_composition(req).await.unwrap();
    assert!(!resp.artifact_id.is_empty());
    assert!(resp.artifact_id.starts_with("comp-"));
}

#[tokio::test]
async fn test_create_composition_rejects_invalid_fps() {
    let svc = fresh_service();
    let req = CreateCompositionRequest {
        title: "bad".into(),
        width: 1920,
        height: 1080,
        duration_sec: 5.0,
        fps: 60, // invalid
        bg: None,
        html: None,
        template: None,
        design_md: None,
        session_id: None,
    };
    let err = svc.create_composition(req).await.unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("fps") || msg.contains("60"),
        "error should mention fps: {}",
        msg
    );
}

#[tokio::test]
async fn test_create_composition_rejects_duration_over_60s() {
    let svc = fresh_service();
    let req = CreateCompositionRequest {
        title: "bad".into(),
        width: 1920,
        height: 1080,
        duration_sec: 120.0,
        fps: 30,
        bg: None,
        html: None,
        template: None,
        design_md: None,
        session_id: None,
    };
    let err = svc.create_composition(req).await.unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("duration") || msg.contains("60"),
        "error should mention duration: {}",
        msg
    );
}

#[tokio::test]
async fn test_job_registry_create_and_lookup() {
    use nevoflux_daemon::canvas_video::job::{JobRegistry, JobState};

    let reg = JobRegistry::new();
    let job_id = reg.create("comp-xyz".into(), 1920, 1080, 5.0, 30).await;
    assert!(job_id.starts_with("job-"));

    let snapshot = reg.snapshot(&job_id).await.expect("job exists");
    assert_eq!(snapshot.state, JobState::Queued);
    assert_eq!(snapshot.composition_id, "comp-xyz");
    assert_eq!(snapshot.total_frames, 150); // 5s × 30fps
}

#[tokio::test]
async fn test_job_state_transitions() {
    use nevoflux_daemon::canvas_video::job::{JobRegistry, JobState};

    let reg = JobRegistry::new();
    let job_id = reg.create("comp-a".into(), 640, 360, 1.0, 30).await;

    reg.set_state(&job_id, JobState::Running).await;
    assert_eq!(
        reg.snapshot(&job_id).await.unwrap().state,
        JobState::Running
    );

    reg.set_progress(&job_id, 15, "encoding frame 15/30".into())
        .await;
    let s = reg.snapshot(&job_id).await.unwrap();
    assert_eq!(s.current_frame, 15);
    assert_eq!(s.step, "encoding frame 15/30");

    reg.set_state(&job_id, JobState::Succeeded).await;
    assert_eq!(
        reg.snapshot(&job_id).await.unwrap().state,
        JobState::Succeeded
    );
}

#[tokio::test]
async fn test_job_cancel_transitions_to_cancelled() {
    use nevoflux_daemon::canvas_video::job::{JobRegistry, JobState};

    let reg = JobRegistry::new();
    let job_id = reg.create("comp-a".into(), 640, 360, 1.0, 30).await;
    reg.set_state(&job_id, JobState::Running).await;
    let cancelled = reg.cancel(&job_id).await;
    assert!(cancelled);
    assert_eq!(
        reg.snapshot(&job_id).await.unwrap().state,
        JobState::Cancelled
    );
}

#[tokio::test]
async fn test_render_start_creates_job_and_returns_id() {
    use nevoflux_daemon::canvas_video::job::JobState;
    use nevoflux_protocol::canvas_video::RenderStartRequest;

    let svc = fresh_service();
    let create_resp = svc
        .create_composition(CreateCompositionRequest {
            title: "demo".into(),
            width: 640,
            height: 360,
            duration_sec: 1.0,
            fps: 30,
            bg: None,
            html: Some(
                r#"<!doctype html><div id="stage" data-width="640" data-height="360" data-duration="1" data-fps="30"></div>"#
                    .into(),
            ),
            template: None,
            design_md: None,
            session_id: None,
        })
        .await
        .unwrap();

    let start_resp = svc
        .render_start(RenderStartRequest {
            composition_id: create_resp.artifact_id.clone(),
        })
        .await
        .unwrap();

    assert!(start_resp.job_id.starts_with("job-"));

    // Job is now tracked; state is Queued, Running, or Succeeded (stub
    // bridge short-circuits to Succeeded but the spawned task may run
    // before or after this observation).
    let snap = svc.job_snapshot(&start_resp.job_id).await.unwrap();
    assert!(matches!(
        snap.state,
        JobState::Queued | JobState::Running | JobState::Succeeded
    ));
    assert_eq!(snap.composition_id, create_resp.artifact_id);
    assert_eq!(snap.total_frames, 30);
}

/// Caller-supplied DESIGN.md → daemon parses frontmatter and injects a
/// `<style data-nf-design-tokens>` block at the top of the generated
/// `index.html`. The composition is created via the `html` path (no skill
/// registry needed) so the test runs in isolation.
#[tokio::test]
async fn test_create_with_caller_design_md_injects_tokens() {
    let svc = fresh_service();
    let req = CreateCompositionRequest {
        title: "design-md-test".into(),
        width: 640,
        height: 360,
        duration_sec: 1.0,
        fps: 30,
        bg: None,
        html: Some("<!doctype html><html><head><title>T</title></head><body><div id='stage'>X</div></body></html>".into()),
        template: None,
        design_md: Some(SAMPLE_DESIGN_MD.to_string()),
        session_id: None,
    };
    let resp = svc.create_composition(req).await.unwrap();

    use nevoflux_storage::repositories::ArtifactRepository;
    let storage = svc.storage().unwrap().clone();
    let repo = ArtifactRepository::new(storage.database());
    let rec = repo.get(&resp.artifact_id).unwrap().unwrap();
    let files = rec.files.expect("multi-file artifact");

    // index.html got the marked block with the caller's primary color.
    let index = files.get("index.html").expect("index.html present");
    assert!(
        index.contains("data-nf-design-tokens"),
        "marked block missing in index.html"
    );
    assert!(
        index.contains("--color-primary: #ff6600;"),
        "expected --color-primary from caller's DESIGN.md, got: {index:?}"
    );
    // Original body content survived the injection.
    assert!(index.contains("<div id='stage'>X</div>"));

    // DESIGN.md stored verbatim (the injection source-of-truth, used by apply).
    let stored_md = files.get("DESIGN.md").expect("DESIGN.md present");
    assert!(stored_md.contains("primary: \"#ff6600\""));

    // content field synced with the injected index.html.
    assert_eq!(rec.content, *index);
}

/// `canvas_apply_design_md` re-runs token injection from the artifact's
/// stored DESIGN.md. After the user edits DESIGN.md and calls apply, only
/// the `<style data-nf-design-tokens>` block changes — copy/text/CSS edits
/// elsewhere in `index.html` survive byte-identical.
#[tokio::test]
async fn test_apply_design_md_replaces_only_marked_block() {
    let svc = fresh_service();
    let req = CreateCompositionRequest {
        title: "apply-test".into(),
        width: 640,
        height: 360,
        duration_sec: 1.0,
        fps: 30,
        bg: None,
        html: Some("<!doctype html><html><head><title>T</title></head><body><h1 id='headline'>Original headline</h1><div id='stage'>BODY</div></body></html>".into()),
        template: None,
        design_md: Some(SAMPLE_DESIGN_MD.to_string()),
        session_id: None,
    };
    let resp = svc.create_composition(req).await.unwrap();

    use nevoflux_storage::repositories::ArtifactRepository;
    let storage = svc.storage().unwrap().clone();
    let repo = ArtifactRepository::new(storage.database());

    // Capture index.html after creation.
    let before = repo.get(&resp.artifact_id).unwrap().unwrap();
    let before_files = before.files.unwrap();
    let before_index = before_files.get("index.html").unwrap().clone();

    // User edits DESIGN.md (simulating a Canvas Editor save): primary becomes green.
    let altered_md = SAMPLE_DESIGN_MD.replace("#ff6600", "#00ff00");
    let mut altered_files = before_files.clone();
    altered_files.insert("DESIGN.md".to_string(), altered_md);
    repo.update_files(&resp.artifact_id, &altered_files, &before_index)
        .unwrap();

    // Apply re-injection.
    svc.apply_design_md(&resp.artifact_id).await.unwrap();

    // Assert: only the marked block changed; everything else byte-identical.
    let after = repo.get(&resp.artifact_id).unwrap().unwrap();
    let after_files = after.files.unwrap();
    let after_index = after_files.get("index.html").unwrap().clone();

    assert!(
        after_index.contains("--color-primary: #00ff00;"),
        "expected new green primary, got: {after_index}"
    );
    assert!(
        !after_index.contains("--color-primary: #ff6600;"),
        "old primary should be gone, got: {after_index}"
    );
    // Content outside the marked block survives.
    assert!(after_index.contains("<h1 id='headline'>Original headline</h1>"));
    assert!(after_index.contains("<div id='stage'>BODY</div>"));
    assert!(after_index.contains("<title>T</title>"));

    // Idempotent: applying again produces the same result.
    svc.apply_design_md(&resp.artifact_id).await.unwrap();
    let after2 = repo.get(&resp.artifact_id).unwrap().unwrap();
    let after2_files = after2.files.unwrap();
    let after2_index = after2_files.get("index.html").unwrap();
    assert_eq!(after2_index, &after_index, "apply is not idempotent");
}

/// Without caller-supplied design_md AND with no skills loaded (the test
/// fixture's empty SkillRegistry), composition creation MUST still
/// succeed — the skill-aux read failures degrade silently to an empty
/// DESIGN.md, the inject step becomes a no-op, and the raw template HTML
/// is stored.
#[tokio::test]
async fn test_create_without_design_md_falls_back_silently() {
    let svc = fresh_service();
    let req = CreateCompositionRequest {
        title: "fallback".into(),
        width: 640,
        height: 360,
        duration_sec: 1.0,
        fps: 30,
        bg: None,
        html: Some(
            "<!doctype html><html><head><title>T</title></head><body>Y</body></html>".into(),
        ),
        template: None,
        design_md: None,
        session_id: None,
    };
    let resp = svc.create_composition(req).await.unwrap();
    use nevoflux_storage::repositories::ArtifactRepository;
    let storage = svc.storage().unwrap().clone();
    let repo = ArtifactRepository::new(storage.database());
    let rec = repo.get(&resp.artifact_id).unwrap().unwrap();
    let files = rec.files.unwrap();

    // index.html present; design tokens block may or may not appear depending
    // on whether the test fixture skill registry exposes a video skill.
    let index = files.get("index.html").unwrap();
    assert!(index.contains("<title>T</title>"));
    // DESIGN.md slot exists (might be empty string if no skill registry).
    assert!(files.contains_key("DESIGN.md"));
}

#[tokio::test]
async fn test_render_start_rejects_unknown_composition() {
    use nevoflux_protocol::canvas_video::RenderStartRequest;

    let svc = fresh_service();
    let err = svc
        .render_start(RenderStartRequest {
            composition_id: "comp-does-not-exist".into(),
        })
        .await
        .unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("not found") || msg.contains("unknown"),
        "unexpected err: {}",
        msg
    );
}

/// End-to-end integration: drive the page-driven signal channel with fake frame
/// chunks + render_done, and assert ffmpeg produced a non-empty MP4.
///
/// Skipped silently when ffmpeg is unavailable so CI without ffmpeg on PATH
/// doesn't flake.
#[tokio::test]
async fn test_render_loop_reassembles_and_encodes() {
    use std::time::Duration;

    use ffmpeg_sidecar::command::ffmpeg_is_installed;
    use image::{ImageBuffer, Rgba};
    use nevoflux_daemon::canvas_video::job::JobState;
    use nevoflux_protocol::canvas_video::{RenderFrameChunk, RenderStartRequest};

    if !ffmpeg_is_installed() {
        eprintln!("ffmpeg not on PATH — skipping test_render_loop_reassembles_and_encodes");
        return;
    }

    // Production service (no stub short-circuit) so the full render loop runs.
    let svc = Arc::new(CanvasVideoService::new());

    // Keep the render tiny so the test finishes in well under a second:
    // 12 frames at 24 fps = 0.5 s composition (the configured minimum).
    let fps: u32 = 24;
    let duration_sec: f32 = 0.5;
    let total_frames: u32 = (duration_sec * fps as f32).ceil() as u32;

    let create_resp = svc
        .create_composition(CreateCompositionRequest {
            title: "integration".into(),
            width: 16,
            height: 16,
            duration_sec,
            fps,
            bg: None,
            html: Some("<!doctype html><body></body>".into()),
            template: None,
            design_md: None,
            session_id: None,
        })
        .await
        .unwrap();

    let start_resp = svc
        .render_start(RenderStartRequest {
            composition_id: create_resp.artifact_id.clone(),
        })
        .await
        .unwrap();
    let job_id = start_resp.job_id.clone();

    // Build a 16x16 PNG with varying color per frame to avoid degenerate encoder paths.
    let make_png = |frame: u32| -> Vec<u8> {
        let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(16, 16);
        let r = (frame * 40) as u8;
        for (_, _, px) in img.enumerate_pixels_mut() {
            *px = Rgba([r, 128, 255 - r, 255]);
        }
        let mut buf: Vec<u8> = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    };

    // Give the render loop a tick to register its signal channel before we
    // start pushing frames. The channel is registered inside the spawned task.
    tokio::time::sleep(Duration::from_millis(50)).await;

    for frame_idx in 0..total_frames {
        let png = make_png(frame_idx);
        // Send as a single chunk (is_last=true, total_chunks=1).
        svc.on_frame_chunk(RenderFrameChunk {
            job_id: job_id.clone(),
            frame_idx,
            chunk_idx: 0,
            total_chunks: 1,
            is_last: true,
            bytes: png,
        })
        .await
        .unwrap();
    }

    svc.on_render_done(&job_id, total_frames).await;

    // Poll until the job is Succeeded or Failed (max ~15s — ffmpeg startup
    // + 5 tiny PNGs should finish in under a second).
    let mut final_state = JobState::Running;
    for _ in 0..150 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Some(snap) = svc.job_snapshot(&job_id).await {
            match snap.state {
                JobState::Succeeded | JobState::Failed | JobState::Cancelled => {
                    final_state = snap.state;
                    break;
                }
                _ => {}
            }
        }
    }

    assert_eq!(
        final_state,
        JobState::Succeeded,
        "job did not reach Succeeded (final={:?})",
        final_state
    );

    // Verify the MP4 was written and is non-trivially sized.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let output_path = std::path::PathBuf::from(home)
        .join(".cache")
        .join("nevoflux")
        .join("render")
        .join(format!("{}.mp4", job_id));
    let metadata = std::fs::metadata(&output_path)
        .unwrap_or_else(|e| panic!("output not found at {:?}: {}", output_path, e));
    assert!(
        metadata.len() > 0,
        "output MP4 is empty at {:?}",
        output_path
    );

    // Best-effort cleanup; don't fail the test if removal fails.
    let _ = std::fs::remove_file(&output_path);
}
