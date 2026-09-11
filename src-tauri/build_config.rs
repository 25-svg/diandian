use std::env;

pub const DEV_UPDATE_ENDPOINT: &str = "http://127.0.0.1:8787/";
pub const DEV_LICENSE_PUBLIC_KEY: &str =
    "BKeOTfnMcq2y3_jgJSTkWgVjRo-ALAmxJ3k1JwF-i5kF2ae6pA6gBJtDFG_-fm3kAvNu8qfwgddtZ86h-VeMWhQ";

fn decode_base64url(input: &str) -> Result<Vec<u8>, &'static str> {
    if input.is_empty() || input.contains('=') {
        return Err("must be unpadded base64url");
    }
    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut accumulator = 0u32;
    let mut bits = 0u8;
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return Err("must be unpadded base64url"),
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1u32 << bits).saturating_sub(1);
        }
    }
    if bits >= 6 || accumulator != 0 {
        return Err("has non-canonical trailing bits");
    }
    Ok(output)
}

pub fn validate_public_key(value: &str) -> Result<(), &'static str> {
    let bytes = decode_base64url(value)?;
    if bytes.len() != 65 || bytes.first() != Some(&4) {
        return Err("must encode one uncompressed 65-byte P-256 public key");
    }
    Ok(())
}

pub fn validate_endpoint(value: &str) -> Result<(), &'static str> {
    if !value.starts_with("https://") || value.len() <= "https://".len() {
        return Err("must be an HTTPS root origin");
    }
    let authority_and_path = &value["https://".len()..];
    if value
        .bytes()
        .any(|byte| byte.is_ascii_whitespace() || byte == b'\\')
        || authority_and_path.contains('@')
        || authority_and_path.contains('?')
        || authority_and_path.contains('#')
    {
        return Err("must not contain credentials, query, or fragment");
    }
    let (authority, path) = authority_and_path
        .split_once('/')
        .map_or((authority_and_path, ""), |(authority, path)| {
            (authority, path)
        });
    if authority.is_empty() || !path.is_empty() {
        return Err("must be a root origin");
    }
    Ok(())
}

pub fn configure() {
    println!("cargo:rerun-if-env-changed=DIANDIAN_UPDATE_ENDPOINT");
    println!("cargo:rerun-if-env-changed=DIANDIAN_LICENSE_PUBLIC_KEY");
    let release = env::var("PROFILE").as_deref() == Ok("release");
    let endpoint = env::var("DIANDIAN_UPDATE_ENDPOINT").ok();
    let public_key = env::var("DIANDIAN_LICENSE_PUBLIC_KEY").ok();
    if release {
        let mut missing = Vec::new();
        if endpoint.as_deref().unwrap_or_default().is_empty() {
            missing.push("DIANDIAN_UPDATE_ENDPOINT");
        }
        if public_key.as_deref().unwrap_or_default().is_empty() {
            missing.push("DIANDIAN_LICENSE_PUBLIC_KEY");
        }
        if !missing.is_empty() {
            panic!("missing required release variables: {}", missing.join(", "));
        }
    }
    let endpoint = endpoint.as_deref().unwrap_or(DEV_UPDATE_ENDPOINT);
    let public_key = public_key.as_deref().unwrap_or(DEV_LICENSE_PUBLIC_KEY);
    if release || endpoint != DEV_UPDATE_ENDPOINT {
        validate_endpoint(endpoint)
            .unwrap_or_else(|reason| panic!("invalid DIANDIAN_UPDATE_ENDPOINT: {reason}"));
    }
    validate_public_key(public_key)
        .unwrap_or_else(|reason| panic!("invalid DIANDIAN_LICENSE_PUBLIC_KEY: {reason}"));
    println!("cargo:rustc-env=DIANDIAN_UPDATE_ENDPOINT={endpoint}");
    println!("cargo:rustc-env=DIANDIAN_LEASE_PUBLIC_KEY={public_key}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_requires_https_root_without_credentials_or_suffixes() {
        assert!(validate_endpoint("https://updates.example.com/").is_ok());
        for invalid in [
            "http://updates.example.com/",
            "https://user@updates.example.com/",
            "https://updates.example.com/v1",
            "https://updates.example.com/?channel=prod",
            "https://updates.example.com/#prod",
        ] {
            assert!(validate_endpoint(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn public_key_is_canonical_uncompressed_p256() {
        assert!(validate_public_key(DEV_LICENSE_PUBLIC_KEY).is_ok());
        for invalid in ["", "AA", "BAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="] {
            assert!(validate_public_key(invalid).is_err());
        }
    }
}
