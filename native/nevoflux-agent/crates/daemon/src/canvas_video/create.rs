//! canvas.video.create_composition — persists to ArtifactRepository.

use std::collections::HashMap;

use crate::canvas_video::CanvasVideoService;
use crate::error::{DaemonError, Result};
use nevoflux_protocol::canvas_video::{
    CompositionKind, CompositionMeta, CompositionOrigin, CompositionSpec, CreateCompositionRequest,
    CreateCompositionResponse,
};
use nevoflux_storage::repositories::ArtifactRepository;
use nevoflux_storage::CreateArtifactParams;

const ALLOWED_FPS: &[u32] = &[24, 25, 30];
const MAX_DIMENSION: u32 = 1920;
const MAX_DURATION_SEC: f32 = 60.0;
const MIN_DURATION_SEC: f32 = 0.5;
pub async fn create(
    svc: &CanvasVideoService,
    req: CreateCompositionRequest,
) -> Result<CreateCompositionResponse> {
    validate(&req)?;

    let index_html_raw = resolve_index_html(svc, &req).await?;
    let design_md = resolve_design_md(svc, &req).await?;
    // Inject DESIGN.md tokens into a marked <style> block at the top of <head>.
    // Failure is non-fatal: invalid DESIGN.md falls back to the un-injected
    // HTML so a broken brand layer doesn't block composition creation; the
    // template's own var(--x, fallback) values still produce a usable canvas.
    let index_html =
        match crate::canvas_video::design::inject_design_tokens(&index_html_raw, &design_md) {
            Ok(html) => html,
            Err(e) => {
                tracing::warn!(
                "canvas_video::create: design token injection failed ({e}); using raw template HTML"
            );
                index_html_raw
            }
        };
    let meta = build_meta(&req);
    let meta_json = serde_json::to_string_pretty(&meta)
        .map_err(|e| DaemonError::InternalError(format!("meta serialize: {e}")))?;

    let storage = svc
        .storage()
        .ok_or_else(|| DaemonError::InternalError("canvas_video: storage not wired".into()))?;
    let repo = ArtifactRepository::new(storage.database());

    let artifact_id = format!("comp-{}", uuid::Uuid::new_v4().simple());

    let mut files = HashMap::new();
    files.insert("index.html".to_string(), index_html.clone());
    // Store the original DESIGN.md (pre-render-time source-of-truth). The
    // injected tokens live inside index.html; canvas_apply_design_md re-runs
    // injection from this stored DESIGN.md after the user edits it.
    files.insert("DESIGN.md".to_string(), design_md);
    files.insert("composition.meta.json".to_string(), meta_json);

    let params = CreateArtifactParams {
        id: artifact_id.clone(),
        // Pass the caller-provided session_id directly; None means orphan (NULL in DB).
        // The sessions FK allows NULL; non-NULL values must reference an existing session.
        session_id: req.session_id.clone(),
        title: req.title.clone(),
        description: None,
        content_type: "text/html".into(),
        // `content` kept in sync with `files["index.html"]` so existing
        // Canvas preview iframes that read flat `content` still work.
        content: index_html,
        files: Some(files),
        entry: Some("index.html".into()),
    };
    repo.create(params)
        .map_err(|e| DaemonError::InternalError(format!("{e}")))?;

    // Auto-persist the composition so it appears in the My Canvas list by
    // default. Compositions are designed to be reused / re-rendered / shared;
    // landing as a non-persistent artifact hides them behind a UI the user
    // has to discover. Flips `is_persistent = 1` with the current timestamp,
    // mirroring what `canvas_persist::service::save` does for manual pins.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    storage
        .database()
        .with_connection(|conn| {
            conn.execute(
                "UPDATE artifacts SET is_persistent = 1, persisted_at = ?1 WHERE id = ?2",
                rusqlite::params![now, artifact_id],
            )?;
            Ok::<(), nevoflux_storage::StorageError>(())
        })
        .map_err(|e| DaemonError::InternalError(format!("auto-persist composition: {e}")))?;

    Ok(CreateCompositionResponse { artifact_id })
}

fn validate(req: &CreateCompositionRequest) -> Result<()> {
    if !ALLOWED_FPS.contains(&req.fps) {
        return Err(DaemonError::InvalidRequest(format!(
            "invalid fps {}: allowed {:?}",
            req.fps, ALLOWED_FPS
        )));
    }
    if req.duration_sec < MIN_DURATION_SEC || req.duration_sec > MAX_DURATION_SEC {
        return Err(DaemonError::InvalidRequest(format!(
            "duration_sec {} outside [{}, {}]",
            req.duration_sec, MIN_DURATION_SEC, MAX_DURATION_SEC
        )));
    }
    if req.width == 0 || req.width > MAX_DIMENSION || req.height == 0 || req.height > MAX_DIMENSION
    {
        return Err(DaemonError::InvalidRequest(format!(
            "dimensions {}x{} out of range (max {})",
            req.width, req.height, MAX_DIMENSION
        )));
    }
    Ok(())
}

async fn resolve_index_html(
    svc: &CanvasVideoService,
    req: &CreateCompositionRequest,
) -> Result<String> {
    // 1. Explicit html override wins.
    if let Some(raw) = &req.html {
        if req.template.is_some() {
            tracing::warn!("canvas_create_composition: both html and template given; html wins");
        }
        return Ok(raw.clone());
    }
    // 2. Template via skill registry.
    if let Some(tpl) = &req.template {
        let path = format!("templates/{tpl}.html");
        let reg = svc.skills().ok_or_else(|| {
            DaemonError::InternalError("canvas_video: skill registry not wired".into())
        })?;
        let body = reg
            .read()
            .await
            .read_auxiliary_file("video", &path)
            .map_err(|_| DaemonError::SkillAssetNotFound {
                skill: "video".into(),
                path: path.clone(),
            })?;
        return substitute_placeholders(&body, req, tpl);
    }
    // 3. Reject — neither template nor html supplied. Returning a 500-byte
    //    default scaffold here silently masks LLM mistakes (it omitted both
    //    fields), and downstream the composition has no real content. Surface
    //    the error so the agent retries with template or html.
    Err(DaemonError::InvalidRequest(
        "canvas_create_composition: must provide either `template` (one of: \
         website-promo-16x9, product-intro-16x9, product-intro-9x16, tiktok-hook, \
         video-overlay, logo-3d-reveal, product-3d-spin) or `html` (raw composition body). \
         Both are missing."
            .into(),
    ))
}

/// Resolve the DESIGN.md content for a new composition.
///
/// Resolution priority:
/// 1. Caller-supplied `req.design_md` wins outright (treats whitespace-only
///    as "not supplied" so accidental empty strings don't blank the brand).
/// 2. Otherwise, when `req.template` is set, look up the template-specific
///    `templates/<name>.design.md` — each shipped template has its own brand
///    default matching its CSS `:root` fallback values.
/// 3. Final fallback to the generic `reference/DESIGN-template.md` (used by
///    `html`-mode compositions and as a last-resort default).
///
/// All read failures degrade silently to `String::new()` so a missing
/// auxiliary file never blocks composition creation; the daemon's
/// `inject_design_tokens` step then becomes a no-op.
async fn resolve_design_md(
    svc: &CanvasVideoService,
    req: &CreateCompositionRequest,
) -> Result<String> {
    if let Some(md) = &req.design_md {
        if !md.trim().is_empty() {
            return Ok(md.clone());
        }
    }
    let reg = svc.skills().ok_or_else(|| {
        DaemonError::InternalError("canvas_video: skill registry not wired".into())
    })?;
    let guard = reg.read().await;
    if let Some(tpl) = &req.template {
        let path = format!("templates/{tpl}.design.md");
        if let Ok(s) = guard.read_auxiliary_file("video", &path) {
            return Ok(s);
        }
    }
    match guard.read_auxiliary_file("video", "reference/DESIGN-template.md") {
        Ok(s) => Ok(s),
        Err(_) => Ok(String::new()), // empty fallback per spec §10.1
    }
}

fn build_meta(req: &CreateCompositionRequest) -> CompositionMeta {
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    CompositionMeta {
        kind: CompositionKind::Composition,
        version: 1,
        spec: CompositionSpec {
            width: req.width,
            height: req.height,
            duration_sec: req.duration_sec,
            fps: req.fps,
            bg: req.bg.clone(),
        },
        origin: CompositionOrigin {
            template: req.template.clone(),
            created_with: "canvas_create_composition".into(),
            created_at,
        },
    }
}

fn substitute_placeholders(
    body: &str,
    req: &CreateCompositionRequest,
    _template: &str,
) -> Result<String> {
    let bg = req.bg.clone().unwrap_or_else(|| "#000".into());
    // All placeholders are optional. Templates may hardcode their canonical
    // dimensions in data-width / data-height attributes (the convention for
    // the seven shipped /video templates: each template's aspect is part of
    // its identity, e.g. tiktok-hook is intrinsically 9:16 at 1080x1920),
    // OR they may use {{width}}/{{height}}/{{duration}}/{{fps}}/{{bg}} for
    // parameterization. Missing placeholders are not an error -- absent
    // means "the template doesn't take that parameter at materialization
    // time," not "broken template."
    let replacements = [
        ("{{width}}", req.width.to_string()),
        ("{{height}}", req.height.to_string()),
        ("{{duration}}", req.duration_sec.to_string()),
        ("{{fps}}", req.fps.to_string()),
        ("{{bg}}", bg),
    ];
    let mut out = body.to_string();
    for (needle, value) in &replacements {
        if body.contains(needle) {
            out = out.replace(needle, value);
        }
    }
    Ok(out)
}

/// Public wrapper over `default_scaffold` so callers outside this module can
/// regenerate the same default HTML given a request.
pub fn default_scaffold_for(req: &CreateCompositionRequest) -> String {
    default_scaffold(req)
}

fn default_scaffold(req: &CreateCompositionRequest) -> String {
    let bg = req.bg.clone().unwrap_or_else(|| "#000".into());
    format!(
        r##"<!doctype html>
<html>
<head><meta charset="utf-8"><style>
  body {{ margin: 0; background: {bg}; }}
  #stage {{ position: relative; width: {w}px; height: {h}px; overflow: hidden; }}
</style></head>
<body>
  <div id="stage"
       data-composition-id="{title}"
       data-width="{w}"
       data-height="{h}"
       data-duration="{d}"
       data-fps="{fps}"
       data-bg="{bg}">
  </div>
  <script type="module">
    window.__timelines = window.__timelines || [];
  </script>
</body>
</html>"##,
        bg = bg,
        title = req.title,
        w = req.width,
        h = req.height,
        d = req.duration_sec,
        fps = req.fps,
    )
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    use crate::canvas_video::CanvasVideoService;
    use nevoflux_protocol::canvas_video::{CompositionKind, CompositionMeta};
    use nevoflux_storage::repositories::{ArtifactRepository, SessionRepository};
    use nevoflux_storage::CreateSessionParams;
    use std::sync::Arc;

    fn mk_req(title: &str) -> CreateCompositionRequest {
        CreateCompositionRequest {
            title: title.into(),
            width: 640,
            height: 360,
            duration_sec: 5.0,
            fps: 30,
            bg: Some("#000".into()),
            // resolve_index_html now rejects creates with neither template nor
            // html, so seed a minimal html for unit-test fixtures.
            html: Some("<html><body></body></html>".into()),
            template: None,
            design_md: None,
            session_id: None,
        }
    }

    #[tokio::test]
    async fn create_writes_row_with_three_files() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let resp = svc.create_composition(mk_req("hello")).await.unwrap();
        let storage = svc.storage().unwrap().clone();
        let repo = ArtifactRepository::new(storage.database());
        let rec = repo.get(&resp.artifact_id).unwrap().expect("row exists");
        assert_eq!(rec.title, "hello");
        assert_eq!(rec.content_type, "text/html");
        assert_eq!(rec.entry.as_deref(), Some("index.html"));
        let files = rec.files.as_ref().expect("files map");
        assert!(files.contains_key("index.html"));
        assert!(files.contains_key("DESIGN.md"));
        assert!(files.contains_key("composition.meta.json"));
        let meta: CompositionMeta =
            serde_json::from_str(files.get("composition.meta.json").unwrap()).unwrap();
        assert_eq!(meta.kind, CompositionKind::Composition);
        assert_eq!(meta.spec.width, 640);
        assert_eq!(meta.spec.fps, 30);
    }

    #[tokio::test]
    async fn create_uses_html_override_when_given() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let mut req = mk_req("h");
        req.html = Some("<!doctype html><body>OVERRIDE</body>".into());
        let resp = svc.create_composition(req).await.unwrap();
        let repo = ArtifactRepository::new(svc.storage().unwrap().database());
        let rec = repo.get(&resp.artifact_id).unwrap().unwrap();
        assert!(rec
            .files
            .unwrap()
            .get("index.html")
            .unwrap()
            .contains("OVERRIDE"));
    }

    #[tokio::test]
    async fn create_defaults_session_id_when_missing() {
        // When no session_id is provided, the artifact is orphan (NULL session_id).
        // The sessions FK allows NULL; we do not invent a fake session string.
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let resp = svc.create_composition(mk_req("h")).await.unwrap();
        let repo = ArtifactRepository::new(svc.storage().unwrap().database());
        let rec = repo.get(&resp.artifact_id).unwrap().unwrap();
        assert!(
            rec.session_id.is_none(),
            "orphan artifact must have NULL session_id, got {:?}",
            rec.session_id
        );
    }

    #[tokio::test]
    async fn create_honors_given_session_id() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let storage = svc.storage().unwrap().clone();

        // The sessions FK is enforced; create the session before inserting the artifact.
        let session_id = "sess-xyz";
        SessionRepository::new(storage.database())
            .create(CreateSessionParams::new().with_id(session_id))
            .expect("create session");

        let mut req = mk_req("h");
        req.session_id = Some(session_id.into());
        let resp = svc.create_composition(req).await.unwrap();
        let repo = ArtifactRepository::new(storage.database());
        let rec = repo.get(&resp.artifact_id).unwrap().unwrap();
        assert_eq!(rec.session_id.as_deref(), Some("sess-xyz"));
    }

    #[tokio::test]
    async fn create_rejects_missing_template() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let mut req = mk_req("h");
        // mk_req seeds html so the create won't fall through to the
        // "neither field set" rejection path; null html here to force the
        // template lookup, which is what this test exercises.
        req.html = None;
        req.template = Some("does-not-exist".into());
        let err = svc.create_composition(req).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("skill asset not found") || msg.contains("SkillAssetNotFound"),
            "got: {msg}"
        );
    }

    #[tokio::test]
    async fn create_rejects_when_both_template_and_html_missing() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let mut req = mk_req("h");
        req.html = None;
        req.template = None;
        let err = svc.create_composition(req).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("must provide either") && msg.contains("template") && msg.contains("html"),
            "got: {msg}"
        );
    }

    #[tokio::test]
    async fn create_keeps_content_in_sync_with_entry() {
        let svc = Arc::new(CanvasVideoService::new_for_tests());
        let resp = svc.create_composition(mk_req("h")).await.unwrap();
        let repo = ArtifactRepository::new(svc.storage().unwrap().database());
        let rec = repo.get(&resp.artifact_id).unwrap().unwrap();
        let files = rec.files.as_ref().unwrap();
        assert_eq!(rec.content, *files.get("index.html").unwrap());
    }
}
