/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Link Checker Scenario Example
//!
//! Demonstrates the complete interaction flow from the protocol documentation section 6:
//! User asks "Check current page links" -> Agent streams thinking process ->
//! Agent renders table with dead links -> User clicks export -> Agent exports CSV

use nevoflux_common::protocol::*;
use tokio::sync::mpsc;

/// Simulated link checker result
struct LinkCheckResult {
    url: String,
    status: String,
    reason: String,
}

/// Simulate link checking tool
async fn check_page_links(_url: &str) -> Vec<LinkCheckResult> {
    // Simulate finding 5 dead links
    vec![
        LinkCheckResult {
            url: "https://example.com/old-page".to_string(),
            status: "404".to_string(),
            reason: "Not Found".to_string(),
        },
        LinkCheckResult {
            url: "https://example.com/broken".to_string(),
            status: "500".to_string(),
            reason: "Internal Server Error".to_string(),
        },
        LinkCheckResult {
            url: "https://timeout.com/slow".to_string(),
            status: "Timeout".to_string(),
            reason: "Connection timeout".to_string(),
        },
        LinkCheckResult {
            url: "https://example.com/missing".to_string(),
            status: "404".to_string(),
            reason: "Not Found".to_string(),
        },
        LinkCheckResult {
            url: "https://example.com/forbidden".to_string(),
            status: "403".to_string(),
            reason: "Forbidden".to_string(),
        },
    ]
}

/// Export results to CSV
fn export_to_csv(results: &[LinkCheckResult]) -> String {
    let mut csv = String::from("URL,Status,Reason\n");
    for result in results {
        csv.push_str(&format!(
            "{},{},{}\n",
            result.url, result.status, result.reason
        ));
    }
    csv
}

#[tokio::main]
async fn main() {
    println!("=== Link Checker Scenario ===\n");

    // Simulate message channel
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // Session ID
    let session_id = "sess-001".to_string();

    // =========================================================================
    // Step 1: User sends chat input
    // =========================================================================
    println!("Step 1: User sends 'Check current page links'");

    let chat_input = ChatInput {
        text: "Check current page links".to_string(),
        files: Vec::new(),
        context_ref: Some(ContextRef {
            include_current_tab: true,
            include_selection: false,
        }),
    };

    let envelope = Envelope::new(session_id.clone(), "input.chat".to_string(), chat_input);

    println!(
        "Sent envelope: {}",
        serde_json::to_string_pretty(&envelope).unwrap()
    );
    println!();

    // =========================================================================
    // Step 2: Agent streams thinking process
    // =========================================================================
    println!("Step 2: Agent streams thinking process");

    let stream_id = "stream_resp_001".to_string();

    // Simulate streaming text
    let thinking_steps = vec![
        "Scanning current page for links...",
        "\nFound 25 links total",
        "\nChecking each link for availability...",
        "\nDetected 5 dead links",
    ];

    for delta in thinking_steps {
        let text_stream = TextStream {
            stream_id: stream_id.clone(),
            delta: delta.to_string(),
            format: "markdown".to_string(),
            finish: false,
        };

        let stream_envelope = Envelope::new(
            session_id.clone(),
            "agent.stream.text".to_string(),
            text_stream,
        );

        tx.send(serde_json::to_string(&stream_envelope).unwrap())
            .unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    // Finish text stream
    let finish_stream = TextStream {
        stream_id: stream_id.clone(),
        delta: String::new(),
        format: "markdown".to_string(),
        finish: true,
    };

    let finish_envelope = Envelope::new(
        session_id.clone(),
        "agent.stream.text".to_string(),
        finish_stream,
    );

    tx.send(serde_json::to_string(&finish_envelope).unwrap())
        .unwrap();

    println!("Streamed thinking process");
    println!();

    // =========================================================================
    // Step 3: Agent renders table with dead links
    // =========================================================================
    println!("Step 3: Agent renders table with dead links + export button");

    let results = check_page_links("https://example.com").await;

    // Build table data
    let headers = vec![
        "URL".to_string(),
        "Status".to_string(),
        "Reason".to_string(),
    ];
    let rows: Vec<Vec<String>> = results
        .iter()
        .map(|r| vec![r.url.clone(), r.status.clone(), r.reason.clone()])
        .collect();

    // Build A2UI component tree
    let mut layout = container("flex-col")
        .with_child(text("Found 5 Dead Links", "h3").with_prop("color", "danger"))
        .with_child(table(headers, rows))
        .with_child(
            container("flex-row")
                .with_prop("gap", "sm")
                .with_child(button("Export CSV", "export_csv").with_prop("variant", "primary"))
                .with_child(button("Recheck All", "recheck_links").with_prop("variant", "default")),
        );

    // Assign component IDs
    let mut counter = 0;
    layout.assign_ids("cmp_", &mut counter);

    let ui_render = UiRender {
        route: "sidebar".to_string(),
        view_id: "view_link_check_01".to_string(),
        layout,
    };

    let ui_envelope = Envelope::new(session_id.clone(), "agent.ui.render".to_string(), ui_render);

    println!("Sent UI render:");
    println!("{}", serde_json::to_string_pretty(&ui_envelope).unwrap());
    println!();

    tx.send(serde_json::to_string(&ui_envelope).unwrap())
        .unwrap();

    // =========================================================================
    // Step 4: User clicks "Export CSV" button
    // =========================================================================
    println!("Step 4: User clicks 'Export CSV' button");

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    let ui_event = UiEventInput {
        view_id: "view_link_check_01".to_string(),
        action_id: "export_csv".to_string(),
        form_data: std::collections::HashMap::new(),
    };

    let event_envelope = Envelope::new(session_id.clone(), "input.ui_event".to_string(), ui_event);

    println!(
        "Sent UI event: {}",
        serde_json::to_string_pretty(&event_envelope).unwrap()
    );
    println!();

    // =========================================================================
    // Step 5: Agent processes export and updates UI
    // =========================================================================
    println!("Step 5: Agent exports CSV and updates UI");

    // Simulate CSV export
    let csv_content = export_to_csv(&results);
    println!("Generated CSV:\n{}", csv_content);

    // Update button to show success
    let mut update_props = std::collections::HashMap::new();
    update_props.insert("loading".to_string(), serde_json::json!(false));
    update_props.insert("label".to_string(), serde_json::json!("Exported ✓"));
    update_props.insert("variant".to_string(), serde_json::json!("success"));

    let ui_update = UiUpdate {
        view_id: "view_link_check_01".to_string(),
        target_component_id: "cmp_4".to_string(), // Button component ID
        props: update_props,
    };

    let update_envelope =
        Envelope::new(session_id.clone(), "agent.ui.update".to_string(), ui_update);

    println!("Sent UI update:");
    println!(
        "{}",
        serde_json::to_string_pretty(&update_envelope).unwrap()
    );
    println!();

    tx.send(serde_json::to_string(&update_envelope).unwrap())
        .unwrap();

    // =========================================================================
    // Display all sent messages
    // =========================================================================
    println!("\n=== Messages Sent to Frontend ===");
    drop(tx);

    let mut count = 1;
    while let Some(msg) = rx.recv().await {
        println!("\nMessage {}:", count);
        println!("{}", msg);
        count += 1;
    }

    println!("\n=== Scenario Complete ===");
}
