//! Key resolution from external secret managers.
//!
//! Supported URI schemes:
//!
//! | URI | Description |
//! |-----|-------------|
//! | `file:///path/to/key` | 32-byte binary key file |
//! | `env://VAR_NAME` | hex (64 chars) or base64 key stored in an env var |
//! | `aws-secrets-manager://secret-id` | AWS Secrets Manager — string (hex/base64) or binary value |
//! | `vault://mount/path?field=name` | HashiCorp Vault KV (requires `VAULT_ADDR` + `VAULT_TOKEN`); `field` defaults to `"key"`; supports KV v1 and v2 |

use base64::prelude::*;

use crate::error::{Result, StoreError};

/// Resolve a `key_source` URI to a raw 32-byte encryption key.
pub async fn resolve(source: &str) -> Result<[u8; 32]> {
    if let Some(var) = source.strip_prefix("env://") {
        resolve_env(var)
    } else if source.starts_with("file://") {
        resolve_file_url(source)
    } else if let Some(secret_id) = source.strip_prefix("aws-secrets-manager://") {
        resolve_aws_sm(secret_id).await
    } else if source.starts_with("vault://") {
        resolve_vault(source).await
    } else {
        Err(StoreError::InvalidUrl(format!("unsupported key_source scheme: {:?}", source)))
    }
}

// ── env:// ───────────────────────────────────────────────────────────────────

fn resolve_env(var: &str) -> Result<[u8; 32]> {
    let val = std::env::var(var).map_err(|_| StoreError::KeySource(format!("env var {var:?} is not set")))?;
    parse_key_str(val.trim())
}

// ── file:// ──────────────────────────────────────────────────────────────────

fn resolve_file_url(source: &str) -> Result<[u8; 32]> {
    let url = url::Url::parse(source).map_err(|e| StoreError::InvalidUrl(e.to_string()))?;
    let path = std::path::Path::new(url.path());
    crate::factory::read_key_file(path)
}

// ── aws-secrets-manager:// ───────────────────────────────────────────────────

async fn resolve_aws_sm(secret_id: &str) -> Result<[u8; 32]> {
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest()).load().await;
    let client = aws_sdk_secretsmanager::Client::new(&config);
    let resp = client
        .get_secret_value()
        .secret_id(secret_id)
        .send()
        .await
        .map_err(|e| StoreError::KeySource(format!("AWS Secrets Manager: {e}")))?;

    if let Some(s) = resp.secret_string() {
        parse_key_str(s.trim())
    } else if let Some(blob) = resp.secret_binary() {
        let bytes = blob.as_ref();
        bytes_to_key(bytes)
    } else {
        Err(StoreError::KeySource("secret has neither string nor binary value".to_owned()))
    }
}

// ── vault:// ─────────────────────────────────────────────────────────────────

async fn resolve_vault(source: &str) -> Result<[u8; 32]> {
    let url = url::Url::parse(source).map_err(|e| StoreError::InvalidUrl(e.to_string()))?;

    // Reconstruct vault path from host + path segments (e.g. vault://secret/data/foo → secret/data/foo).
    let host = url.host_str().unwrap_or("");
    let path = url.path().trim_start_matches('/');
    let vault_path = if path.is_empty() { host.to_owned() } else { format!("{host}/{path}") };

    let field =
        url.query_pairs().find(|(k, _)| k == "field").map(|(_, v)| v.into_owned()).unwrap_or_else(|| "key".to_owned());

    let vault_addr =
        std::env::var("VAULT_ADDR").map_err(|_| StoreError::KeySource("VAULT_ADDR env var is not set".to_owned()))?;
    let vault_token =
        std::env::var("VAULT_TOKEN").map_err(|_| StoreError::KeySource("VAULT_TOKEN env var is not set".to_owned()))?;

    let api_url = format!("{}/{}", vault_addr.trim_end_matches('/'), vault_path);

    let resp = reqwest::Client::new()
        .get(&api_url)
        .header("X-Vault-Token", &vault_token)
        .send()
        .await
        .map_err(|e| StoreError::KeySource(format!("Vault HTTP: {e}")))?;

    if !resp.status().is_success() {
        return Err(StoreError::KeySource(format!("Vault returned HTTP {}", resp.status())));
    }

    let json: serde_json::Value =
        resp.json().await.map_err(|e| StoreError::KeySource(format!("Vault response JSON: {e}")))?;

    // KV v2 → data.data.<field>; KV v1 → data.<field>
    let val = json
        .pointer(&format!("/data/data/{field}"))
        .or_else(|| json.pointer(&format!("/data/{field}")))
        .and_then(|v| v.as_str())
        .ok_or_else(|| StoreError::KeySource(format!("field {field:?} not found in Vault response")))?;

    parse_key_str(val.trim())
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Parse a string key as hex (64 chars) or base64 into 32 bytes.
fn parse_key_str(s: &str) -> Result<[u8; 32]> {
    // Hex: exactly 64 hex characters.
    if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        if let Some(key) = decode_hex_32(s) {
            return Ok(key);
        }
    }

    // Base64: try standard, URL-safe, with and without padding.
    let decoded = BASE64_STANDARD
        .decode(s)
        .or_else(|_| BASE64_URL_SAFE.decode(s))
        .or_else(|_| BASE64_STANDARD_NO_PAD.decode(s))
        .or_else(|_| BASE64_URL_SAFE_NO_PAD.decode(s))
        .map_err(|_| StoreError::KeySource("key is not valid hex (64 chars) or base64".to_owned()))?;

    bytes_to_key(&decoded)
}

fn bytes_to_key(bytes: &[u8]) -> Result<[u8; 32]> {
    if bytes.len() == 32 {
        let mut key = [0u8; 32];
        key.copy_from_slice(bytes);
        Ok(key)
    } else {
        Err(StoreError::KeySource(format!("key must be 32 bytes, got {}", bytes.len())))
    }
}

fn decode_hex_32(s: &str) -> Option<[u8; 32]> {
    let mut key = [0u8; 32];
    for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        key[i] = (hi << 4) | lo;
    }
    Some(key)
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_key() {
        let hex = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
        let key = parse_key_str(hex).unwrap();
        assert_eq!(key[0], 0x01);
        assert_eq!(key[31], 0x20);
    }

    #[test]
    fn test_parse_base64_standard_key() {
        // base64 of 32 zero bytes
        let b64 = BASE64_STANDARD.encode([0u8; 32]);
        let key = parse_key_str(&b64).unwrap();
        assert_eq!(key, [0u8; 32]);
    }

    #[test]
    fn test_parse_base64_url_safe_no_pad_key() {
        let b64 = BASE64_URL_SAFE_NO_PAD.encode([0xffu8; 32]);
        let key = parse_key_str(&b64).unwrap();
        assert_eq!(key, [0xffu8; 32]);
    }

    #[test]
    fn test_parse_key_wrong_length() {
        assert!(parse_key_str("aabb").is_err());
    }

    #[test]
    fn test_resolve_env() {
        let hex = "0000000000000000000000000000000000000000000000000000000000000001";
        // safe: single-threaded test, no concurrent env access
        unsafe { std::env::set_var("_TOME_TEST_KEY_SOURCE", hex) };
        let key = resolve_env("_TOME_TEST_KEY_SOURCE").unwrap();
        assert_eq!(key[31], 0x01);
        unsafe { std::env::remove_var("_TOME_TEST_KEY_SOURCE") };
    }

    #[test]
    fn test_resolve_env_missing() {
        unsafe { std::env::remove_var("_TOME_TEST_KEY_MISSING") };
        assert!(resolve_env("_TOME_TEST_KEY_MISSING").is_err());
    }
}
