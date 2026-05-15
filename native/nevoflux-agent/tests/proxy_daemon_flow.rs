//! Integration tests for Proxy <-> Daemon communication.
//!
//! These tests verify the complete flow of messages between a proxy client
//! (simulating the browser extension or MCP bridge) and the daemon server.
//!
//! Note: These tests use `#[serial]` to avoid port conflicts when running
//! multiple tests concurrently.

use nevoflux_bridge::{generate_proxy_id, BridgeConfig, DaemonClient};
use nevoflux_daemon::{start_server, Router, ServerConfig, SessionManager};
use nevoflux_protocol::Channel;
use serial_test::serial;
use std::sync::Arc;
use std::time::Duration;

/// Helper to create a test session manager with a temp database.
fn create_test_session_manager() -> Arc<SessionManager> {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("nevoflux_test_{}.db", std::process::id()));
    Arc::new(
        SessionManager::new(db_path.to_str().unwrap()).expect("Failed to create session manager"),
    )
}

/// Test that a proxy can successfully connect to the daemon.
#[tokio::test]
#[serial]
async fn test_proxy_connects_to_daemon() {
    // Start daemon server
    let config = ServerConfig::default();
    let router = Arc::new(Router::new());
    let session_manager = create_test_session_manager();
    let server = start_server(config, router, session_manager).await.unwrap();
    let port = server.port();

    // Give server time to bind and start receiving
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client using connect_to (direct address) instead of connect (port file discovery)
    let bridge_config = BridgeConfig::new();
    let mut client = DaemonClient::new(generate_proxy_id(), bridge_config);

    let addr = format!("127.0.0.1:{}", port);
    let connect_result = client.connect_to(&addr).await;
    assert!(
        connect_result.is_ok(),
        "Failed to connect: {:?}",
        connect_result
    );
}

/// Test that a proxy can send a chat message to the daemon.
#[tokio::test]
#[serial]
async fn test_proxy_sends_chat_message_to_daemon() {
    // Start daemon server
    let config = ServerConfig::default();
    let router = Arc::new(Router::new());
    let session_manager = create_test_session_manager();
    let server = start_server(config, router.clone(), session_manager)
        .await
        .unwrap();
    let port = server.port();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let bridge_config = BridgeConfig::new();
    let proxy_id = generate_proxy_id();
    let mut client = DaemonClient::new(&proxy_id, bridge_config);

    let addr = format!("127.0.0.1:{}", port);
    client.connect_to(&addr).await.unwrap();

    // Register proxy in the router (normally done by the daemon on handshake)
    router
        .proxy_registry()
        .register(&proxy_id, std::process::id());

    // Send a chat message
    let payload = serde_json::json!({
        "type": "chat_message",
        "payload": {
            "session_id": "sess-001",
            "message_id": "msg-001",
            "text": "Hello, daemon!"
        }
    });

    let result = client.send_chat("req-001", payload).await;
    assert!(result.is_ok(), "Failed to send chat: {:?}", result);
}

/// Test that a proxy can send an MCP message to the daemon.
#[tokio::test]
#[serial]
async fn test_proxy_sends_mcp_message_to_daemon() {
    // Start daemon server
    let config = ServerConfig::default();
    let router = Arc::new(Router::new());
    let session_manager = create_test_session_manager();
    let server = start_server(config, router.clone(), session_manager)
        .await
        .unwrap();
    let port = server.port();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let bridge_config = BridgeConfig::new();
    let proxy_id = generate_proxy_id();
    let mut client = DaemonClient::new(&proxy_id, bridge_config);

    let addr = format!("127.0.0.1:{}", port);
    client.connect_to(&addr).await.unwrap();

    // Register proxy
    router
        .proxy_registry()
        .register(&proxy_id, std::process::id());

    // Send an MCP message (JSON-RPC request)
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });

    let result = client.send_mcp("req-002", payload).await;
    assert!(result.is_ok(), "Failed to send MCP: {:?}", result);
}

/// Test that multiple proxies can connect to the same daemon.
#[tokio::test]
#[serial]
async fn test_multiple_proxies_connect() {
    // Start daemon server
    let config = ServerConfig::default();
    let router = Arc::new(Router::new());
    let session_manager = create_test_session_manager();
    let server = start_server(config, router.clone(), session_manager)
        .await
        .unwrap();
    let port = server.port();

    tokio::time::sleep(Duration::from_millis(100)).await;

    let addr = format!("127.0.0.1:{}", port);

    // Connect multiple clients
    let proxy_id_1 = generate_proxy_id();
    let mut client_1 = DaemonClient::new(&proxy_id_1, BridgeConfig::new());
    client_1.connect_to(&addr).await.unwrap();
    router.proxy_registry().register(&proxy_id_1, 1001);

    let proxy_id_2 = generate_proxy_id();
    let mut client_2 = DaemonClient::new(&proxy_id_2, BridgeConfig::new());
    client_2.connect_to(&addr).await.unwrap();
    router.proxy_registry().register(&proxy_id_2, 1002);

    let proxy_id_3 = generate_proxy_id();
    let mut client_3 = DaemonClient::new(&proxy_id_3, BridgeConfig::new());
    client_3.connect_to(&addr).await.unwrap();
    router.proxy_registry().register(&proxy_id_3, 1003);

    // Verify all are registered
    assert!(router.proxy_registry().is_registered(&proxy_id_1));
    assert!(router.proxy_registry().is_registered(&proxy_id_2));
    assert!(router.proxy_registry().is_registered(&proxy_id_3));
    assert_eq!(router.proxy_registry().active_count(), 3);
}

/// Test router correctly tracks requests from proxies.
#[tokio::test]
#[serial]
async fn test_router_tracks_proxy_requests() {
    // Start daemon server
    let config = ServerConfig::default();
    let router = Arc::new(Router::new());
    let session_manager = create_test_session_manager();
    let server = start_server(config, router.clone(), session_manager)
        .await
        .unwrap();
    let port = server.port();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let bridge_config = BridgeConfig::new();
    let proxy_id = generate_proxy_id();
    let mut client = DaemonClient::new(&proxy_id, bridge_config);

    let addr = format!("127.0.0.1:{}", port);
    client.connect_to(&addr).await.unwrap();

    // Register proxy and request
    router
        .proxy_registry()
        .register(&proxy_id, std::process::id());
    router.register_request("req-001", &proxy_id, "session-001");

    // Verify routing
    let found_proxy = router.find_proxy_for_request("req-001");
    assert_eq!(found_proxy, Some(proxy_id.clone()));

    let found_for_session = router.find_proxy_for_session("session-001");
    assert_eq!(found_for_session, Some(proxy_id.clone()));
}

/// Test that port file discovery works for daemon connection.
#[tokio::test]
#[serial]
async fn test_port_file_discovery() {
    // Start daemon server
    let config = ServerConfig::default();
    let router = Arc::new(Router::new());
    let session_manager = create_test_session_manager();
    let server = start_server(config, router, session_manager).await.unwrap();
    let port = server.port();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create temp directory with port/pid files (use managed names for Prod mode)
    let temp = tempfile::TempDir::new().unwrap();
    std::fs::write(temp.path().join("daemon-managed.port"), port.to_string()).unwrap();
    std::fs::write(
        temp.path().join("daemon-managed.pid"),
        std::process::id().to_string(),
    )
    .unwrap();

    // Connect using port discovery
    let bridge_config = BridgeConfig::new().with_data_dir(temp.path());
    let mut client = DaemonClient::new(generate_proxy_id(), bridge_config);

    let connect_result = client.connect().await;
    assert!(
        connect_result.is_ok(),
        "Failed to connect via port discovery: {:?}",
        connect_result
    );

    // Verify daemon info was set
    assert!(client.daemon_info().is_some());
    assert_eq!(client.daemon_info().unwrap().port, port);
}

/// Test server shutdown.
#[tokio::test]
#[serial]
async fn test_server_shutdown() {
    let config = ServerConfig::default();
    let router = Arc::new(Router::new());
    let session_manager = create_test_session_manager();
    let mut server = start_server(config, router, session_manager).await.unwrap();

    let port = server.port();
    assert!((19500..=19600).contains(&port));

    // Shutdown should complete without error
    server.shutdown().await;
}

/// Test that a connected proxy can close its connection.
#[tokio::test]
#[serial]
async fn test_proxy_close_connection() {
    // Start daemon server
    let config = ServerConfig::default();
    let router = Arc::new(Router::new());
    let session_manager = create_test_session_manager();
    let server = start_server(config, router.clone(), session_manager)
        .await
        .unwrap();
    let port = server.port();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let proxy_id = generate_proxy_id();
    let mut client = DaemonClient::new(&proxy_id, BridgeConfig::new());

    let addr = format!("127.0.0.1:{}", port);
    client.connect_to(&addr).await.unwrap();
    router
        .proxy_registry()
        .register(&proxy_id, std::process::id());

    // Close connection
    let close_result = client.close().await;
    assert!(close_result.is_ok());

    // Simulate disconnect cleanup
    router.handle_proxy_disconnect(&proxy_id);
    assert!(!router.proxy_registry().is_registered(&proxy_id));
}

/// Test sending messages with different channel types.
#[tokio::test]
#[serial]
async fn test_channel_types() {
    use nevoflux_protocol::ProxyEnvelope;

    // Start daemon server
    let config = ServerConfig::default();
    let router = Arc::new(Router::new());
    let session_manager = create_test_session_manager();
    let server = start_server(config, router.clone(), session_manager)
        .await
        .unwrap();
    let port = server.port();

    tokio::time::sleep(Duration::from_millis(100)).await;

    // Connect client
    let proxy_id = generate_proxy_id();
    let mut client = DaemonClient::new(&proxy_id, BridgeConfig::new());

    let addr = format!("127.0.0.1:{}", port);
    client.connect_to(&addr).await.unwrap();
    router
        .proxy_registry()
        .register(&proxy_id, std::process::id());

    // Test Chat channel envelope
    let chat_envelope = ProxyEnvelope::new(
        &proxy_id,
        "req-chat",
        Channel::Chat,
        serde_json::json!({"type": "chat_message"}),
    );
    assert_eq!(chat_envelope.channel, Channel::Chat);

    // Test MCP channel envelope
    let mcp_envelope = ProxyEnvelope::new(
        &proxy_id,
        "req-mcp",
        Channel::Mcp,
        serde_json::json!({"jsonrpc": "2.0"}),
    );
    assert_eq!(mcp_envelope.channel, Channel::Mcp);

    // Both can be sent
    let result1 = client.send(chat_envelope).await;
    assert!(result1.is_ok());

    let result2 = client.send(mcp_envelope).await;
    assert!(result2.is_ok());
}
