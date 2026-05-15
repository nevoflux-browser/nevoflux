// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! `Fingerprint` type returned by the Actor `probe` method.
//!
//! The 15-field struct mirrors the JSON shape described in the spec
//! section 5.2 and the Actor code at
//! `src/nevoflux/engine-overlays/browser/actors/NevofluxChild.sys.mjs::probe()`.

use serde::{Deserialize, Serialize};

/// Rich text editor framework detected by ancestor CSS pattern matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EditorFramework {
    #[serde(rename = "draft.js")]
    DraftJs,
    Lexical,
    #[serde(rename = "prosemirror")]
    ProseMirror,
    Slate,
    #[serde(rename = "codemirror")]
    CodeMirror,
    Monaco,
    Quill,
    #[serde(rename = "tinymce")]
    TinyMce,
    /// Catch-all for editors the Actor detected but we don't model.
    /// Triggered by `#[serde(other)]`.
    #[serde(other)]
    Unknown,
}

/// Element fingerprint captured by the Actor `probe` method.
///
/// All fields are strictly mirrored from the JS implementation at
/// `NevofluxChild.sys.mjs::probe()`. The spec table is in section 5.2.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Fingerprint {
    pub tag: String,

    pub input_type: Option<String>,

    pub has_value_property: bool,

    pub is_content_editable: bool,

    pub disabled: bool,

    pub readonly: bool,

    pub is_visible: bool,

    pub is_focusable: bool,

    pub editor_framework: Option<EditorFramework>,

    pub react_fiber_present: bool,

    pub inside_iframe: bool,

    pub shadow_root_depth: u32,

    pub innermost_editable_selector: Option<String>,

    pub computed_role: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserialize_standard_input_fingerprint() {
        let value = json!({
            "tag": "input",
            "input_type": "text",
            "has_value_property": true,
            "is_content_editable": false,
            "disabled": false,
            "readonly": false,
            "is_visible": true,
            "is_focusable": true,
            "editor_framework": null,
            "react_fiber_present": false,
            "inside_iframe": false,
            "shadow_root_depth": 0,
            "innermost_editable_selector": null,
            "computed_role": null
        });

        let fp: Fingerprint = serde_json::from_value(value).expect("deserialize");
        assert_eq!(fp.tag, "input");
        assert_eq!(fp.input_type.as_deref(), Some("text"));
        assert!(fp.has_value_property);
        assert!(!fp.is_content_editable);
        assert_eq!(fp.editor_framework, None);
    }

    #[test]
    fn deserialize_draft_js_fingerprint() {
        let value = json!({
            "tag": "div",
            "input_type": null,
            "has_value_property": false,
            "is_content_editable": true,
            "disabled": false,
            "readonly": false,
            "is_visible": true,
            "is_focusable": true,
            "editor_framework": "draft.js",
            "react_fiber_present": true,
            "inside_iframe": false,
            "shadow_root_depth": 0,
            "innermost_editable_selector": "div.public-DraftEditor-content",
            "computed_role": "textbox"
        });

        let fp: Fingerprint = serde_json::from_value(value).expect("deserialize");
        assert_eq!(fp.tag, "div");
        assert!(fp.is_content_editable);
        assert_eq!(fp.editor_framework, Some(EditorFramework::DraftJs));
        assert!(fp.react_fiber_present);
        assert_eq!(
            fp.innermost_editable_selector.as_deref(),
            Some("div.public-DraftEditor-content")
        );
    }

    #[test]
    fn deserialize_lexical_framework() {
        let value = json!({
            "tag": "div",
            "input_type": null,
            "has_value_property": false,
            "is_content_editable": true,
            "disabled": false,
            "readonly": false,
            "is_visible": true,
            "is_focusable": true,
            "editor_framework": "lexical",
            "react_fiber_present": true,
            "inside_iframe": false,
            "shadow_root_depth": 0,
            "innermost_editable_selector": "#lex",
            "computed_role": null
        });

        let fp: Fingerprint = serde_json::from_value(value).unwrap();
        assert_eq!(fp.editor_framework, Some(EditorFramework::Lexical));
    }

    #[test]
    fn deserialize_prosemirror_framework() {
        let value = json!({
            "tag": "div", "input_type": null, "has_value_property": false,
            "is_content_editable": true, "disabled": false, "readonly": false,
            "is_visible": true, "is_focusable": true,
            "editor_framework": "prosemirror",
            "react_fiber_present": false, "inside_iframe": false,
            "shadow_root_depth": 0, "innermost_editable_selector": "#pm",
            "computed_role": null
        });
        let fp: Fingerprint = serde_json::from_value(value).unwrap();
        assert_eq!(fp.editor_framework, Some(EditorFramework::ProseMirror));
    }

    #[test]
    fn deserialize_slate_framework() {
        let value = json!({
            "tag": "div", "input_type": null, "has_value_property": false,
            "is_content_editable": true, "disabled": false, "readonly": false,
            "is_visible": true, "is_focusable": true,
            "editor_framework": "slate",
            "react_fiber_present": true, "inside_iframe": false,
            "shadow_root_depth": 0, "innermost_editable_selector": "#slate",
            "computed_role": null
        });
        let fp: Fingerprint = serde_json::from_value(value).unwrap();
        assert_eq!(fp.editor_framework, Some(EditorFramework::Slate));
    }

    #[test]
    fn unknown_framework_deserializes_as_unknown_variant() {
        let value = json!({
            "tag": "div", "input_type": null, "has_value_property": false,
            "is_content_editable": true, "disabled": false, "readonly": false,
            "is_visible": true, "is_focusable": true,
            "editor_framework": "some_new_editor",
            "react_fiber_present": false, "inside_iframe": false,
            "shadow_root_depth": 0, "innermost_editable_selector": null,
            "computed_role": null
        });
        let fp: Fingerprint = serde_json::from_value(value).unwrap();
        assert_eq!(fp.editor_framework, Some(EditorFramework::Unknown));
    }

    #[test]
    fn null_framework_is_none() {
        let value = json!({
            "tag": "div", "input_type": null, "has_value_property": false,
            "is_content_editable": true, "disabled": false, "readonly": false,
            "is_visible": true, "is_focusable": true,
            "editor_framework": null,
            "react_fiber_present": false, "inside_iframe": false,
            "shadow_root_depth": 0, "innermost_editable_selector": null,
            "computed_role": null
        });
        let fp: Fingerprint = serde_json::from_value(value).unwrap();
        assert!(fp.editor_framework.is_none());
    }
}
