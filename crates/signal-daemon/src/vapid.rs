use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use p256::ecdsa::{SigningKey, VerifyingKey};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VapidKeys {
    #[serde(rename = "private_key")]
    pub private_key: String,
    #[serde(rename = "public_key")]
    pub public_key: String,
}

#[derive(Debug, Serialize)]
pub struct VapidDiagnostics {
    #[serde(rename = "publicKey")]
    pub public_key: String,
    pub length: usize,
    pub first_byte: u8,
}

pub fn load_or_generate_vapid_keys(path: &Path) -> anyhow::Result<VapidKeys> {
    if path.exists() {
        let content = fs::read_to_string(path)?;
        let keys: VapidKeys = serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Invalid VAPID keys file: {}", e))?;

        if let Err(e) = validate_vapid_keys(&keys) {
            warn!("VAPID keys file invalid, regenerating: {}", e);
            let new_keys = generate_vapid_keys()?;
            save_vapid_keys(path, &new_keys)?;
            return Ok(new_keys);
        }

        info!("Loaded existing VAPID keys from {}", path.display());
        return Ok(keys);
    }

    let keys = generate_vapid_keys()?;
    save_vapid_keys(path, &keys)?;
    info!("Generated new VAPID keys at {}", path.display());
    Ok(keys)
}

fn generate_vapid_keys() -> anyhow::Result<VapidKeys> {
    let signing_key = SigningKey::random(&mut OsRng);
    let verifying_key = VerifyingKey::from(&signing_key);

    let private_bytes: [u8; 32] = signing_key.to_bytes().into();
    let public_bytes = verifying_key.to_encoded_point(false);

    let private_key = URL_SAFE_NO_PAD.encode(&private_bytes);
    let public_key = URL_SAFE_NO_PAD.encode(public_bytes.as_bytes());

    Ok(VapidKeys {
        private_key,
        public_key,
    })
}

fn save_vapid_keys(path: &Path, keys: &VapidKeys) -> anyhow::Result<()> {
    let content = serde_json::to_string_pretty(keys)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}

pub fn validate_vapid_keys(keys: &VapidKeys) -> anyhow::Result<()> {
    let private_bytes = URL_SAFE_NO_PAD.decode(&keys.private_key)?;
    if private_bytes.len() != 32 {
        anyhow::bail!("Private key must be 32 bytes, got {}", private_bytes.len());
    }

    let public_bytes = URL_SAFE_NO_PAD.decode(&keys.public_key)?;
    if public_bytes.len() != 65 {
        anyhow::bail!("Public key must be 65 bytes, got {}", public_bytes.len());
    }

    if public_bytes[0] != 0x04 {
        anyhow::bail!(
            "Public key must start with 0x04, got {:02x}",
            public_bytes[0]
        );
    }

    Ok(())
}

pub fn get_diagnostics(public_key: &str) -> anyhow::Result<VapidDiagnostics> {
    let bytes = URL_SAFE_NO_PAD.decode(public_key)?;
    let first_byte = bytes.first().copied().unwrap_or(0);
    Ok(VapidDiagnostics {
        public_key: public_key.to_string(),
        length: bytes.len(),
        first_byte,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_vapid_keys() {
        let keys = generate_vapid_keys().expect("Failed to generate VAPID keys");
        validate_vapid_keys(&keys).expect("Generated keys failed validation");
    }

    #[test]
    fn test_public_key_is_65_bytes_with_0x04_prefix() {
        let keys = generate_vapid_keys().expect("Failed to generate VAPID keys");
        let diag = get_diagnostics(&keys.public_key).expect("Failed to get diagnostics");
        assert_eq!(diag.length, 65, "Public key should be 65 bytes");
        assert_eq!(diag.first_byte, 0x04, "Public key should start with 0x04");
    }

    #[test]
    fn test_private_key_is_32_bytes() {
        let keys = generate_vapid_keys().expect("Failed to generate VAPID keys");
        let private_bytes = URL_SAFE_NO_PAD
            .decode(&keys.private_key)
            .expect("Failed to decode private key");
        assert_eq!(private_bytes.len(), 32, "Private key should be 32 bytes");
    }

    #[test]
    fn test_invalid_keys_regeneration() {
        let bad_keys = VapidKeys {
            private_key: "invalid".to_string(),
            public_key: "also-invalid".to_string(),
        };
        let result = validate_vapid_keys(&bad_keys);
        assert!(result.is_err(), "Invalid keys should fail validation");
    }
}
