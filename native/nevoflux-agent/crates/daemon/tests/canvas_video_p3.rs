//! P3 integration — composition via ArtifactRepository + render read-through.

use std::sync::Arc;

use nevoflux_daemon::canvas_video::CanvasVideoService;
use nevoflux_protocol::canvas_video::{CompositionMeta, CreateCompositionRequest};
use nevoflux_storage::models::CreateSessionParams;
use nevoflux_storage::repositories::{ArtifactRepository, SessionRepository};

#[ignore]
#[tokio::test]
async fn create_then_load_reads_from_repo() {
    let svc = Arc::new(CanvasVideoService::new_for_tests());
    let storage = svc.storage().unwrap().clone();

    // The sessions FK is enforced; create the session before inserting the artifact.
    let session_id = "integration-session";
    SessionRepository::new(storage.database())
        .create(CreateSessionParams::new().with_id(session_id))
        .expect("create session");

    let req = CreateCompositionRequest {
        title: "int-test".into(),
        width: 320,
        height: 240,
        duration_sec: 1.0,
        fps: 30,
        bg: Some("#000".into()),
        html: Some("<!doctype html><body><div id='stage' style='width:320px;height:240px;background:red'></div></body>".into()),
        template: None,
        design_md: None,
        session_id: Some(session_id.into()),
    };
    let resp = svc.create_composition(req).await.unwrap();
    let (html, w, h, d, fps) = svc.load_composition(&resp.artifact_id).await.unwrap();
    assert!(html.contains("stage"));
    assert_eq!((w, h, fps), (320, 240, 30));
    assert!((d - 1.0).abs() < 1e-3);

    // Verify the row landed with 3 files.
    let repo = ArtifactRepository::new(storage.database());
    let rec = repo.get(&resp.artifact_id).unwrap().unwrap();
    let files = rec.files.unwrap();
    assert!(files.contains_key("index.html"));
    assert!(files.contains_key("DESIGN.md"));
    assert!(files.contains_key("composition.meta.json"));
    let meta: CompositionMeta =
        serde_json::from_str(files.get("composition.meta.json").unwrap()).unwrap();
    meta.validate_hard_limits().unwrap();
}
