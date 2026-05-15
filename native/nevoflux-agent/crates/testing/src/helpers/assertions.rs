//! Custom assertion helpers for testing.

use nevoflux_protocol::{DaemonEnvelope, ProxyEnvelope};

/// Assert that two envelopes have the same routing info.
#[track_caller]
pub fn assert_envelope_routing_eq(actual: &ProxyEnvelope, expected: &ProxyEnvelope) {
    assert_eq!(
        actual.proxy_id, expected.proxy_id,
        "proxy_id mismatch: {} != {}",
        actual.proxy_id, expected.proxy_id
    );
    assert_eq!(
        actual.request_id, expected.request_id,
        "request_id mismatch: {} != {}",
        actual.request_id, expected.request_id
    );
    assert_eq!(
        actual.channel, expected.channel,
        "channel mismatch: {:?} != {:?}",
        actual.channel, expected.channel
    );
}

/// Assert that a daemon envelope is a response to a proxy envelope.
#[track_caller]
pub fn assert_is_response_to(response: &DaemonEnvelope, request: &ProxyEnvelope) {
    assert_eq!(
        response.proxy_id, request.proxy_id,
        "Response proxy_id doesn't match request: {} != {}",
        response.proxy_id, request.proxy_id
    );
    assert_eq!(
        response.request_id,
        Some(request.request_id.clone()),
        "Response request_id doesn't match: {:?} != Some({})",
        response.request_id,
        request.request_id
    );
    assert_eq!(
        response.channel, request.channel,
        "Response channel doesn't match: {:?} != {:?}",
        response.channel, request.channel
    );
}

/// Assert that a JSON value contains expected fields.
#[track_caller]
pub fn assert_json_contains(json: &serde_json::Value, path: &str, expected: &serde_json::Value) {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = json;

    for part in &parts {
        current = current.get(*part).unwrap_or_else(|| {
            panic!("JSON path '{}' not found at '{}' in {:?}", path, part, json)
        });
    }

    assert_eq!(
        current, expected,
        "JSON at path '{}' doesn't match:\nActual: {:?}\nExpected: {:?}",
        path, current, expected
    );
}

/// Assert that a result is Ok and return the value.
#[track_caller]
pub fn assert_ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> T {
    match result {
        Ok(v) => v,
        Err(e) => panic!("Expected Ok, got Err({:?})", e),
    }
}

/// Assert that a result is Err.
#[track_caller]
pub fn assert_err<T: std::fmt::Debug, E>(result: Result<T, E>) -> E {
    match result {
        Ok(v) => panic!("Expected Err, got Ok({:?})", v),
        Err(e) => e,
    }
}

/// Assert that an Option is Some and return the value.
#[track_caller]
pub fn assert_some<T>(option: Option<T>) -> T {
    match option {
        Some(v) => v,
        None => panic!("Expected Some, got None"),
    }
}

/// Assert that an Option is None.
#[track_caller]
pub fn assert_none<T: std::fmt::Debug>(option: Option<T>) {
    if let Some(v) = option {
        panic!("Expected None, got Some({:?})", v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::EnvelopeBuilder;
    use nevoflux_protocol::Channel;

    #[test]
    fn test_assert_envelope_routing_eq() {
        let env1 = EnvelopeBuilder::new()
            .with_proxy_id("proxy-1")
            .with_request_id("req-1")
            .with_channel(Channel::Chat)
            .build();

        let env2 = EnvelopeBuilder::new()
            .with_proxy_id("proxy-1")
            .with_request_id("req-1")
            .with_channel(Channel::Chat)
            .with_payload(serde_json::json!({"different": "payload"}))
            .build();

        // Should not panic - routing info is the same
        assert_envelope_routing_eq(&env1, &env2);
    }

    #[test]
    fn test_assert_json_contains() {
        let json = serde_json::json!({
            "user": {
                "name": "Alice",
                "age": 30
            },
            "status": "active"
        });

        assert_json_contains(&json, "status", &serde_json::json!("active"));
        assert_json_contains(&json, "user.name", &serde_json::json!("Alice"));
        assert_json_contains(&json, "user.age", &serde_json::json!(30));
    }

    #[test]
    fn test_assert_ok() {
        let result: Result<i32, &str> = Ok(42);
        let value = assert_ok(result);
        assert_eq!(value, 42);
    }

    #[test]
    fn test_assert_err() {
        let result: Result<i32, &str> = Err("error");
        let error = assert_err(result);
        assert_eq!(error, "error");
    }

    #[test]
    fn test_assert_some() {
        let option = Some(42);
        let value = assert_some(option);
        assert_eq!(value, 42);
    }

    #[test]
    fn test_assert_none() {
        let option: Option<i32> = None;
        assert_none(option);
    }
}
