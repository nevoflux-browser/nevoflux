//! NFEB (NevoFlux Encrypted Bundle) binary format.
//!
//! Compact binary encoding of an [`EncryptedShareBundle`] suitable for
//! efficient storage in R2 and transfer over the wire. The format
//! consists of a fixed magic string, a version byte, an LEB128-prefixed
//! JSON header (containing the share ID and KDF parameters), followed
//! by the raw nonce, ciphertext, and authentication tag.
//!
//! ```text
//! +--------+---------+----------+--------------+-----------+------------+----------+
//! | Magic  | Version | HdrLen   | JSON Header  | Nonce(12) | Ciphertext | AuthTag  |
//! | 4 bytes| 1 byte  | LEB128   | HdrLen bytes | 12 bytes  | variable   | 16 bytes |
//! +--------+---------+----------+--------------+-----------+------------+----------+
//! ```

use crate::error::{DaemonError, Result};
use crate::share::types::{EncryptedShareBundle, KdfParams};
use serde::{Deserialize, Serialize};

/// Magic bytes identifying an NFEB buffer: ASCII `"NFEB"`.
pub const MAGIC: &[u8; 4] = b"NFEB";
/// Current format version.
pub const VERSION: u8 = 1;
/// Length of the AES-GCM nonce in bytes.
pub const NONCE_LEN: usize = 12;
/// Length of the AES-GCM authentication tag in bytes.
pub const AUTH_TAG_LEN: usize = 16;

/// JSON header containing metadata needed to decrypt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NfebHeader {
    /// Share identifier (used as part of the KDF salt).
    pub share_id: String,
    /// Key derivation parameters.
    pub kdf_params: KdfParams,
}

/// Serialize an [`EncryptedShareBundle`] to the NFEB binary format.
pub fn serialize(bundle: &EncryptedShareBundle) -> Result<Vec<u8>> {
    // Validate fixed-length fields.
    if bundle.nonce.len() != NONCE_LEN {
        return Err(DaemonError::InternalError(format!(
            "Expected {}-byte nonce, got {}",
            NONCE_LEN,
            bundle.nonce.len()
        )));
    }
    if bundle.auth_tag.len() != AUTH_TAG_LEN {
        return Err(DaemonError::InternalError(format!(
            "Expected {}-byte auth tag, got {}",
            AUTH_TAG_LEN,
            bundle.auth_tag.len()
        )));
    }

    let header = NfebHeader {
        share_id: bundle.share_id.clone(),
        kdf_params: bundle.kdf_params.clone(),
    };
    let header_json = serde_json::to_vec(&header)
        .map_err(|e| DaemonError::InternalError(format!("Header JSON error: {}", e)))?;

    let mut out = Vec::with_capacity(
        4 + 1 + 10 + header_json.len() + NONCE_LEN + bundle.ciphertext.len() + AUTH_TAG_LEN,
    );

    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    write_leb128(&mut out, header_json.len() as u64);
    out.extend_from_slice(&header_json);
    out.extend_from_slice(&bundle.nonce);
    out.extend_from_slice(&bundle.ciphertext);
    out.extend_from_slice(&bundle.auth_tag);

    Ok(out)
}

/// Deserialize a NFEB binary buffer into an [`EncryptedShareBundle`].
pub fn deserialize(data: &[u8]) -> Result<EncryptedShareBundle> {
    let mut pos = 0;

    // Magic.
    if data.len() < 5 || &data[0..4] != MAGIC {
        return Err(DaemonError::InvalidRequest("Invalid NFEB magic".into()));
    }
    pos += 4;

    // Version.
    if data[pos] != VERSION {
        return Err(DaemonError::InvalidRequest(format!(
            "Unsupported NFEB version: {}",
            data[pos]
        )));
    }
    pos += 1;

    // Header length (LEB128).
    let (hdr_len, consumed) = read_leb128(&data[pos..])
        .ok_or_else(|| DaemonError::InvalidRequest("Invalid LEB128 header length".into()))?;
    pos += consumed;

    if pos + hdr_len as usize > data.len() {
        return Err(DaemonError::InvalidRequest("Truncated header".into()));
    }

    // Header JSON.
    let header_bytes = &data[pos..pos + hdr_len as usize];
    let header: NfebHeader = serde_json::from_slice(header_bytes)
        .map_err(|e| DaemonError::InvalidRequest(format!("Invalid header JSON: {}", e)))?;
    pos += hdr_len as usize;

    // Nonce.
    if pos + NONCE_LEN > data.len() {
        return Err(DaemonError::InvalidRequest("Truncated nonce".into()));
    }
    let nonce = data[pos..pos + NONCE_LEN].to_vec();
    pos += NONCE_LEN;

    // Remaining = ciphertext + auth_tag.
    if data.len() < pos + AUTH_TAG_LEN {
        return Err(DaemonError::InvalidRequest("Missing auth tag".into()));
    }
    let ciphertext_end = data.len() - AUTH_TAG_LEN;
    let ciphertext = data[pos..ciphertext_end].to_vec();
    let auth_tag = data[ciphertext_end..].to_vec();

    Ok(EncryptedShareBundle {
        share_id: header.share_id,
        kdf_params: header.kdf_params,
        nonce,
        ciphertext,
        auth_tag,
    })
}

/// Write an unsigned integer as LEB128.
fn write_leb128(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Read an unsigned LEB128. Returns `(value, bytes_consumed)` on success,
/// or `None` on overflow/truncation.
fn read_leb128(data: &[u8]) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0;
    for (i, &byte) in data.iter().enumerate() {
        if i >= 10 {
            return None; // overflow guard (u64 needs at most 10 bytes)
        }
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some((value, i + 1));
        }
        shift += 7;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bundle() -> EncryptedShareBundle {
        EncryptedShareBundle {
            share_id: "ABCDEFGHJK".into(),
            kdf_params: KdfParams::default(),
            nonce: (0u8..12).collect(),
            ciphertext: vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE],
            auth_tag: (0u8..16).collect(),
        }
    }

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let bundle = sample_bundle();
        let bytes = serialize(&bundle).expect("serialize");
        let decoded = deserialize(&bytes).expect("deserialize");

        assert_eq!(decoded.share_id, bundle.share_id);
        assert_eq!(decoded.nonce, bundle.nonce);
        assert_eq!(decoded.ciphertext, bundle.ciphertext);
        assert_eq!(decoded.auth_tag, bundle.auth_tag);
        assert_eq!(decoded.kdf_params.algorithm, bundle.kdf_params.algorithm);
        assert_eq!(decoded.kdf_params.memory_kib, bundle.kdf_params.memory_kib);
        assert_eq!(decoded.kdf_params.iterations, bundle.kdf_params.iterations);
        assert_eq!(
            decoded.kdf_params.parallelism,
            bundle.kdf_params.parallelism
        );
        assert_eq!(
            decoded.kdf_params.salt_format,
            bundle.kdf_params.salt_format
        );
    }

    #[test]
    fn test_magic_bytes() {
        let bundle = sample_bundle();
        let bytes = serialize(&bundle).expect("serialize");
        assert_eq!(&bytes[0..4], b"NFEB");
        assert_eq!(&bytes[0..4], MAGIC);
    }

    #[test]
    fn test_version_byte() {
        let bundle = sample_bundle();
        let bytes = serialize(&bundle).expect("serialize");
        assert_eq!(bytes[4], 0x01);
        assert_eq!(bytes[4], VERSION);
    }

    #[test]
    fn test_rejects_wrong_magic() {
        let bundle = sample_bundle();
        let mut bytes = serialize(&bundle).expect("serialize");
        bytes[0] = b'X';
        let err = deserialize(&bytes).expect_err("should reject wrong magic");
        match err {
            DaemonError::InvalidRequest(msg) => assert!(msg.contains("magic")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_rejects_wrong_version() {
        let bundle = sample_bundle();
        let mut bytes = serialize(&bundle).expect("serialize");
        bytes[4] = 0xFF;
        let err = deserialize(&bytes).expect_err("should reject wrong version");
        match err {
            DaemonError::InvalidRequest(msg) => assert!(msg.contains("version")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_rejects_truncated_data() {
        let bundle = sample_bundle();
        let bytes = serialize(&bundle).expect("serialize");

        // Too short to even contain magic + version.
        assert!(deserialize(&bytes[0..3]).is_err());
        assert!(deserialize(&[]).is_err());

        // Truncate in the middle of the header.
        let header_cut = 5 + 2; // magic + version + part of hdr_len
        assert!(deserialize(&bytes[..header_cut.min(bytes.len())]).is_err());

        // Truncate inside the nonce region.
        let near_end = bytes.len() - (NONCE_LEN + 6 + AUTH_TAG_LEN) + 5;
        assert!(deserialize(&bytes[..near_end]).is_err());

        // Missing auth tag.
        let without_tag = bytes.len() - AUTH_TAG_LEN - 1;
        assert!(deserialize(&bytes[..without_tag]).is_err());
    }

    #[test]
    fn test_leb128_encode_decode() {
        let values = [
            0u64,
            1,
            126,
            127,
            128,
            129,
            255,
            256,
            16383,
            16384,
            16385,
            2_097_151,
            2_097_152,
            u32::MAX as u64,
            u64::MAX,
        ];

        for &v in &values {
            let mut buf = Vec::new();
            write_leb128(&mut buf, v);
            let (decoded, consumed) = read_leb128(&buf).expect("read_leb128");
            assert_eq!(decoded, v, "roundtrip failed for {}", v);
            assert_eq!(consumed, buf.len(), "consumed mismatch for {}", v);
        }

        // Specific encoding checks.
        let mut buf = Vec::new();
        write_leb128(&mut buf, 0);
        assert_eq!(buf, vec![0x00]);

        buf.clear();
        write_leb128(&mut buf, 127);
        assert_eq!(buf, vec![0x7F]);

        buf.clear();
        write_leb128(&mut buf, 128);
        assert_eq!(buf, vec![0x80, 0x01]);

        buf.clear();
        write_leb128(&mut buf, 16384);
        assert_eq!(buf, vec![0x80, 0x80, 0x01]);

        // Overflow: 11 continuation bytes should be rejected.
        let bad = vec![0xFF; 11];
        assert!(read_leb128(&bad).is_none());
    }

    #[test]
    fn test_header_extraction() {
        // Verify the header JSON can be parsed out of the raw bytes
        // without needing to decrypt anything.
        let bundle = sample_bundle();
        let bytes = serialize(&bundle).expect("serialize");

        // Skip magic + version.
        let mut pos = 5;
        let (hdr_len, consumed) = read_leb128(&bytes[pos..]).expect("read_leb128");
        pos += consumed;

        let header_bytes = &bytes[pos..pos + hdr_len as usize];
        let header: NfebHeader = serde_json::from_slice(header_bytes).expect("parse header JSON");

        assert_eq!(header.share_id, "ABCDEFGHJK");
        assert_eq!(header.kdf_params.algorithm, "argon2id");

        // The header must NOT contain nonce/ciphertext/auth_tag fields.
        let raw: serde_json::Value =
            serde_json::from_slice(header_bytes).expect("parse header as value");
        assert!(raw.get("nonce").is_none());
        assert!(raw.get("ciphertext").is_none());
        assert!(raw.get("auth_tag").is_none());
        assert!(raw.get("share_id").is_some());
        assert!(raw.get("kdf_params").is_some());
    }

    #[test]
    fn test_rejects_bad_nonce_length() {
        let mut bundle = sample_bundle();
        bundle.nonce = vec![0u8; 8];
        let err = serialize(&bundle).expect_err("should reject bad nonce");
        match err {
            DaemonError::InternalError(msg) => assert!(msg.contains("nonce")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_rejects_bad_auth_tag_length() {
        let mut bundle = sample_bundle();
        bundle.auth_tag = vec![0u8; 12];
        let err = serialize(&bundle).expect_err("should reject bad auth tag");
        match err {
            DaemonError::InternalError(msg) => assert!(msg.contains("auth tag")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_rejects_bad_header_json() {
        // Build a buffer with valid magic+version but corrupt JSON.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(VERSION);
        let bad_json = b"not json";
        write_leb128(&mut bytes, bad_json.len() as u64);
        bytes.extend_from_slice(bad_json);
        bytes.extend_from_slice(&[0u8; NONCE_LEN]);
        bytes.extend_from_slice(&[0u8; AUTH_TAG_LEN]);

        let err = deserialize(&bytes).expect_err("should reject bad JSON");
        match err {
            DaemonError::InvalidRequest(msg) => assert!(msg.contains("header JSON")),
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn test_large_ciphertext_roundtrip() {
        let mut bundle = sample_bundle();
        bundle.ciphertext = vec![0xAB; 100_000];
        let bytes = serialize(&bundle).expect("serialize");
        let decoded = deserialize(&bytes).expect("deserialize");
        assert_eq!(decoded.ciphertext.len(), 100_000);
        assert_eq!(decoded.ciphertext, bundle.ciphertext);
    }

    #[test]
    fn test_empty_ciphertext_roundtrip() {
        let mut bundle = sample_bundle();
        bundle.ciphertext = Vec::new();
        let bytes = serialize(&bundle).expect("serialize");
        let decoded = deserialize(&bytes).expect("deserialize");
        assert!(decoded.ciphertext.is_empty());
        assert_eq!(decoded.nonce, bundle.nonce);
        assert_eq!(decoded.auth_tag, bundle.auth_tag);
    }
}
