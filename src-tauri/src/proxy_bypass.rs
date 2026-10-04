//! Normalize host patterns and native libcurl host/IP/CIDR bypass rules.
use crate::error::AppError;
use serde_json::Value;

pub fn normalize(input: &str) -> Result<String, AppError> {
    let mut entries = Vec::new();
    for entry in input
        .split([',', ';', '\n', '\r'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let value = if entry == "*" {
            entry.to_owned()
        } else if let Ok(network) = entry.parse::<ipnet::IpNet>() {
            network.trunc().to_string()
        } else if let Ok(address) = entry
            .trim_start_matches('[')
            .trim_end_matches(']')
            .parse::<std::net::IpAddr>()
        {
            address.to_string()
        } else if let Some(network) = ipv4_wildcard(entry) {
            network
        } else if entry.contains('*')
            && entry.len() <= 253
            && entry.bytes().any(|ch| ch.is_ascii_alphabetic())
            && entry
                .bytes()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'.' | b'*'))
        {
            entry.to_ascii_lowercase()
        } else if let Ok(url::Host::Domain(domain)) =
            url::Host::parse(entry.trim_start_matches('.'))
        {
            if domain.is_empty()
                || !domain
                    .bytes()
                    .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'.'))
            {
                return Err(invalid(entry));
            }
            domain
        } else {
            return Err(invalid(entry));
        };
        if !entries.contains(&value) {
            entries.push(value);
        }
    }
    Ok(entries.join(","))
}

fn ipv4_wildcard(entry: &str) -> Option<String> {
    let parts: Vec<_> = entry.split('.').collect();
    let prefix = parts.iter().position(|part| *part == "*")?;
    if prefix == 0
        || prefix > 3
        || parts.len() > 4
        || parts[prefix..].iter().any(|part| *part != "*")
    {
        return None;
    }
    let mut octets = [0; 4];
    for (index, part) in parts[..prefix].iter().enumerate() {
        octets[index] = part.parse::<u8>().ok()?;
    }
    ipnet::Ipv4Net::new(std::net::Ipv4Addr::from(octets), (prefix * 8) as u8)
        .ok()
        .map(|network| network.to_string())
}

/// Import supported bypass rules; report unsupported OS expressions.
pub fn import_system(input: &str) -> (String, Vec<String>) {
    let mut accepted = Vec::new();
    let mut unsupported = Vec::new();
    for entry in input
        .split([',', ';', '\n', '\r'])
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
    {
        match normalize(entry) {
            Ok(value) if !accepted.contains(&value) => accepted.push(value),
            Err(_) if !unsupported.iter().any(|value| value == entry) => {
                unsupported.push(entry.to_string())
            }
            _ => {}
        }
    }
    (accepted.join(","), unsupported)
}

fn invalid(entry: &str) -> AppError {
    AppError::InvalidInput(format!(
        "Unsupported proxy bypass entry: {entry}. Use a host, domain pattern, IP, CIDR network, or *; separate entries with newlines or commas."
    ))
}

pub fn normalize_options(options: &mut serde_json::Map<String, Value>) -> Result<(), AppError> {
    if ["all-proxy", "http-proxy", "https-proxy"]
        .iter()
        .all(|key| options.get(*key).and_then(Value::as_str) == Some(""))
    {
        options.insert("no-proxy".into(), "".into());
        return Ok(());
    }
    if let Some(value) = options.get_mut("no-proxy") {
        let input = value
            .as_str()
            .ok_or_else(|| AppError::InvalidInput("no-proxy must be text".into()))?;
        *value = normalize(input)?.into();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_multiline_and_system_lists_for_curl() {
        assert_eq!(
            normalize(" 10.0.0.1\r\n10.0.0.2; localhost,10.0.0.1 ").unwrap(),
            "10.0.0.1,10.0.0.2,localhost"
        );
        assert_eq!(
            normalize(".EXAMPLE.com\n[::1]\n10.0.0.7/8").unwrap(),
            "example.com,::1,10.0.0.0/8"
        );
        assert_eq!(normalize("*\n*").unwrap(), "*");
        assert_eq!(normalize("\n , ; ").unwrap(), "");
        assert_eq!(
            normalize("127.*;10.*;192.168.*").unwrap(),
            "127.0.0.0/8,10.0.0.0/8,192.168.0.0/16"
        );
        assert_eq!(
            import_system("<local>;127.*;*.local"),
            ("127.0.0.0/8,*.local".into(), vec!["<local>".into()])
        );
    }

    #[test]
    fn rejects_invalid_bypass_rules() {
        assert_eq!(
            normalize("*.EXAMPLE.com;dlinks.*").unwrap(),
            "*.example.com,dlinks.*"
        );
        for value in [
            "<local>",
            "127.*.1",
            "https://example.com",
            "host:8080",
            "10.0.0.0/99",
            "a=b",
            "bad host",
        ] {
            assert!(normalize(value).is_err(), "{value}");
        }
    }
}
