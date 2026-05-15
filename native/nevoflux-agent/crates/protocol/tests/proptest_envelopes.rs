// crates/protocol/tests/proptest_envelopes.rs

use nevoflux_protocol::{Channel, DaemonEnvelope, ProxyEnvelope};
use proptest::prelude::*;

fn arb_channel() -> impl Strategy<Value = Channel> {
    prop_oneof![Just(Channel::Chat), Just(Channel::Mcp),]
}

fn arb_proxy_envelope() -> impl Strategy<Value = ProxyEnvelope> {
    (
        "[a-z0-9]{8}",   // proxy_id
        "[a-z0-9-]{36}", // request_id
        arb_channel(),
        any::<u64>(), // timestamp_ms
    )
        .prop_map(
            |(proxy_id, request_id, channel, timestamp_ms)| ProxyEnvelope {
                proxy_id,
                request_id,
                auth: None,
                channel,
                payload: serde_json::json!({"type": "test"}),
                timestamp_ms,
            },
        )
}

fn arb_daemon_envelope() -> impl Strategy<Value = DaemonEnvelope> {
    (
        "[a-z0-9]{8}",                         // proxy_id
        proptest::option::of("[a-z0-9-]{36}"), // request_id
        arb_channel(),
        any::<u64>(), // timestamp_ms
    )
        .prop_map(
            |(proxy_id, request_id, channel, timestamp_ms)| DaemonEnvelope {
                proxy_id,
                request_id,
                channel,
                payload: serde_json::json!({"type": "test"}),
                timestamp_ms,
            },
        )
}

proptest! {
    #[test]
    fn proxy_envelope_json_roundtrip(envelope in arb_proxy_envelope()) {
        let json = serde_json::to_string(&envelope).unwrap();
        let decoded: ProxyEnvelope = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(envelope, decoded);
    }

    #[test]
    fn proxy_envelope_messagepack_roundtrip(envelope in arb_proxy_envelope()) {
        let encoded = rmp_serde::to_vec(&envelope).unwrap();
        let decoded: ProxyEnvelope = rmp_serde::from_slice(&encoded).unwrap();
        prop_assert_eq!(envelope, decoded);
    }

    #[test]
    fn daemon_envelope_json_roundtrip(envelope in arb_daemon_envelope()) {
        let json = serde_json::to_string(&envelope).unwrap();
        let decoded: DaemonEnvelope = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(envelope, decoded);
    }

    #[test]
    fn daemon_envelope_messagepack_roundtrip(envelope in arb_daemon_envelope()) {
        let encoded = rmp_serde::to_vec(&envelope).unwrap();
        let decoded: DaemonEnvelope = rmp_serde::from_slice(&encoded).unwrap();
        prop_assert_eq!(envelope, decoded);
    }

    #[test]
    fn malformed_json_no_panic(data in ".*") {
        let _ = serde_json::from_str::<ProxyEnvelope>(&data);
        // No panic means success
    }

    #[test]
    fn malformed_messagepack_no_panic(data in prop::collection::vec(any::<u8>(), 0..1000)) {
        let _ = rmp_serde::from_slice::<ProxyEnvelope>(&data);
        // No panic means success
    }
}
