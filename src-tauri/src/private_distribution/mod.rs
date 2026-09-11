//! Private distribution core; application wiring belongs to the activation task.
pub mod client;
pub mod lease;
pub mod model;
pub mod vault;

#[cfg(test)]
#[path = "../security.rs"]
mod security;
#[cfg(not(test))]
use crate::security;

#[cfg(test)]
mod private_distribution_tests {
    use super::{
        lease::{evaluate_lease_at, evaluate_status_at, verify_lease},
        model::{LicenseStatus, StoredCredential},
        vault::CredentialVault,
    };
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    // Node WebCrypto ECDSA P-256/SHA-256, the same four ordered claims as crypto.ts.
    // Only the public key and signed messages are retained; no private material.
    const KEY: &str =
        "BKeOTfnMcq2y3_jgJSTkWgVjRo-ALAmxJ3k1JwF-i5kF2ae6pA6gBJtDFG_-fm3kAvNu8qfwgddtZ86h-VeMWhQ";
    const LEASE: &str = "eyJkZXZpY2VJZCI6InRlc3QtZGV2aWNlIiwiaXNzdWVkQXQiOjEwMCwiZXhwaXJlc0F0Ijo2MDQ5MDAsInNlcnZlclRpbWUiOjEwMH0.ss4Rc3gKB3BF5ZH1q_0UF7-C8i9i-1U2NwssIdLbsCe1NOjbSUcUlKP9w_ymTVfNdMZgSK6ruymRW8k0ERAJ8w";
    const LATER: &str = "eyJkZXZpY2VJZCI6InRlc3QtZGV2aWNlIiwiaXNzdWVkQXQiOjEwMDAsImV4cGlyZXNBdCI6NjA1ODAwLCJzZXJ2ZXJUaW1lIjoxMDAwfQ.ztNA39tXx1lxHHcrr79-GiUqy28QN9PpKPfAF_mZK7iB2AoW8Lcl766-H34SQ_9692cA_DqOli5bhH9AxaufqA";
    fn key() -> Vec<u8> {
        URL_SAFE_NO_PAD.decode(KEY).unwrap()
    }
    fn credential() -> StoredCredential {
        StoredCredential {
            install_id: "test-install".into(),
            device_token: "a".repeat(43),
            lease: LEASE.into(),
            last_server_time: 100,
        }
    }

    #[test]
    fn valid_lease_allows_exactly_seven_days() {
        for (now, expected) in [
            (100, LicenseStatus::Valid),
            (101, LicenseStatus::OfflineGrace),
            (604899, LicenseStatus::OfflineGrace),
            (604900, LicenseStatus::Expired),
            (604901, LicenseStatus::Expired),
        ] {
            assert_eq!(
                evaluate_lease_at(LEASE, &key(), now, 100).unwrap(),
                expected
            );
        }
        assert_eq!(verify_lease(LEASE, &key()).unwrap().expires_at, 604900);
    }
    #[test]
    fn clock_rollback_requires_online_validation() {
        assert_eq!(
            evaluate_lease_at(LATER, &key(), 800, 1000).unwrap(),
            LicenseStatus::ClockInvalid
        );
        assert_eq!(
            evaluate_lease_at(LATER, &key(), 999, 1000).unwrap(),
            LicenseStatus::ClockInvalid
        );
        assert_eq!(
            evaluate_lease_at(LEASE, &key(), 100, 101).unwrap(),
            LicenseStatus::ClockInvalid
        );
    }
    #[test]
    fn revoked_and_expired_cannot_fall_back_to_offline_grace() {
        assert_eq!(
            evaluate_status_at(Some(LEASE), &key(), 700000, 100, true).unwrap(),
            LicenseStatus::Revoked
        );
        assert_eq!(
            evaluate_status_at(Some(LEASE), &key(), 0, 100, true).unwrap(),
            LicenseStatus::Revoked
        );
        assert_eq!(
            evaluate_status_at(None, &key(), 100, 100, false).unwrap(),
            LicenseStatus::Unactivated
        );
    }
    #[test]
    fn signatures_reject_tampering_and_malformed_wire_data() {
        let (body, signature) = LEASE.split_once('.').unwrap();
        for candidate in [
            format!("{body}.AA"),
            format!("{body}.{signature}="),
            format!("{body}.{signature}.extra"),
            format!("{body}.{}", URL_SAFE_NO_PAD.encode([0u8; 64])),
        ] {
            assert!(verify_lease(&candidate, &key()).is_err());
        }
        for body in [
            r#"{"deviceId":"other","issuedAt":100,"expiresAt":604900,"serverTime":100}"#,
            r#"{"deviceId":"test-device","issuedAt":100,"expiresAt":604900,"serverTime":100,"alg":"none"}"#,
            r#"{"deviceId":"test-device","issuedAt":100,"issuedAt":100,"expiresAt":604900,"serverTime":100}"#,
            r#"{"deviceId":"test-device","issuedAt":100,"expiresAt":604901,"serverTime":100}"#,
            r#"{"deviceId":"test-device","issuedAt":9007199254740992,"expiresAt":9007199255345792,"serverTime":9007199254740992}"#,
        ] {
            assert!(verify_lease(
                &format!("{}.{signature}", URL_SAFE_NO_PAD.encode(body)),
                &key()
            )
            .is_err());
        }
        assert!(evaluate_lease_at(LEASE, &key(), i64::MAX, 100).is_err());
        assert!(evaluate_lease_at(LEASE, &key(), 100, -1).is_err());
        assert!(verify_lease(LEASE, &[4; 65]).is_err());
    }

    #[test]
    fn signed_noncanonical_duplicate_unknown_missing_and_overflow_claims_are_rejected() {
        use ring::{
            rand::SystemRandom,
            signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING},
        };
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let signer =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
                .unwrap();
        for body in [
            r#"{"deviceId":"d","issuedAt":100,"expiresAt":604900,"serverTime":100,"alg":"ES256"}"#,
            r#"{"deviceId":"d","issuedAt":100,"issuedAt":100,"expiresAt":604900,"serverTime":100}"#,
            r#"{"deviceId":"d","issuedAt":100,"expiresAt":604900}"#,
            r#"{"deviceId":"d", "issuedAt":100,"expiresAt":604900,"serverTime":100}"#,
            r#"{"issuedAt":100,"deviceId":"d","expiresAt":604900,"serverTime":100}"#,
            r#"{"deviceId":"d","issuedAt":100.0,"expiresAt":604900,"serverTime":100}"#,
            r#"{"deviceId":"d","issuedAt":100,"expiresAt":604901,"serverTime":100}"#,
            r#"{"deviceId":"d","issuedAt":100,"expiresAt":604900,"serverTime":99}"#,
            r#"{"deviceId":"d","issuedAt":9007199254740992,"expiresAt":9007199255345792,"serverTime":9007199254740992}"#,
            r#"{"deviceId":"d","issuedAt":9223372036854775808,"expiresAt":9223372036855380608,"serverTime":9223372036854775808}"#,
        ] {
            let sig = signer.sign(&rng, body.as_bytes()).unwrap();
            let lease = format!(
                "{}.{}",
                URL_SAFE_NO_PAD.encode(body),
                URL_SAFE_NO_PAD.encode(sig.as_ref())
            );
            assert!(verify_lease(&lease, signer.public_key().as_ref()).is_err());
        }
    }

    #[test]
    fn webcrypto_high_s_and_low_s_are_both_valid() {
        let (body, sig) = LEASE.split_once('.').unwrap();
        let mut sig = URL_SAFE_NO_PAD.decode(sig).unwrap();
        // P-256 subgroup order, big endian. (r,n-s) verifies the same message.
        let order: [u8; 32] = [
            0xff, 0xff, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xbc, 0xe6, 0xfa, 0xad, 0xa7, 0x17, 0x9e, 0x84, 0xf3, 0xb9, 0xca, 0xc2,
            0xfc, 0x63, 0x25, 0x51,
        ];
        let mut borrow = 0i16;
        for i in (0..32).rev() {
            let value = order[i] as i16 - sig[32 + i] as i16 - borrow;
            sig[32 + i] = value.rem_euclid(256) as u8;
            borrow = i16::from(value < 0);
        }
        assert!(verify_lease(LEASE, &key()).is_ok());
        assert!(verify_lease(&format!("{body}.{}", URL_SAFE_NO_PAD.encode(sig)), &key()).is_ok());
    }
    #[test]
    fn credential_wire_is_camel_case_and_debug_is_redacted() {
        let value = serde_json::to_value(credential()).unwrap();
        assert_eq!(value["installId"], "test-install");
        assert_eq!(value["lastServerTime"], 100);
        assert!(value.get("deviceToken").is_some());
        assert!(!format!("{:?}", credential()).contains(&"a".repeat(43)));
        for (state, wire) in [
            (LicenseStatus::Unactivated, "unactivated"),
            (LicenseStatus::Valid, "valid"),
            (LicenseStatus::OfflineGrace, "offline_grace"),
            (LicenseStatus::Expired, "expired"),
            (LicenseStatus::Revoked, "revoked"),
            (LicenseStatus::ClockInvalid, "clock_invalid"),
        ] {
            assert_eq!(serde_json::to_value(state).unwrap(), wire);
        }
    }
    #[cfg(windows)]
    #[test]
    fn vault_file_never_contains_plain_token() {
        let root = tempfile::tempdir().unwrap();
        let vault = CredentialVault::at(root.path().join("device.dpapi"));
        assert!(vault.load().unwrap().is_none());
        vault.store(&credential()).unwrap();
        let bytes = std::fs::read(vault.path()).unwrap();
        assert!(!bytes
            .windows(43)
            .any(|part| part == "a".repeat(43).as_bytes()));
        assert_eq!(
            vault.load().unwrap().unwrap().device_token,
            credential().device_token
        );
        let mut updated = credential();
        updated.install_id = "replacement".into();
        vault.store(&updated).unwrap();
        assert_eq!(vault.load().unwrap().unwrap().install_id, "replacement");
        assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
        std::fs::write(vault.path(), b"corrupt").unwrap();
        assert!(vault.load().is_err());
        vault.clear().unwrap();
        vault.clear().unwrap();
        assert!(vault.load().unwrap().is_none());
    }
    #[cfg(not(windows))]
    #[test]
    fn vault_fails_closed_without_dpapi() {
        let root = tempfile::tempdir().unwrap();
        let vault = CredentialVault::at(root.path().join("device.dpapi"));
        assert!(vault.store(&credential()).is_err());
        assert!(!vault.path().exists());
    }

    #[test]
    fn malformed_lease_never_authorizes() {
        assert!(evaluate_lease_at("invalid", &[], 100, 100).is_err());
        assert_eq!(
            serde_json::to_string(&LicenseStatus::OfflineGrace).unwrap(),
            "\"offline_grace\""
        );
    }
}
