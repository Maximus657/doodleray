use base64::{engine::general_purpose::STANDARD, Engine as _};
use minisign_verify::{PublicKey, Signature};
use std::{env, fs, path::PathBuf};

fn decode_outer_base64(value: &str, label: &str) -> Result<String, String> {
    let decoded = STANDARD
        .decode(value.trim())
        .map_err(|_| format!("{label} is not valid outer base64"))?;
    String::from_utf8(decoded).map_err(|_| format!("{label} does not contain UTF-8 minisign text"))
}

fn verify_updater_signature(
    artifact: &[u8],
    signature_outer_base64: &str,
    public_key_outer_base64: &str,
) -> Result<(), String> {
    let public_key_text = decode_outer_base64(public_key_outer_base64, "updater public key")?;
    let signature_text = decode_outer_base64(signature_outer_base64, "updater signature")?;
    let public_key = PublicKey::decode(&public_key_text)
        .map_err(|error| format!("invalid minisign public key: {error}"))?;
    let signature = Signature::decode(&signature_text)
        .map_err(|error| format!("invalid minisign signature: {error}"))?;
    public_key
        .verify(artifact, &signature, false)
        .map_err(|error| format!("signature does not match updater bytes: {error}"))
}

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let artifact_path = PathBuf::from(
        args.next()
            .ok_or("usage: verify_updater_signature <artifact> <signature> <tauri.conf.json>")?,
    );
    let signature_path = PathBuf::from(
        args.next()
            .ok_or("usage: verify_updater_signature <artifact> <signature> <tauri.conf.json>")?,
    );
    let config_path = PathBuf::from(
        args.next()
            .ok_or("usage: verify_updater_signature <artifact> <signature> <tauri.conf.json>")?,
    );
    if args.next().is_some() {
        return Err(
            "usage: verify_updater_signature <artifact> <signature> <tauri.conf.json>".into(),
        );
    }

    let artifact = fs::read(&artifact_path)
        .map_err(|error| format!("could not read updater artifact: {error}"))?;
    let signature = fs::read_to_string(&signature_path)
        .map_err(|error| format!("could not read updater signature: {error}"))?;
    let config: serde_json::Value = serde_json::from_slice(
        &fs::read(&config_path).map_err(|error| format!("could not read Tauri config: {error}"))?,
    )
    .map_err(|error| format!("could not parse Tauri config: {error}"))?;
    let public_key = config
        .pointer("/plugins/updater/pubkey")
        .and_then(serde_json::Value::as_str)
        .ok_or("Tauri config does not contain plugins.updater.pubkey")?;

    verify_updater_signature(&artifact, &signature, public_key)?;
    println!("Updater signature verified against the configured Tauri updater public key.");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Updater signature verification failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::verify_updater_signature;
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    const PUBLIC_KEY: &str = "untrusted comment: minisign public key E7620F1842B4E81F\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    const SIGNATURE: &str = "untrusted comment: signature from minisign secret key\nRUQf6LRCGA9i559r3g7V1qNyJDApGip8MfqcadIgT9CuhV3EMhHoN1mGTkUidF/z7SrlQgXdy8ofjb7bNJJylDOocrCo8KLzZwo=\ntrusted comment: timestamp:1556193335\tfile:test\ny/rUw2y8/hOUYjZU71eHp/Wo1KZ40fGy2VJEDl34XMJM+TX48Ss/17u3IvIfbVR1FkZZSNCisQbuQY+bHwhEBg==";

    #[test]
    fn verifies_tauri_outer_base64_signature_and_rejects_tampering() {
        let public_key = STANDARD.encode(PUBLIC_KEY);
        let signature = STANDARD.encode(SIGNATURE);

        verify_updater_signature(b"test", &signature, &public_key).unwrap();
        assert!(
            verify_updater_signature(b"tampered", &signature, &public_key).is_err(),
            "tampered updater bytes must fail verification"
        );
    }
}
