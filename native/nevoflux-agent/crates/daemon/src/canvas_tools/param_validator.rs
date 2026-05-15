//! Parameter validation for canvas tools.
//!
//! Validates user-supplied parameter values against [`ParamSpec`] definitions,
//! ensuring type correctness and constraint compliance before tool execution.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::canvas_tools::types::{ParamSpec, ParamType};
use crate::error::{DaemonError, Result};

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Validate all supplied parameter values against their specifications.
///
/// Checks:
/// 1. Every required parameter (not optional, no default) is present in `values`.
/// 2. No unknown parameters appear in `values`.
/// 3. Each supplied value passes its type-specific validation.
pub fn validate_params(
    params: &HashMap<String, ParamSpec>,
    values: &HashMap<String, String>,
    session_dir: &Path,
) -> Result<()> {
    // Check for missing required params.
    for (name, spec) in params {
        if !spec.optional && spec.default.is_none() && !values.contains_key(name) {
            return Err(DaemonError::InvalidRequest(format!(
                "missing required parameter: {name}"
            )));
        }
    }

    // Check for unknown params.
    for name in values.keys() {
        if !params.contains_key(name) {
            return Err(DaemonError::InvalidRequest(format!(
                "unknown parameter: {name}"
            )));
        }
    }

    // Validate each supplied value.
    for (name, value) in values {
        if let Some(spec) = params.get(name) {
            validate_single_param(name, value, &spec.param_type, session_dir)?;
        }
    }

    Ok(())
}

/// Validate a single parameter value against its type.
pub fn validate_single_param(
    name: &str,
    value: &str,
    param_type: &ParamType,
    session_dir: &Path,
) -> Result<()> {
    match param_type {
        ParamType::Path { allowed_prefix } => {
            validate_path(name, value, allowed_prefix.as_deref(), session_dir)
        }
        ParamType::Duration => validate_duration(name, value),
        ParamType::Int { min, max } => validate_int(name, value, *min, *max),
        ParamType::Float { min, max } => validate_float(name, value, *min, *max),
        ParamType::Bool => validate_bool(name, value),
        ParamType::Enum { values: allowed } => validate_enum(name, value, allowed),
        ParamType::Text { pattern } => validate_text(name, value, pattern.as_deref()),
        ParamType::Identifier => validate_identifier(name, value),
    }
}

// ---------------------------------------------------------------------------
// Type-specific validators
// ---------------------------------------------------------------------------

/// Validate a path parameter.
///
/// - Rejects path traversal (`..` components).
/// - If `allowed_prefix` is set, expands `$SESSION_DIR` and checks containment.
fn validate_path(
    name: &str,
    value: &str,
    allowed_prefix: Option<&str>,
    session_dir: &Path,
) -> Result<()> {
    let path = Path::new(value);

    // Reject path traversal.
    for component in path.components() {
        if let std::path::Component::ParentDir = component {
            return Err(DaemonError::InvalidRequest(format!(
                "parameter '{name}': path traversal ('..') is not allowed"
            )));
        }
    }

    // Check allowed_prefix constraint.
    if let Some(prefix) = allowed_prefix {
        let expanded = expand_session_dir(prefix, session_dir);
        let abs_value = if path.is_absolute() {
            path.to_path_buf()
        } else {
            // Relative paths are resolved against the expanded prefix for comparison.
            expanded.join(value)
        };

        if !abs_value.starts_with(&expanded) {
            return Err(DaemonError::InvalidRequest(format!(
                "parameter '{name}': path must be within '{}'",
                expanded.display()
            )));
        }
    }

    Ok(())
}

/// Validate a duration parameter.
///
/// Accepts:
/// - A non-negative float (seconds), e.g. `"30"`, `"2.5"`
/// - HH:MM:SS format, e.g. `"01:30:00"`
fn validate_duration(name: &str, value: &str) -> Result<()> {
    match parse_duration_seconds(value) {
        Ok(secs) => {
            if secs < 0.0 {
                return Err(DaemonError::InvalidRequest(format!(
                    "parameter '{name}': duration must be non-negative, got {secs}"
                )));
            }
            Ok(())
        }
        Err(msg) => Err(DaemonError::InvalidRequest(format!(
            "parameter '{name}': {msg}"
        ))),
    }
}

/// Validate an integer parameter with optional min/max bounds.
fn validate_int(name: &str, value: &str, min: Option<i64>, max: Option<i64>) -> Result<()> {
    let n: i64 = value.parse().map_err(|_| {
        DaemonError::InvalidRequest(format!(
            "parameter '{name}': expected integer, got '{value}'"
        ))
    })?;

    if let Some(lo) = min {
        if n < lo {
            return Err(DaemonError::InvalidRequest(format!(
                "parameter '{name}': value {n} is below minimum {lo}"
            )));
        }
    }
    if let Some(hi) = max {
        if n > hi {
            return Err(DaemonError::InvalidRequest(format!(
                "parameter '{name}': value {n} is above maximum {hi}"
            )));
        }
    }

    Ok(())
}

/// Validate a float parameter with optional min/max bounds.
fn validate_float(name: &str, value: &str, min: Option<f64>, max: Option<f64>) -> Result<()> {
    let n: f64 = value.parse().map_err(|_| {
        DaemonError::InvalidRequest(format!("parameter '{name}': expected float, got '{value}'"))
    })?;

    if let Some(lo) = min {
        if n < lo {
            return Err(DaemonError::InvalidRequest(format!(
                "parameter '{name}': value {n} is below minimum {lo}"
            )));
        }
    }
    if let Some(hi) = max {
        if n > hi {
            return Err(DaemonError::InvalidRequest(format!(
                "parameter '{name}': value {n} is above maximum {hi}"
            )));
        }
    }

    Ok(())
}

/// Validate a boolean parameter.
///
/// Accepted values (case-insensitive): `true`, `false`, `1`, `0`, `yes`, `no`.
fn validate_bool(name: &str, value: &str) -> Result<()> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "false" | "1" | "0" | "yes" | "no" => Ok(()),
        _ => Err(DaemonError::InvalidRequest(format!(
            "parameter '{name}': expected boolean (true/false/1/0/yes/no), got '{value}'"
        ))),
    }
}

/// Validate an enum parameter against its allowed values.
fn validate_enum(name: &str, value: &str, allowed: &[String]) -> Result<()> {
    if !allowed.iter().any(|v| v == value) {
        return Err(DaemonError::InvalidRequest(format!(
            "parameter '{name}': value '{value}' is not one of: {}",
            allowed.join(", ")
        )));
    }
    Ok(())
}

/// Validate a text parameter with an optional regex pattern.
fn validate_text(name: &str, value: &str, pattern: Option<&str>) -> Result<()> {
    if let Some(pat) = pattern {
        let re = Regex::new(pat).map_err(|e| {
            DaemonError::InternalError(format!(
                "parameter '{name}': invalid regex pattern '{pat}': {e}"
            ))
        })?;
        if !re.is_match(value) {
            return Err(DaemonError::InvalidRequest(format!(
                "parameter '{name}': value does not match pattern '{pat}'"
            )));
        }
    }
    Ok(())
}

/// Validate an identifier parameter.
///
/// Must match `^[a-zA-Z0-9_-]{1,64}$`.
fn validate_identifier(name: &str, value: &str) -> Result<()> {
    let re = Regex::new(r"^[a-zA-Z0-9_-]{1,64}$").expect("hardcoded regex is valid");
    if !re.is_match(value) {
        return Err(DaemonError::InvalidRequest(format!(
            "parameter '{name}': identifier must match [a-zA-Z0-9_-]{{1,64}}, got '{value}'"
        )));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Expand `$SESSION_DIR` prefix in a string to the given session directory.
pub fn expand_session_dir(s: &str, session_dir: &Path) -> PathBuf {
    if s == "$SESSION_DIR" {
        session_dir.to_path_buf()
    } else if let Some(rest) = s.strip_prefix("$SESSION_DIR/") {
        session_dir.join(rest)
    } else {
        PathBuf::from(s)
    }
}

/// Parse a duration string as seconds.
///
/// Accepts:
/// - A non-negative float, e.g. `"30"`, `"2.5"`
/// - HH:MM:SS format, e.g. `"01:30:00"`
pub fn parse_duration_seconds(value: &str) -> std::result::Result<f64, String> {
    // Try float first.
    if let Ok(secs) = value.parse::<f64>() {
        return Ok(secs);
    }

    // Try HH:MM:SS.
    let parts: Vec<&str> = value.split(':').collect();
    if parts.len() == 3 {
        let h: f64 = parts[0]
            .parse()
            .map_err(|_| format!("invalid duration: cannot parse hours in '{value}'"))?;
        let m: f64 = parts[1]
            .parse()
            .map_err(|_| format!("invalid duration: cannot parse minutes in '{value}'"))?;
        let s: f64 = parts[2]
            .parse()
            .map_err(|_| format!("invalid duration: cannot parse seconds in '{value}'"))?;

        if !(0.0..60.0).contains(&m) || !(0.0..60.0).contains(&s) {
            return Err(format!(
                "invalid duration: minutes/seconds out of range in '{value}'"
            ));
        }

        return Ok(h * 3600.0 + m * 60.0 + s);
    }

    Err(format!(
        "invalid duration: expected number of seconds or HH:MM:SS, got '{value}'"
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn session_dir() -> PathBuf {
        PathBuf::from("/tmp/nevoflux-test-session")
    }

    fn make_params(specs: Vec<(&str, ParamSpec)>) -> HashMap<String, ParamSpec> {
        specs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
    }

    fn make_values(kvs: Vec<(&str, &str)>) -> HashMap<String, String> {
        kvs.into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    // -----------------------------------------------------------------------
    // Top-level validate_params tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_required_param_missing() {
        let params = make_params(vec![(
            "input",
            ParamSpec {
                param_type: ParamType::Text { pattern: None },
                optional: false,
                default: None,
            },
        )]);
        let values = make_values(vec![]);
        let err = validate_params(&params, &values, &session_dir()).unwrap_err();
        assert!(err
            .to_string()
            .contains("missing required parameter: input"));
    }

    #[test]
    fn test_validate_optional_param_missing_ok() {
        let params = make_params(vec![(
            "input",
            ParamSpec {
                param_type: ParamType::Text { pattern: None },
                optional: true,
                default: None,
            },
        )]);
        let values = make_values(vec![]);
        validate_params(&params, &values, &session_dir()).unwrap();
    }

    #[test]
    fn test_validate_required_param_with_default_missing_ok() {
        let params = make_params(vec![(
            "count",
            ParamSpec {
                param_type: ParamType::Int {
                    min: None,
                    max: None,
                },
                optional: false,
                default: Some("10".into()),
            },
        )]);
        let values = make_values(vec![]);
        validate_params(&params, &values, &session_dir()).unwrap();
    }

    #[test]
    fn test_validate_unknown_param_rejected() {
        let params = make_params(vec![(
            "input",
            ParamSpec {
                param_type: ParamType::Text { pattern: None },
                optional: true,
                default: None,
            },
        )]);
        let values = make_values(vec![("unknown_key", "hello")]);
        let err = validate_params(&params, &values, &session_dir()).unwrap_err();
        assert!(err.to_string().contains("unknown parameter: unknown_key"));
    }

    // -----------------------------------------------------------------------
    // Path validator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_path_no_traversal() {
        let err = validate_single_param(
            "file",
            "../etc/passwd",
            &ParamType::Path {
                allowed_prefix: None,
            },
            &session_dir(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("path traversal"));
    }

    #[test]
    fn test_validate_path_ok_without_prefix() {
        validate_single_param(
            "file",
            "output/result.txt",
            &ParamType::Path {
                allowed_prefix: None,
            },
            &session_dir(),
        )
        .unwrap();
    }

    #[test]
    fn test_validate_path_allowed_prefix_ok() {
        let sd = session_dir();
        let value = format!("{}/data/output.txt", sd.display());
        validate_single_param(
            "file",
            &value,
            &ParamType::Path {
                allowed_prefix: Some("$SESSION_DIR".into()),
            },
            &sd,
        )
        .unwrap();
    }

    #[test]
    fn test_validate_path_allowed_prefix_violation() {
        let err = validate_single_param(
            "file",
            "/etc/passwd",
            &ParamType::Path {
                allowed_prefix: Some("$SESSION_DIR".into()),
            },
            &session_dir(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("path must be within"));
    }

    #[test]
    fn test_validate_path_relative_within_prefix() {
        // Relative paths are resolved against the prefix.
        validate_single_param(
            "file",
            "subdir/file.txt",
            &ParamType::Path {
                allowed_prefix: Some("$SESSION_DIR/output".into()),
            },
            &session_dir(),
        )
        .unwrap();
    }

    // -----------------------------------------------------------------------
    // Duration validator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_duration_float() {
        validate_single_param("dur", "30", &ParamType::Duration, &session_dir()).unwrap();
        validate_single_param("dur", "2.5", &ParamType::Duration, &session_dir()).unwrap();
        validate_single_param("dur", "0", &ParamType::Duration, &session_dir()).unwrap();
    }

    #[test]
    fn test_validate_duration_hhmmss() {
        validate_single_param("dur", "01:30:00", &ParamType::Duration, &session_dir()).unwrap();
        validate_single_param("dur", "00:00:30", &ParamType::Duration, &session_dir()).unwrap();
    }

    #[test]
    fn test_validate_duration_invalid() {
        let err = validate_single_param(
            "dur",
            "not-a-duration",
            &ParamType::Duration,
            &session_dir(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("invalid duration"));
    }

    #[test]
    fn test_validate_duration_negative() {
        let err =
            validate_single_param("dur", "-5", &ParamType::Duration, &session_dir()).unwrap_err();
        assert!(err.to_string().contains("non-negative"));
    }

    // -----------------------------------------------------------------------
    // Int validator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_int_ok() {
        validate_single_param(
            "count",
            "42",
            &ParamType::Int {
                min: Some(0),
                max: Some(100),
            },
            &session_dir(),
        )
        .unwrap();
    }

    #[test]
    fn test_validate_int_out_of_range() {
        let err = validate_single_param(
            "count",
            "200",
            &ParamType::Int {
                min: Some(0),
                max: Some(100),
            },
            &session_dir(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("above maximum"));

        let err = validate_single_param(
            "count",
            "-5",
            &ParamType::Int {
                min: Some(0),
                max: Some(100),
            },
            &session_dir(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("below minimum"));
    }

    #[test]
    fn test_validate_int_invalid() {
        let err = validate_single_param(
            "count",
            "abc",
            &ParamType::Int {
                min: None,
                max: None,
            },
            &session_dir(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("expected integer"));
    }

    #[test]
    fn test_validate_int_no_bounds() {
        validate_single_param(
            "count",
            "-999999",
            &ParamType::Int {
                min: None,
                max: None,
            },
            &session_dir(),
        )
        .unwrap();
    }

    // -----------------------------------------------------------------------
    // Float validator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_float_ok() {
        validate_single_param(
            "ratio",
            "0.5",
            &ParamType::Float {
                min: Some(0.0),
                max: Some(1.0),
            },
            &session_dir(),
        )
        .unwrap();
    }

    #[test]
    fn test_validate_float_out_of_range() {
        let err = validate_single_param(
            "ratio",
            "1.5",
            &ParamType::Float {
                min: Some(0.0),
                max: Some(1.0),
            },
            &session_dir(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("above maximum"));

        let err = validate_single_param(
            "ratio",
            "-0.1",
            &ParamType::Float {
                min: Some(0.0),
                max: Some(1.0),
            },
            &session_dir(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("below minimum"));
    }

    #[test]
    fn test_validate_float_invalid() {
        let err = validate_single_param(
            "ratio",
            "not-a-number",
            &ParamType::Float {
                min: None,
                max: None,
            },
            &session_dir(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("expected float"));
    }

    // -----------------------------------------------------------------------
    // Bool validator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_bool_variants() {
        for val in &[
            "true", "false", "1", "0", "yes", "no", "True", "FALSE", "Yes", "NO",
        ] {
            validate_single_param("flag", val, &ParamType::Bool, &session_dir()).unwrap();
        }
    }

    #[test]
    fn test_validate_bool_invalid() {
        let err =
            validate_single_param("flag", "maybe", &ParamType::Bool, &session_dir()).unwrap_err();
        assert!(err.to_string().contains("expected boolean"));
    }

    // -----------------------------------------------------------------------
    // Enum validator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_enum_ok() {
        validate_single_param(
            "format",
            "json",
            &ParamType::Enum {
                values: vec!["json".into(), "csv".into(), "xml".into()],
            },
            &session_dir(),
        )
        .unwrap();
    }

    #[test]
    fn test_validate_enum_invalid() {
        let err = validate_single_param(
            "format",
            "yaml",
            &ParamType::Enum {
                values: vec!["json".into(), "csv".into(), "xml".into()],
            },
            &session_dir(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("not one of"));
        assert!(err.to_string().contains("json, csv, xml"));
    }

    // -----------------------------------------------------------------------
    // Text validator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_text_no_pattern() {
        validate_single_param(
            "msg",
            "anything goes here! 123 @#$",
            &ParamType::Text { pattern: None },
            &session_dir(),
        )
        .unwrap();
    }

    #[test]
    fn test_validate_text_pattern_match() {
        validate_single_param(
            "msg",
            "hello_world",
            &ParamType::Text {
                pattern: Some(r"^\w+$".into()),
            },
            &session_dir(),
        )
        .unwrap();
    }

    #[test]
    fn test_validate_text_pattern_mismatch() {
        let err = validate_single_param(
            "msg",
            "hello world!!",
            &ParamType::Text {
                pattern: Some(r"^\w+$".into()),
            },
            &session_dir(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("does not match pattern"));
    }

    // -----------------------------------------------------------------------
    // Identifier validator tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_identifier_ok() {
        for val in &["hello", "my-tool", "tool_123", "A", "a-b-c_d"] {
            validate_single_param("id", val, &ParamType::Identifier, &session_dir()).unwrap();
        }
    }

    #[test]
    fn test_validate_identifier_invalid() {
        // Spaces not allowed.
        let err = validate_single_param("id", "has space", &ParamType::Identifier, &session_dir())
            .unwrap_err();
        assert!(err.to_string().contains("identifier must match"));

        // Empty string not allowed.
        let err =
            validate_single_param("id", "", &ParamType::Identifier, &session_dir()).unwrap_err();
        assert!(err.to_string().contains("identifier must match"));

        // Too long (>64 chars).
        let long = "a".repeat(65);
        let err =
            validate_single_param("id", &long, &ParamType::Identifier, &session_dir()).unwrap_err();
        assert!(err.to_string().contains("identifier must match"));
    }

    // -----------------------------------------------------------------------
    // Helper tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_expand_session_dir() {
        let sd = PathBuf::from("/tmp/session-123");
        assert_eq!(expand_session_dir("$SESSION_DIR", &sd), sd);
        assert_eq!(
            expand_session_dir("$SESSION_DIR/output", &sd),
            sd.join("output")
        );
        assert_eq!(
            expand_session_dir("/some/other/path", &sd),
            PathBuf::from("/some/other/path")
        );
    }

    #[test]
    fn test_parse_duration_seconds_float() {
        assert!((parse_duration_seconds("30").unwrap() - 30.0).abs() < f64::EPSILON);
        assert!((parse_duration_seconds("2.5").unwrap() - 2.5).abs() < f64::EPSILON);
        assert!((parse_duration_seconds("0").unwrap() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_duration_seconds_hhmmss() {
        let secs = parse_duration_seconds("01:30:00").unwrap();
        assert!((secs - 5400.0).abs() < f64::EPSILON);

        let secs = parse_duration_seconds("00:01:30").unwrap();
        assert!((secs - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_parse_duration_seconds_invalid() {
        assert!(parse_duration_seconds("abc").is_err());
        assert!(parse_duration_seconds("1:2").is_err());
    }

    // -----------------------------------------------------------------------
    // Integration: all params happy path
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_all_params_happy_path() {
        let sd = session_dir();
        let output_path = format!("{}/output.png", sd.display());

        let params = make_params(vec![
            (
                "input",
                ParamSpec {
                    param_type: ParamType::Path {
                        allowed_prefix: Some("$SESSION_DIR".into()),
                    },
                    optional: false,
                    default: None,
                },
            ),
            (
                "duration",
                ParamSpec {
                    param_type: ParamType::Duration,
                    optional: false,
                    default: None,
                },
            ),
            (
                "count",
                ParamSpec {
                    param_type: ParamType::Int {
                        min: Some(1),
                        max: Some(100),
                    },
                    optional: false,
                    default: None,
                },
            ),
            (
                "ratio",
                ParamSpec {
                    param_type: ParamType::Float {
                        min: Some(0.0),
                        max: Some(1.0),
                    },
                    optional: false,
                    default: None,
                },
            ),
            (
                "verbose",
                ParamSpec {
                    param_type: ParamType::Bool,
                    optional: true,
                    default: None,
                },
            ),
            (
                "format",
                ParamSpec {
                    param_type: ParamType::Enum {
                        values: vec!["json".into(), "csv".into()],
                    },
                    optional: false,
                    default: None,
                },
            ),
            (
                "label",
                ParamSpec {
                    param_type: ParamType::Identifier,
                    optional: false,
                    default: None,
                },
            ),
            (
                "note",
                ParamSpec {
                    param_type: ParamType::Text { pattern: None },
                    optional: true,
                    default: Some("default note".into()),
                },
            ),
        ]);

        let values = make_values(vec![
            ("input", &output_path),
            ("duration", "01:00:00"),
            ("count", "50"),
            ("ratio", "0.75"),
            ("verbose", "yes"),
            ("format", "json"),
            ("label", "my-task_01"),
        ]);

        validate_params(&params, &values, &sd).unwrap();
    }
}
