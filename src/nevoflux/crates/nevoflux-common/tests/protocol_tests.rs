/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Protocol tests

use nevoflux_common::protocol::*;
use serde_json;
use std::collections::HashMap;

#[test]
fn test_envelope_creation() {
    let payload = ChatInput {
        text: "Hello".to_string(),
        files: Vec::new(),
        context_ref: None,
    };

    let envelope = Envelope::new("session-123".to_string(), "input.chat".to_string(), payload);

    assert_eq!(envelope.ver, "2.0");
    assert_eq!(envelope.session_id, "session-123");
    assert_eq!(envelope.msg_type, "input.chat");
    assert!(envelope.timestamp > 0);
    assert!(!envelope.msg_id.is_empty());
}

#[test]
fn test_envelope_serialization() {
    let payload = ChatInput {
        text: "Test message".to_string(),
        files: Vec::new(),
        context_ref: None,
    };

    let envelope = Envelope::new("session-1".to_string(), "input.chat".to_string(), payload);

    let json = serde_json::to_string(&envelope).unwrap();
    assert!(json.contains("\"ver\":\"2.0\""));
    assert!(json.contains("\"session_id\":\"session-1\""));
    assert!(json.contains("\"type\":\"input.chat\""));
}

#[test]
fn test_chat_input_with_files() {
    let files = vec![FileAttachment {
        name: "test.png".to_string(),
        mime: "image/png".to_string(),
        data: "base64data".to_string(),
    }];

    let chat = ChatInput {
        text: "Look at this".to_string(),
        files,
        context_ref: Some(ContextRef {
            include_current_tab: true,
            include_selection: false,
        }),
    };

    assert_eq!(chat.files.len(), 1);
    assert_eq!(chat.files[0].name, "test.png");
    assert!(chat.context_ref.is_some());
}

#[test]
fn test_ui_event_input() {
    let mut form_data = HashMap::new();
    form_data.insert("amount".to_string(), serde_json::json!(100));
    form_data.insert("currency".to_string(), serde_json::json!("USD"));

    let event = UiEventInput {
        view_id: "view_01".to_string(),
        action_id: "confirm_payment".to_string(),
        form_data,
    };

    assert_eq!(event.view_id, "view_01");
    assert_eq!(event.action_id, "confirm_payment");
    assert_eq!(event.form_data.len(), 2);
}

#[test]
fn test_text_stream() {
    let stream = TextStream {
        stream_id: "stream_1".to_string(),
        delta: "Hello world".to_string(),
        format: "markdown".to_string(),
        finish: false,
    };

    assert_eq!(stream.delta, "Hello world");
    assert!(!stream.finish);
}

#[test]
fn test_a2ui_component_builder() {
    let component = container("flex-col")
        .with_id("container-1")
        .with_prop("gap", "sm")
        .with_child(text("Hello World", "h1").with_prop("color", "primary"))
        .with_child(button("Click Me", "btn_action"));

    assert_eq!(component.component, "Container");
    assert_eq!(component.id, Some("container-1".to_string()));
    assert_eq!(component.children.len(), 2);
    assert_eq!(component.children[0].component, "Text");
    assert_eq!(component.children[1].component, "Button");
}

#[test]
fn test_component_id_assignment() {
    let mut component = container("flex-col")
        .with_child(text("Text 1", "p"))
        .with_child(text("Text 2", "p"))
        .with_child(
            container("flex-row")
                .with_child(button("Btn 1", "action1"))
                .with_child(button("Btn 2", "action2")),
        );

    let mut counter = 0;
    component.assign_ids("cmp_", &mut counter);

    assert_eq!(component.id, Some("cmp_1".to_string()));
    assert_eq!(component.children[0].id, Some("cmp_2".to_string()));
    assert_eq!(component.children[1].id, Some("cmp_3".to_string()));
    assert_eq!(component.children[2].id, Some("cmp_4".to_string()));
    assert_eq!(
        component.children[2].children[0].id,
        Some("cmp_5".to_string())
    );
    assert_eq!(
        component.children[2].children[1].id,
        Some("cmp_6".to_string())
    );
}

#[test]
fn test_table_component() {
    let headers = vec!["Name".to_string(), "Age".to_string()];
    let rows = vec![
        vec!["Alice".to_string(), "30".to_string()],
        vec!["Bob".to_string(), "25".to_string()],
    ];

    let table_comp = table(headers, rows);

    assert_eq!(table_comp.component, "Table");
    assert!(table_comp.props.contains_key("headers"));
    assert!(table_comp.props.contains_key("rows"));
}

#[test]
fn test_ui_render_serialization() {
    let layout = container("flex-col").with_child(text("Test", "h1"));

    let ui_render = UiRender {
        route: "sidebar".to_string(),
        view_id: "view_1".to_string(),
        layout,
    };

    let json = serde_json::to_string(&ui_render).unwrap();
    assert!(json.contains("\"route\":\"sidebar\""));
    assert!(json.contains("\"view_id\":\"view_1\""));
    assert!(json.contains("\"component\":\"Container\""));
}

#[test]
fn test_ui_update() {
    let mut props = HashMap::new();
    props.insert("loading".to_string(), serde_json::json!(true));
    props.insert("label".to_string(), serde_json::json!("Processing..."));

    let ui_update = UiUpdate {
        view_id: "view_1".to_string(),
        target_component_id: "btn_1".to_string(),
        props,
    };

    assert_eq!(ui_update.view_id, "view_1");
    assert_eq!(ui_update.target_component_id, "btn_1");
    assert_eq!(ui_update.props.len(), 2);
}

#[test]
fn test_browser_control() {
    let control = BrowserControl {
        tab_id: 123,
        action: "navigate".to_string(),
        selector: None,
        value: Some("https://example.com".to_string()),
    };

    assert_eq!(control.tab_id, 123);
    assert_eq!(control.action, "navigate");
    assert!(control.value.is_some());
}

#[test]
fn test_complete_envelope_deserialization() {
    let json = r#"{
        "ver": "2.0",
        "msg_id": "msg-123",
        "session_id": "session-1",
        "type": "input.chat",
        "payload": {
            "text": "Hello",
            "files": []
        },
        "timestamp": 1234567890
    }"#;

    let envelope: Envelope<ChatInput> = serde_json::from_str(json).unwrap();

    assert_eq!(envelope.ver, "2.0");
    assert_eq!(envelope.msg_id, "msg-123");
    assert_eq!(envelope.session_id, "session-1");
    assert_eq!(envelope.msg_type, "input.chat");
    assert_eq!(envelope.payload.text, "Hello");
}

#[test]
fn test_nested_component_serialization() {
    let component = container("flex-col")
        .with_child(
            container("flex-row")
                .with_child(text("Label:", "p"))
                .with_child(input("username", "text")),
        )
        .with_child(button("Submit", "submit_action"));

    let json = serde_json::to_string_pretty(&component).unwrap();

    // Verify structure
    let parsed: A2UiComponent = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.component, "Container");
    assert_eq!(parsed.children.len(), 2);
    assert_eq!(parsed.children[0].children.len(), 2);
}
