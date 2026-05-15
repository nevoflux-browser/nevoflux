use nevoflux_protocol::chat::Artifact;

#[test]
fn test_artifact_wire_compat_missing_is_persistent() {
    // Old JSON without is_persistent field (from older senders)
    let old_json = r#"{"id":"art-001","title":"My Dashboard","content_type":"text/html","description":null,"content":"<html><body><h1>Hello</h1></body></html>","files":null,"entry":null}"#;
    let artifact: Artifact = serde_json::from_str(old_json).expect("Should deserialize");
    assert_eq!(
        artifact.is_persistent, false,
        "Missing is_persistent should default to false"
    );
    assert_eq!(artifact.id, "art-001");
}

#[test]
fn test_artifact_wire_compat_with_is_persistent_true() {
    // New JSON with is_persistent explicitly set to true
    let new_json = r#"{"id":"art-002","title":"My Dashboard","content_type":"text/html","description":null,"content":"<html><body><h1>Hello</h1></body></html>","files":null,"entry":null,"is_persistent":true}"#;
    let artifact: Artifact = serde_json::from_str(new_json).expect("Should deserialize");
    assert_eq!(artifact.is_persistent, true);
    assert_eq!(artifact.id, "art-002");
}

#[test]
fn test_artifact_wire_compat_with_is_persistent_false() {
    // New JSON with is_persistent explicitly set to false
    let new_json = r#"{"id":"art-003","title":"My Dashboard","content_type":"text/html","description":null,"content":"<html><body><h1>Hello</h1></body></html>","files":null,"entry":null,"is_persistent":false}"#;
    let artifact: Artifact = serde_json::from_str(new_json).expect("Should deserialize");
    assert_eq!(artifact.is_persistent, false);
    assert_eq!(artifact.id, "art-003");
}

use nevoflux_protocol::canvas_video::*;

#[test]
fn test_create_composition_request_roundtrip() {
    let req = CreateCompositionRequest {
        title: "demo".into(),
        width: 1920,
        height: 1080,
        duration_sec: 5.0,
        fps: 30,
        bg: None,
        html: Some("<html></html>".into()),
        template: None,
        design_md: None,
        session_id: None,
    };
    let s = serde_json::to_string(&req).unwrap();
    let r: CreateCompositionRequest = serde_json::from_str(&s).unwrap();
    assert_eq!(r.title, "demo");
    assert_eq!(r.width, 1920);
    assert_eq!(r.html, Some("<html></html>".into()));
}

#[test]
fn test_frame_chunk_roundtrip_large() {
    let chunk = RenderFrameChunk {
        job_id: "job-1".into(),
        frame_idx: 42,
        chunk_idx: 0,
        total_chunks: 2,
        is_last: false,
        bytes: vec![0xAA; 1024 * 1024], // 1 MB
    };
    let s = serde_json::to_string(&chunk).unwrap();
    let r: RenderFrameChunk = serde_json::from_str(&s).unwrap();
    assert_eq!(r.bytes.len(), 1024 * 1024);
    assert_eq!(r.bytes[0], 0xAA);
}
