use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Awg2Obfuscation {
    pub jc: u16,
    pub jmin: u16,
    pub jmax: u16,
    pub s1: u16,
    pub s2: u16,
    pub s3: u16,
    pub s4: u16,
    pub h1: String,
    pub h2: String,
    pub h3: String,
    pub h4: String,
    #[serde(default)]
    pub i1: Option<String>,
    #[serde(default)]
    pub i2: Option<String>,
    #[serde(default)]
    pub i3: Option<String>,
    #[serde(default)]
    pub i4: Option<String>,
    #[serde(default)]
    pub i5: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Awg2Profile<'a> {
    pub server_address: &'a str,
    pub server_port: u16,
    pub private_key: &'a str,
    pub peer_public_key: &'a str,
    pub pre_shared_key: Option<&'a str>,
    pub addresses: &'a [String],
    pub dns_servers: &'a [String],
    pub allowed_ips: &'a [String],
    pub mtu: u16,
    pub persistent_keepalive_secs: u16,
    pub obfs: &'a Awg2Obfuscation,
}

pub fn render_wg_quick(profile: Awg2Profile<'_>) -> Result<String, String> {
    validate_host(profile.server_address)?;
    if profile.server_port == 0 {
        return Err("AWG2 profile has an invalid endpoint port".into());
    }
    validate_key(profile.private_key, "private key")?;
    validate_key(profile.peer_public_key, "peer public key")?;
    if let Some(key) = profile.pre_shared_key {
        validate_key(key, "preshared key")?;
    }
    validate_cidrs(profile.addresses, "address")?;
    validate_ips(profile.dns_servers, "DNS server")?;
    validate_cidrs(profile.allowed_ips, "allowed IP")?;
    if !(1280..=1500).contains(&profile.mtu) {
        return Err("AWG2 profile has an invalid MTU".into());
    }
    validate_obfuscation(profile.obfs, profile.mtu)?;

    let mut config = String::from("[Interface]\n");
    append_line(&mut config, "PrivateKey", profile.private_key);
    append_line(&mut config, "Address", &profile.addresses.join(", "));
    append_line(&mut config, "DNS", &profile.dns_servers.join(", "));
    append_line(&mut config, "MTU", &profile.mtu.to_string());
    append_line(&mut config, "Jc", &profile.obfs.jc.to_string());
    append_line(&mut config, "Jmin", &profile.obfs.jmin.to_string());
    append_line(&mut config, "Jmax", &profile.obfs.jmax.to_string());
    append_line(&mut config, "S1", &profile.obfs.s1.to_string());
    append_line(&mut config, "S2", &profile.obfs.s2.to_string());
    append_line(&mut config, "S3", &profile.obfs.s3.to_string());
    append_line(&mut config, "S4", &profile.obfs.s4.to_string());
    append_line(&mut config, "H1", &profile.obfs.h1);
    append_line(&mut config, "H2", &profile.obfs.h2);
    append_line(&mut config, "H3", &profile.obfs.h3);
    append_line(&mut config, "H4", &profile.obfs.h4);
    for (name, value) in [
        ("I1", profile.obfs.i1.as_deref()),
        ("I2", profile.obfs.i2.as_deref()),
        ("I3", profile.obfs.i3.as_deref()),
        ("I4", profile.obfs.i4.as_deref()),
        ("I5", profile.obfs.i5.as_deref()),
    ] {
        if let Some(value) = value {
            append_line(&mut config, name, value);
        }
    }
    config.push_str("\n[Peer]\n");
    append_line(&mut config, "PublicKey", profile.peer_public_key);
    if let Some(key) = profile.pre_shared_key {
        append_line(&mut config, "PresharedKey", key);
    }
    append_line(
        &mut config,
        "Endpoint",
        &format_endpoint(profile.server_address, profile.server_port),
    );
    append_line(&mut config, "AllowedIPs", &profile.allowed_ips.join(", "));
    append_line(
        &mut config,
        "PersistentKeepalive",
        &profile.persistent_keepalive_secs.to_string(),
    );
    Ok(config)
}

fn append_line(config: &mut String, name: &str, value: &str) {
    config.push_str(name);
    config.push_str(" = ");
    config.push_str(value);
    config.push('\n');
}

fn validate_key(value: &str, label: &str) -> Result<(), String> {
    let decoded = STANDARD
        .decode(value.trim())
        .map_err(|_| format!("AWG2 profile has an invalid {label}"))?;
    if decoded.len() != 32 || value.contains(char::is_whitespace) {
        return Err(format!("AWG2 profile has an invalid {label}"));
    }
    Ok(())
}

fn validate_host(host: &str) -> Result<(), String> {
    let host = host.trim();
    if host.is_empty()
        || host.len() > 253
        || host
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '[' || c == ']')
    {
        return Err("AWG2 profile has an invalid endpoint".into());
    }
    Ok(())
}

fn validate_cidrs(values: &[String], label: &str) -> Result<(), String> {
    if values.is_empty() {
        return Err(format!("AWG2 profile is missing {label}"));
    }
    for value in values {
        let Some((address, prefix)) = value.split_once('/') else {
            return Err(format!("AWG2 profile has an invalid {label}"));
        };
        let address = address
            .parse::<IpAddr>()
            .map_err(|_| format!("AWG2 profile has an invalid {label}"))?;
        let prefix = prefix
            .parse::<u8>()
            .map_err(|_| format!("AWG2 profile has an invalid {label}"))?;
        if prefix > if address.is_ipv4() { 32 } else { 128 } {
            return Err(format!("AWG2 profile has an invalid {label}"));
        }
    }
    Ok(())
}

fn validate_ips(values: &[String], label: &str) -> Result<(), String> {
    if values.is_empty() {
        return Err(format!("AWG2 profile is missing {label}"));
    }
    if values.iter().any(|value| value.parse::<IpAddr>().is_err()) {
        return Err(format!("AWG2 profile has an invalid {label}"));
    }
    Ok(())
}

fn validate_obfuscation(obfs: &Awg2Obfuscation, mtu: u16) -> Result<(), String> {
    if !(1..=10).contains(&obfs.jc)
        || !(64..=1024).contains(&obfs.jmin)
        || !(obfs.jmin..=1024).contains(&obfs.jmax)
        || obfs.jmax >= mtu
    {
        return Err("AWG2 profile has invalid junk-packet settings".into());
    }
    if !(1..=64).contains(&obfs.s1)
        || !(1..=64).contains(&obfs.s2)
        || !(1..=64).contains(&obfs.s3)
        || !(1..=32).contains(&obfs.s4)
    {
        return Err("AWG2 profile has invalid packet-padding settings".into());
    }
    let mut ranges = Vec::with_capacity(4);
    for value in [&obfs.h1, &obfs.h2, &obfs.h3, &obfs.h4] {
        let range = parse_magic_range(value)?;
        if ranges
            .iter()
            .any(|(start, end)| range.0 <= *end && *start <= range.1)
        {
            return Err("AWG2 profile has overlapping magic-number ranges".into());
        }
        ranges.push(range);
    }
    for value in [&obfs.i1, &obfs.i2, &obfs.i3, &obfs.i4, &obfs.i5]
        .into_iter()
        .flatten()
    {
        if value.is_empty()
            || value.len() > 2048
            || value.chars().any(char::is_control)
            || !value.starts_with('<')
            || !value.ends_with('>')
        {
            return Err("AWG2 profile has an invalid masking packet".into());
        }
    }
    Ok(())
}

fn parse_magic_range(value: &str) -> Result<(u32, u32), String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(char::is_whitespace) {
        return Err("AWG2 profile has an invalid magic-number range".into());
    }
    let (start, end) = value.split_once('-').unwrap_or((value, value));
    let start = start
        .parse::<u32>()
        .map_err(|_| "AWG2 profile has an invalid magic-number range".to_string())?;
    let end = end
        .parse::<u32>()
        .map_err(|_| "AWG2 profile has an invalid magic-number range".to_string())?;
    if start == 0 || end == 0 || start > end {
        return Err("AWG2 profile has an invalid magic-number range".into());
    }
    Ok((start, end))
}

fn format_endpoint(address: &str, port: u16) -> String {
    if address.parse::<std::net::Ipv6Addr>().is_ok() {
        format!("[{address}]:{port}")
    } else {
        format!("{address}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(byte: u8) -> String {
        STANDARD.encode([byte; 32])
    }

    fn valid_obfs() -> Awg2Obfuscation {
        Awg2Obfuscation {
            jc: 4,
            jmin: 64,
            jmax: 1024,
            s1: 16,
            s2: 16,
            s3: 16,
            s4: 8,
            h1: "1-10".into(),
            h2: "11-20".into(),
            h3: "21-30".into(),
            h4: "31-40".into(),
            i1: Some("<b 0x1234>".into()),
            ..Default::default()
        }
    }

    #[test]
    fn renders_full_awg2_profile() {
        let obfs = valid_obfs();
        let config = render_wg_quick(Awg2Profile {
            server_address: "vpn.example.test",
            server_port: 51820,
            private_key: &key(1),
            peer_public_key: &key(2),
            pre_shared_key: Some(&key(3)),
            addresses: &["10.0.0.2/32".into(), "fd00::2/128".into()],
            dns_servers: &["1.1.1.1".into(), "2606:4700:4700::1111".into()],
            allowed_ips: &["0.0.0.0/0".into(), "::/0".into()],
            mtu: 1280,
            persistent_keepalive_secs: 25,
            obfs: &obfs,
        })
        .expect("valid profile");

        for field in ["Jc = 4", "S4 = 8", "H4 = 31-40", "I1 = <b 0x1234>"] {
            assert!(config.contains(field), "missing {field}");
        }
        assert!(config.contains("MTU = 1280"));
    }

    #[test]
    fn rejects_fragmenting_junk_and_overlapping_magic_ranges() {
        let mut obfs = valid_obfs();
        obfs.jmax = 1280;
        assert!(validate_obfuscation(&obfs, 1280).is_err());

        let mut obfs = valid_obfs();
        obfs.h4 = "30-40".into();
        assert!(validate_obfuscation(&obfs, 1280).is_err());
    }
}
