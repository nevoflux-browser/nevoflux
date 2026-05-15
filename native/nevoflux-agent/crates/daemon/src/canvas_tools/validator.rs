//! Semantic validation for a parsed [`CanvasTool`].
//!
//! Invoked by the `canvas.tool.save` and `canvas.tool.validate` commands
//! after serde has successfully deserialized the TOML. Produces a
//! single structured error per call — the first rule that fails wins.

use crate::canvas_tools::types::{BackendKind, CanvasTool, ParamType};

/// Validation failure. Each variant carries the `code` used on the wire
/// and, when known, the dotted field path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub code: &'static str,
    pub message: String,
    pub field: Option<String>,
}

impl ValidationError {
    fn v(field: impl Into<Option<String>>, message: impl Into<String>) -> Self {
        Self {
            code: "validation",
            message: message.into(),
            field: field.into(),
        }
    }
}

/// Check every rule in §2.4 of the spec. Returns `Ok(())` on success.
pub fn validate(tool: &CanvasTool) -> Result<(), ValidationError> {
    validate_name(&tool.name)?;
    validate_backend(tool)?;
    if matches!(
        tool.args_mode,
        crate::canvas_tools::types::ArgsMode::Template
    ) {
        validate_template_placeholders(tool)?;
    }
    validate_params(&tool.params)?;
    validate_constraints(&tool.constraints)?;
    Ok(())
}

fn validate_name(name: &str) -> Result<(), ValidationError> {
    if name.is_empty() {
        return Err(ValidationError::v(
            Some("name".into()),
            "name must not be empty",
        ));
    }
    if name.starts_with('.') {
        return Err(ValidationError::v(
            Some("name".into()),
            "name must not start with '.'",
        ));
    }
    let first_ok = name
        .chars()
        .next()
        .map(|c| c.is_ascii_alphanumeric())
        .unwrap_or(false);
    let all_ok = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if !first_ok || !all_ok {
        return Err(ValidationError::v(
            Some("name".into()),
            "name must match [A-Za-z0-9][A-Za-z0-9_-]*",
        ));
    }
    Ok(())
}

fn validate_backend(tool: &CanvasTool) -> Result<(), ValidationError> {
    match tool.kind {
        BackendKind::Command => {
            if tool.binary.as_deref().unwrap_or("").is_empty() {
                return Err(ValidationError::v(
                    Some("binary".into()),
                    "kind=command requires a non-empty binary",
                ));
            }
        }
        BackendKind::Internal => {
            if tool.api.as_deref().unwrap_or("").is_empty() {
                return Err(ValidationError::v(
                    Some("api".into()),
                    "kind=internal requires a non-empty api",
                ));
            }
        }
    }
    Ok(())
}

fn validate_template_placeholders(tool: &CanvasTool) -> Result<(), ValidationError> {
    // Collect every `{{token}}` substring found across `args`.
    let mut used = Vec::new();
    for arg in &tool.args {
        let mut rest = arg.as_str();
        while let Some(start) = rest.find("{{") {
            let after = &rest[start + 2..];
            if let Some(end) = after.find("}}") {
                let token = after[..end].trim();
                if !token.is_empty() {
                    used.push(token.to_string());
                }
                rest = &after[end + 2..];
            } else {
                break; // unmatched {{ — ignore; TOML parse already accepted the string
            }
        }
    }
    for token in &used {
        if !tool.params.contains_key(token) {
            return Err(ValidationError::v(
                Some(format!("args[?].{token}")),
                format!("template references {{{{ {token} }}}} but params.{token} is not declared"),
            ));
        }
    }
    Ok(())
}

fn validate_params(
    params: &std::collections::HashMap<String, crate::canvas_tools::types::ParamSpec>,
) -> Result<(), ValidationError> {
    for (key, spec) in params {
        match &spec.param_type {
            ParamType::Enum { values } => {
                if values.is_empty() {
                    return Err(ValidationError::v(
                        Some(format!("params.{key}.values")),
                        "enum must declare at least one value",
                    ));
                }
            }
            ParamType::Text { pattern: Some(p) } => {
                if regex::Regex::new(p).is_err() {
                    return Err(ValidationError::v(
                        Some(format!("params.{key}.pattern")),
                        format!("pattern is not a valid regex: {p}"),
                    ));
                }
            }
            ParamType::Int { min, max } => {
                if let (Some(lo), Some(hi)) = (min, max) {
                    if lo > hi {
                        return Err(ValidationError::v(
                            Some(format!("params.{key}")),
                            format!("int min ({lo}) > max ({hi})"),
                        ));
                    }
                }
            }
            ParamType::Float { min, max } => {
                if let (Some(lo), Some(hi)) = (min, max) {
                    if lo > hi {
                        return Err(ValidationError::v(
                            Some(format!("params.{key}")),
                            format!("float min ({lo}) > max ({hi})"),
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_constraints(
    c: &crate::canvas_tools::types::ExecutionConstraints,
) -> Result<(), ValidationError> {
    if c.timeout_seconds == 0 {
        return Err(ValidationError::v(
            Some("constraints.timeout_seconds".into()),
            "timeout_seconds must be > 0",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests — one happy path + one failing case per rule.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_tools::types::{
        ArgsMode, BackendKind, ExecutionConstraints, ParamSpec, ParamType,
    };
    use std::collections::HashMap;

    fn base_tool() -> CanvasTool {
        CanvasTool {
            name: "demo".into(),
            description: "d".into(),
            kind: BackendKind::Command,
            binary: Some("/bin/demo".into()),
            api: None,
            args_mode: ArgsMode::Template,
            args: vec![],
            allowed_subcommands: vec![],
            params: HashMap::new(),
            constraints: ExecutionConstraints::default(),
            enabled: true,
            source: crate::canvas_tools::types::ToolSource::User,
        }
    }

    #[test]
    fn happy_path() {
        assert!(validate(&base_tool()).is_ok());
    }

    #[test]
    fn rejects_empty_name() {
        let mut t = base_tool();
        t.name = "".into();
        let e = validate(&t).unwrap_err();
        assert_eq!(e.field.as_deref(), Some("name"));
    }

    #[test]
    fn rejects_dotfile_name() {
        let mut t = base_tool();
        t.name = ".evil".into();
        assert!(validate(&t).is_err());
    }

    #[test]
    fn rejects_bad_char_in_name() {
        let mut t = base_tool();
        t.name = "bad name".into();
        assert!(validate(&t).is_err());
    }

    #[test]
    fn command_requires_binary() {
        let mut t = base_tool();
        t.binary = None;
        let e = validate(&t).unwrap_err();
        assert_eq!(e.field.as_deref(), Some("binary"));
    }

    #[test]
    fn internal_requires_api() {
        let mut t = base_tool();
        t.kind = BackendKind::Internal;
        t.binary = None;
        t.api = None;
        let e = validate(&t).unwrap_err();
        assert_eq!(e.field.as_deref(), Some("api"));
    }

    #[test]
    fn template_placeholder_must_be_declared() {
        let mut t = base_tool();
        t.args = vec!["--file".into(), "{{ path }}".into()];
        // params does NOT declare `path`
        let e = validate(&t).unwrap_err();
        assert!(e.field.as_deref().unwrap().contains(".path"));
    }

    #[test]
    fn template_placeholder_ok_when_declared() {
        let mut t = base_tool();
        t.args = vec!["--file".into(), "{{ path }}".into()];
        t.params.insert(
            "path".into(),
            ParamSpec {
                param_type: ParamType::Path {
                    allowed_prefix: None,
                },
                optional: false,
                default: None,
            },
        );
        assert!(validate(&t).is_ok());
    }

    #[test]
    fn rejects_empty_enum() {
        let mut t = base_tool();
        t.params.insert(
            "mode".into(),
            ParamSpec {
                param_type: ParamType::Enum { values: vec![] },
                optional: false,
                default: None,
            },
        );
        assert!(validate(&t).is_err());
    }

    #[test]
    fn rejects_bad_regex() {
        let mut t = base_tool();
        t.params.insert(
            "q".into(),
            ParamSpec {
                param_type: ParamType::Text {
                    pattern: Some("(".into()),
                },
                optional: true,
                default: None,
            },
        );
        assert!(validate(&t).is_err());
    }

    #[test]
    fn rejects_inverted_int_range() {
        let mut t = base_tool();
        t.params.insert(
            "n".into(),
            ParamSpec {
                param_type: ParamType::Int {
                    min: Some(10),
                    max: Some(1),
                },
                optional: true,
                default: None,
            },
        );
        assert!(validate(&t).is_err());
    }

    #[test]
    fn rejects_zero_timeout() {
        let mut t = base_tool();
        t.constraints.timeout_seconds = 0;
        let e = validate(&t).unwrap_err();
        assert_eq!(e.field.as_deref(), Some("constraints.timeout_seconds"));
    }
}
