use sha2::{Digest, Sha256};

const TOKEN_PREFIX: &str = "sig_dev_";
const PAIRING_CODE_PREFIX: &str = "pair_";
const TOKEN_RANDOM_BYTES: usize = 32;

/// Generate a new device token in format: sig_dev_<base64url(32 random bytes)>
pub fn generate_device_token() -> String {
    use rand::Rng;

    let mut rng = rand::thread_rng();
    let mut random_bytes = vec![0u8; TOKEN_RANDOM_BYTES];
    rng.fill(&mut random_bytes[..]);

    let encoded = base64_url::encode(&random_bytes);
    format!("{}{}", TOKEN_PREFIX, encoded)
}

/// Generate a new pairing code in format: pair_<base64url(32 random bytes)>
pub fn generate_pairing_code() -> String {
    use rand::Rng;

    let mut rng = rand::thread_rng();
    let mut random_bytes = vec![0u8; TOKEN_RANDOM_BYTES];
    rng.fill(&mut random_bytes[..]);

    let encoded = base64_url::encode(&random_bytes);
    format!("{}{}", PAIRING_CODE_PREFIX, encoded)
}

/// Extract token prefix (first 12 characters) for safe logging/display
pub fn get_token_prefix(token: &str) -> String {
    if token.len() > 12 {
        token[..12].to_string()
    } else {
        token.to_string()
    }
}

/// Hash a token for storage (SHA-256)
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Verify a token against its hash
pub fn verify_token(token: &str, hash: &str) -> bool {
    hash_token(token) == hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_device_token_has_correct_format() {
        let token = generate_device_token();
        assert!(token.starts_with(TOKEN_PREFIX));
        assert!(token.len() > TOKEN_PREFIX.len());
    }

    #[test]
    fn generate_device_token_is_unique() {
        let token1 = generate_device_token();
        let token2 = generate_device_token();
        assert_ne!(token1, token2);
    }

    #[test]
    fn hash_token_is_consistent() {
        let token = "test_token";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn verify_token_works() {
        let token = "test_token";
        let hash = hash_token(token);
        assert!(verify_token(token, &hash));
    }

    #[test]
    fn verify_token_rejects_wrong_token() {
        let token = "test_token";
        let hash = hash_token(token);
        assert!(!verify_token("wrong_token", &hash));
    }

    #[test]
    fn generate_pairing_code_has_correct_format() {
        let code = generate_pairing_code();
        assert!(code.starts_with(PAIRING_CODE_PREFIX));
        assert!(code.len() > PAIRING_CODE_PREFIX.len());
    }

    #[test]
    fn generate_pairing_code_is_unique() {
        let code1 = generate_pairing_code();
        let code2 = generate_pairing_code();
        assert_ne!(code1, code2);
    }

    #[test]
    fn device_token_and_pairing_code_have_different_formats() {
        let token = generate_device_token();
        let code = generate_pairing_code();
        assert!(token.starts_with(TOKEN_PREFIX));
        assert!(code.starts_with(PAIRING_CODE_PREFIX));
        assert_ne!(token[..8], code[..8]); // Different prefixes
    }
}
