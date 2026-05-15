//! Canvas share module — ID generation, password handling, owner tokens,
//! encryption, and types.

pub mod binary_format;
pub mod crypto;
pub mod http_client;
pub mod local_store;
pub mod owner_token;
pub mod password;
pub mod service;
pub mod share_id;
pub mod types;

pub use binary_format::{deserialize, serialize, NfebHeader, MAGIC, VERSION};
pub use crypto::{decrypt_share_bundle, derive_key, encrypt_share_bundle};
pub use http_client::{
    ExtendResponse, MetaResponse, ShareHttpClient, UploadResponse, DEFAULT_BASE_URL,
};
pub use local_store::{
    decrypt_bytes_from_storage, decrypt_from_storage, encrypt_bytes_for_storage,
    encrypt_for_storage,
};
pub use owner_token::{generate_owner_token, hash_owner_token};
pub use password::{format_password, generate_password, parse_password};
pub use service::{CanvasShareService, ImportResult, ShareInfo, ShareResult, DEFAULT_TTL_SECS};
pub use share_id::{generate_share_id, validate_share_id};
pub use types::{EncryptedShareBundle, KdfParams, ShareBundle, ShareMetadata};
