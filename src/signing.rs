// Manages signing and verification of journal events.

use std::path::Path;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey, Signature};
use crate::types::JournalEvent;

fn load_key_json(key_path: &Path) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let data = std::fs::read_to_string(key_path)?;
    Ok(serde_json::from_str(&data)?)
}

pub fn sign_event(event: &mut JournalEvent, key_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let kv = load_key_json(key_path)?;
    let secret_b64 = kv["secret_key"].as_str().ok_or("missing secret_key")?;
    let secret_bytes = B64.decode(secret_b64)?;
    if secret_bytes.len() != 32 {
        return Err("secret_key must be 32 bytes".into());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&secret_bytes);
    let signing_key = SigningKey::from_bytes(&arr);
    let payload = canonical_event_payload(event);
    let sig = signing_key.sign(payload.as_bytes());
    event.signature = Some(B64.encode(sig.to_bytes()));
    Ok(())
}

pub fn verify_event(event: &JournalEvent, key_path: &Path) -> Result<bool, Box<dyn std::error::Error>> {
    let sig_b64 = match &event.signature {
        Some(s) => s,
        None => return Ok(false),
    };
    let kv = load_key_json(key_path)?;
    let public_b64 = kv["public_key"].as_str().ok_or("missing public_key")?;
    let public_bytes = B64.decode(public_b64)?;
    if public_bytes.len() != 32 {
        return Err("public_key must be 32 bytes".into());
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&public_bytes);
    let verifying_key = VerifyingKey::from_bytes(&arr)?;
    let sig_bytes = B64.decode(sig_b64)?;
    if sig_bytes.len() != 64 {
        return Err("signature must be 64 bytes".into());
    }
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let signature = Signature::from_bytes(&sig_arr);
    let payload = canonical_event_payload(event);
    Ok(verifying_key.verify(payload.as_bytes(), &signature).is_ok())
}

pub fn canonical_event_payload(event: &JournalEvent) -> String {
    format!(
        "event_id={}|ts={}|organism_id={}|kind={}|severity={}|summary={}|continuity_epoch={}",
        event.event_id,
        event.ts_epoch_s,
        event.organism_id,
        event.kind,
        event.severity,
        event.summary,
        event.continuity_epoch,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::JournalEvent;

    fn sample_event() -> JournalEvent {
        JournalEvent {
            event_id: "e1".to_string(),
            ts_epoch_s: 1000,
            organism_id: "org-1".to_string(),
            runtime_habitat: "host_test".to_string(),
            runtime_instance_id: "run-1".to_string(),
            kind: "startup".to_string(),
            severity: "info".to_string(),
            summary: "test event".to_string(),
            reason: None,
            action: None,
            result: None,
            continuity_epoch: 0,
            signature: None,
        }
    }

    #[test]
    fn canonical_event_payload_is_deterministic() {
        let e = sample_event();
        let p1 = canonical_event_payload(&e);
        let p2 = canonical_event_payload(&e);
        assert_eq!(p1, p2);
        assert!(p1.contains("event_id=e1"));
        assert!(p1.contains("ts=1000"));
        assert!(p1.contains("organism_id=org-1"));
        assert!(p1.contains("kind=startup"));
        assert!(p1.contains("severity=info"));
        assert!(p1.contains("summary=test event"));
        assert!(p1.contains("continuity_epoch=0"));
    }

    #[test]
    fn canonical_payload_excludes_signature() {
        let mut e = sample_event();
        e.signature = Some("some_sig".to_string());
        let payload = canonical_event_payload(&e);
        assert!(!payload.contains("some_sig"));
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        use ed25519_dalek::SigningKey;
        use rand::rngs::OsRng;

        // Generate a key pair
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let secret_b64 = B64.encode(signing_key.to_bytes());
        let public_b64 = B64.encode(verifying_key.to_bytes());

        // Write key file to temp dir
        let dir = std::env::temp_dir().join(format!("oo-sign-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let key_path = dir.join("key.json");
        let key_json = serde_json::json!({
            "secret_key": secret_b64,
            "public_key": public_b64,
        });
        std::fs::write(&key_path, key_json.to_string()).unwrap();

        let mut event = sample_event();
        sign_event(&mut event, &key_path).unwrap();
        assert!(event.signature.is_some());

        let ok = verify_event(&event, &key_path).unwrap();
        assert!(ok);

        // Tamper with event — verify should fail
        event.summary = "tampered".to_string();
        let ok2 = verify_event(&event, &key_path).unwrap();
        assert!(!ok2);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
