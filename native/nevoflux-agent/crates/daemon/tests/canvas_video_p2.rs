//! P2 integration tests (live-browser, `#[ignore]` gated).
//!
//! Run procedure is the same as the PoC gate: start daemon (writes
//! daemon.port), launch browser with extension pointing at that daemon,
//! then:
//!
//! ```text
//! cargo test -p nevoflux-daemon --test canvas_video_p2 \
//!     -- --ignored --nocapture
//! ```

mod common;
use common::tcp_client;

use std::time::{Duration, Instant};
use tokio::time::timeout;

fn fixture_html() -> String {
    include_str!("fixtures/poc-composition.html").to_string()
}

/// Start a 150-frame render; after the first progress event cancel it;
/// assert the next terminal event on jobs:render:{id} is `cancelled`
/// and no `succeeded` arrives.
#[tokio::test]
#[ignore]
async fn cancel_mid_render_emits_cancelled_event() {
    let port = tcp_client::discover_port().expect("find daemon port");
    eprintln!("[p2.cancel] port={}", port);

    let mut client = tcp_client::PocClient::connect(port, "proxy-p2-cancel")
        .await
        .expect("connect");

    // Create a 5 s × 640×360 composition.
    let create_payload = serde_json::json!({
        "type": "canvas_video_create_composition",
        "payload": {
            "title": "p2-cancel",
            "width": 640,
            "height": 360,
            "duration_sec": 5.0,
            "fps": 30,
            "bg": "#000",
            "html": fixture_html(),
        }
    });
    let create_rid = client.send_chat(create_payload).await.expect("send create");
    let create_resp = client
        .recv_matching(Duration::from_secs(10), |env| {
            env.payload.get("type").and_then(|v| v.as_str())
                == Some("canvas_video_create_composition_response")
                && env.request_id.as_deref() == Some(&create_rid)
        })
        .await
        .expect("create response");
    let artifact_id = create_resp
        .payload
        .get("payload")
        .and_then(|p| p.get("artifact_id"))
        .and_then(|v| v.as_str())
        .expect("artifact_id")
        .to_owned();

    // Subscribe BEFORE starting the render.
    let sub_payload = serde_json::json!({
        "type": "events_request",
        "payload": {
            "action": "subscribe",
            "patterns": ["jobs:render:*"],
            "replay_sticky": false,
            "buffer_size": 1024,
        }
    });
    let sub_rid = client.send_chat(sub_payload).await.expect("send subscribe");
    let sub_resp = client
        .recv_matching(Duration::from_secs(5), |env| {
            env.payload.get("type").and_then(|v| v.as_str()) == Some("events_response")
                && env.request_id.as_deref() == Some(&sub_rid)
        })
        .await
        .expect("subscribe response");
    assert_eq!(
        sub_resp
            .payload
            .get("payload")
            .and_then(|p| p.get("result"))
            .and_then(|v| v.as_str()),
        Some("subscribed"),
        "subscribe denied"
    );

    // Start render.
    let start_payload = serde_json::json!({
        "type": "canvas_video_render_start",
        "payload": { "composition_id": artifact_id }
    });
    let start_rid = client.send_chat(start_payload).await.expect("send start");
    let start_resp = client
        .recv_matching(Duration::from_secs(10), |env| {
            env.payload.get("type").and_then(|v| v.as_str())
                == Some("canvas_video_render_start_response")
                && env.request_id.as_deref() == Some(&start_rid)
        })
        .await
        .expect("render_start response");
    let job_id = start_resp
        .payload
        .get("payload")
        .and_then(|p| p.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_owned();
    eprintln!("[p2.cancel] job_id={}", job_id);

    // Wait for the first progress event so we know rendering has begun.
    let _first = client
        .recv_matching(Duration::from_secs(30), |env| {
            if env.payload.get("type").and_then(|v| v.as_str()) != Some("events_delivery") {
                return false;
            }
            let Some(event) = env.payload.get("payload").and_then(|p| p.get("event")) else {
                return false;
            };
            let topic = event.get("topic").and_then(|v| v.as_str()).unwrap_or("");
            if topic != format!("jobs:render:{}", job_id) {
                return false;
            }
            event
                .get("payload")
                .and_then(|p| p.get("event"))
                .and_then(|v| v.as_str())
                == Some("progress")
        })
        .await
        .expect("first progress");
    eprintln!("[p2.cancel] saw first progress, issuing cancel");

    // Send cancel.
    let cancel_payload = serde_json::json!({
        "type": "canvas_video_render_cancel",
        "payload": { "job_id": job_id }
    });
    client.send_chat(cancel_payload).await.expect("send cancel");

    // Consume events until we see cancelled OR succeeded (fail on succeeded).
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut saw_cancelled = false;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Ok(Some(env)) = timeout(remaining, client.recv()).await else {
            break;
        };
        if env.payload.get("type").and_then(|v| v.as_str()) != Some("events_delivery") {
            continue;
        }
        let Some(event) = env.payload.get("payload").and_then(|p| p.get("event")) else {
            continue;
        };
        let topic = event.get("topic").and_then(|v| v.as_str()).unwrap_or("");
        if topic != format!("jobs:render:{}", job_id) {
            continue;
        }
        let body = event.get("payload").cloned().unwrap_or_default();
        match body.get("event").and_then(|v| v.as_str()) {
            Some("cancelled") => {
                let current = body.get("current").and_then(|v| v.as_u64()).unwrap_or(0);
                let total = body.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
                eprintln!("[p2.cancel] cancelled: current={}/{}", current, total);
                assert!(
                    current > 0,
                    "expected at least one rendered frame before cancel"
                );
                assert_eq!(total, 150, "expected total==150");
                saw_cancelled = true;
                break;
            }
            Some("succeeded") => {
                panic!("render succeeded before cancel was honored");
            }
            _ => {}
        }
    }
    assert!(saw_cancelled, "cancelled event never arrived");
}

/// Run a 150-frame render to completion, count topical `progress`
/// deliveries received by this proxy, assert ~20 (18..=25) — proof
/// that should_emit_progress is wired into the live emit path.
#[tokio::test]
#[ignore]
async fn throttle_witness_150_frames_yields_about_20_deliveries() {
    let port = tcp_client::discover_port().expect("find daemon port");
    eprintln!("[p2.throttle] port={}", port);

    let mut client = tcp_client::PocClient::connect(port, "proxy-p2-throttle")
        .await
        .expect("connect");

    let create_payload = serde_json::json!({
        "type": "canvas_video_create_composition",
        "payload": {
            "title": "p2-throttle",
            "width": 640,
            "height": 360,
            "duration_sec": 5.0,
            "fps": 30,
            "bg": "#000",
            "html": fixture_html(),
        }
    });
    let create_rid = client.send_chat(create_payload).await.expect("send create");
    let create_resp = client
        .recv_matching(Duration::from_secs(10), |env| {
            env.payload.get("type").and_then(|v| v.as_str())
                == Some("canvas_video_create_composition_response")
                && env.request_id.as_deref() == Some(&create_rid)
        })
        .await
        .expect("create response");
    let artifact_id = create_resp
        .payload
        .get("payload")
        .and_then(|p| p.get("artifact_id"))
        .and_then(|v| v.as_str())
        .expect("artifact_id")
        .to_owned();

    let sub_payload = serde_json::json!({
        "type": "events_request",
        "payload": {
            "action": "subscribe",
            "patterns": ["jobs:render:*"],
            "replay_sticky": false,
            "buffer_size": 1024,
        }
    });
    let sub_rid = client.send_chat(sub_payload).await.expect("send subscribe");
    let _ = client
        .recv_matching(Duration::from_secs(5), |env| {
            env.payload.get("type").and_then(|v| v.as_str()) == Some("events_response")
                && env.request_id.as_deref() == Some(&sub_rid)
        })
        .await
        .expect("subscribe response");

    let start_payload = serde_json::json!({
        "type": "canvas_video_render_start",
        "payload": { "composition_id": artifact_id }
    });
    let start_rid = client.send_chat(start_payload).await.expect("send start");
    let start_resp = client
        .recv_matching(Duration::from_secs(10), |env| {
            env.payload.get("type").and_then(|v| v.as_str())
                == Some("canvas_video_render_start_response")
                && env.request_id.as_deref() == Some(&start_rid)
        })
        .await
        .expect("render_start response");
    let job_id = start_resp
        .payload
        .get("payload")
        .and_then(|p| p.get("job_id"))
        .and_then(|v| v.as_str())
        .expect("job_id")
        .to_owned();

    let mut progress_count = 0usize;
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let Ok(Some(env)) = timeout(remaining, client.recv()).await else {
            break;
        };
        if env.payload.get("type").and_then(|v| v.as_str()) != Some("events_delivery") {
            continue;
        }
        let Some(event) = env.payload.get("payload").and_then(|p| p.get("event")) else {
            continue;
        };
        let topic = event.get("topic").and_then(|v| v.as_str()).unwrap_or("");
        if topic != format!("jobs:render:{}", job_id) {
            continue;
        }
        let body = event.get("payload").cloned().unwrap_or_default();
        match body.get("event").and_then(|v| v.as_str()) {
            Some("progress") => progress_count += 1,
            Some("succeeded") => break,
            Some("failed") | Some("cancelled") => {
                panic!("unexpected terminal: {:?}", body)
            }
            _ => {}
        }
    }

    eprintln!(
        "[p2.throttle] job_id={} progress deliveries={}",
        job_id, progress_count
    );
    assert!(
        (18..=25).contains(&progress_count),
        "expected 18..=25 progress deliveries, got {}",
        progress_count
    );
}
