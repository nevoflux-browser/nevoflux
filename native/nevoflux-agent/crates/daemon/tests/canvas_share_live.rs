//! Live integration test for Canvas Share against the deployed CF Worker.
//!
//! Run with:
//!
//! ```bash
//! cargo test -p nevoflux-daemon --test canvas_share_live -- --ignored --nocapture
//! ```
//!
//! Default target: `https://share.nevoflux.app`. Override with the
//! `CANVAS_SHARE_URL` environment variable.

use std::time::Duration;

use nevoflux_daemon::share::{
    binary_format,
    crypto::{decrypt_share_bundle, encrypt_share_bundle},
    http_client::ShareHttpClient,
    owner_token::{generate_owner_token, hash_owner_token},
    password::generate_password,
    share_id::generate_share_id,
    types::{ShareBundle, ShareMetadata},
};

fn test_url() -> String {
    std::env::var("CANVAS_SHARE_URL").unwrap_or_else(|_| "https://share.nevoflux.app".to_string())
}

fn b64url_encode(data: &[u8]) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(data)
}

fn sample_bundle() -> ShareBundle {
    ShareBundle {
        artifact_id: "test-art-123".into(),
        artifact_name: "Integration Test Canvas".into(),
        artifact_type: "text/plain".into(),
        content: serde_json::Value::String("hello from integration test".into()),
        metadata: ShareMetadata {
            created_at: chrono::Utc::now().to_rfc3339(),
            version: "1.0".into(),
            author: Some("integration-test".into()),
        },
    }
}

#[tokio::test]
#[ignore = "requires live CF Worker"]
async fn full_share_flow_e2e() {
    let client = ShareHttpClient::new(test_url()).unwrap();

    // 1. Generate credentials + bundle.
    let share_id = generate_share_id();
    let password = generate_password();
    let owner_token = generate_owner_token();
    let owner_token_hash = hash_owner_token(&share_id, &owner_token);
    let owner_token_b64 = b64url_encode(&owner_token);

    eprintln!("share_id: {}", share_id);
    eprintln!("password: {}", password);

    let bundle = sample_bundle();

    // 2. Encrypt + serialize.
    let encrypted = encrypt_share_bundle(&bundle, &password, &share_id).unwrap();
    let nfeb_bytes = binary_format::serialize(&encrypted).unwrap();
    eprintln!("NFEB size: {} bytes", nfeb_bytes.len());
    assert_eq!(&nfeb_bytes[..4], b"NFEB", "magic bytes");

    // 3. Upload.
    let upload_resp = client
        .upload(&share_id, &nfeb_bytes, &owner_token_hash, 600)
        .await
        .expect("upload should succeed");
    assert_eq!(upload_resp.share_id, share_id);
    assert_eq!(upload_resp.size_bytes as usize, nfeb_bytes.len());
    eprintln!("uploaded, expires_at: {}", upload_resp.expires_at);

    // 4. Fetch meta.
    let meta = client
        .fetch_meta(&share_id)
        .await
        .expect("meta should succeed");
    assert_eq!(meta.share_id, share_id);
    assert_eq!(meta.size_bytes as usize, nfeb_bytes.len());
    assert_eq!(meta.view_count, 0);

    // 5. Fetch bundle + decrypt.
    let fetched = client
        .fetch_bundle(&share_id)
        .await
        .expect("fetch should succeed");
    assert_eq!(fetched, nfeb_bytes, "bundle bytes should match exactly");

    let decrypted_encrypted = binary_format::deserialize(&fetched).unwrap();
    let decrypted_bundle = decrypt_share_bundle(&decrypted_encrypted, &password).unwrap();
    assert_eq!(decrypted_bundle.artifact_id, bundle.artifact_id);
    assert_eq!(decrypted_bundle.artifact_name, bundle.artifact_name);

    // 6. Verify view_count incremented. Give the Worker a moment to update KV
    //    asynchronously.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let meta2 = client
        .fetch_meta(&share_id)
        .await
        .expect("meta should succeed");
    assert!(
        meta2.view_count >= 1,
        "view_count should have incremented, got {}",
        meta2.view_count
    );

    // 7. Wrong password should fail decryption.
    let bad_bundle_result = decrypt_share_bundle(&decrypted_encrypted, "wrong-password");
    assert!(
        bad_bundle_result.is_err(),
        "wrong password should fail decryption"
    );

    // 8. Extend TTL.
    let original_expires = upload_resp.expires_at.clone();
    let extend_resp = client
        .extend(&share_id, &owner_token_b64, 3600)
        .await
        .expect("extend should succeed");
    assert_eq!(extend_resp.share_id, share_id);
    assert_ne!(
        extend_resp.expires_at, original_expires,
        "expires_at should change"
    );
    eprintln!("extended to: {}", extend_resp.expires_at);

    // 9. Extend with wrong owner_token should fail.
    let wrong_token = b64url_encode(&[0x00; 32]);
    let bad_extend = client.extend(&share_id, &wrong_token, 60).await;
    assert!(
        bad_extend.is_err(),
        "extend with wrong token should fail, got: {:?}",
        bad_extend
    );

    // 10. Delete.
    client
        .delete(&share_id, &owner_token_b64)
        .await
        .expect("delete should succeed");

    // 11. Verify 404 after delete.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let after_delete = client.fetch_meta(&share_id).await;
    assert!(
        after_delete.is_err(),
        "share should be gone after delete, got: {:?}",
        after_delete
    );

    eprintln!("full e2e flow completed");
}

#[tokio::test]
#[ignore = "requires live CF Worker"]
async fn invalid_share_id_rejected() {
    let client = ShareHttpClient::new(test_url()).unwrap();
    let result = client.fetch_meta("BAD_ID_WITH_CAPS_AND_IOU").await;
    assert!(result.is_err());
}

#[tokio::test]
#[ignore = "requires live CF Worker"]
async fn nonexistent_share_404() {
    let client = ShareHttpClient::new(test_url()).unwrap();
    // Valid 10-char Crockford format but almost certainly doesn't exist.
    let result = client.fetch_meta("0123456789").await;
    assert!(result.is_err());
    let err = result.unwrap_err();
    let msg = err.to_string().to_lowercase();
    assert!(
        msg.contains("not found") || msg.contains("404"),
        "expected not-found error, got: {}",
        err
    );
}
