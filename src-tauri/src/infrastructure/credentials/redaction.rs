use crate::providers::credentials::CredentialError;
use secrecy::{ExposeSecret, SecretString};

pub fn sensitive_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "proxy-authorization"
            | "x-api-key"
            | "api-key"
            | "api_key"
            | "apikey"
            | "key"
            | "token"
            | "access_token"
            | "secret"
            | "signature"
            | "cookie"
            | "set-cookie"
    )
}
pub fn redacted_header(name: &str, value: &str) -> String {
    if sensitive_name(name) {
        "[REDACTED]".into()
    } else {
        value
            .chars()
            .filter(|c| !c.is_control())
            .take(128)
            .collect()
    }
}
pub fn redacted_url(raw: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(raw) else {
        return "[INVALID URL]".into();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_fragment(None);
    let pairs: Vec<_> = url
        .query_pairs()
        .map(|(k, v)| {
            let value = if sensitive_name(&k) {
                "[REDACTED]".into()
            } else {
                v.into_owned()
            };
            (k.into_owned(), value)
        })
        .collect();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    url.to_string()
}

/// Extract embedded credentials before persisting an endpoint. User-info is
/// rejected rather than guessed into an unrelated provider authentication scheme.
pub fn split_endpoint(raw: &str) -> Result<(String, Vec<SecretString>), CredentialError> {
    let mut url = reqwest::Url::parse(raw).map_err(|_| CredentialError::InvalidInput)?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(CredentialError::InvalidInput);
    }
    let mut values = Vec::new();
    let mut pairs = Vec::new();
    for (key, value) in url.query_pairs() {
        if sensitive_name(&key) {
            if !value.trim().is_empty() {
                values.push(SecretString::new(value.into_owned()));
            }
        } else {
            pairs.push((key.into_owned(), value.into_owned()));
        }
    }
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    Ok((url.to_string().trim_end_matches('/').to_owned(), values))
}
pub fn redact_known(raw: &str, values: &[SecretString]) -> String {
    use zeroize::{Zeroize, Zeroizing};
    let mut text = Zeroizing::new(raw.to_owned());
    for value in values {
        if !value.expose_secret().is_empty() {
            let clean = text.replace(value.expose_secret(), "[REDACTED]");
            text.zeroize();
            *text = clean;
        }
    }
    std::mem::take(&mut *text)
}
pub fn redact_json(value: &mut serde_json::Value, secrets: &[SecretString]) {
    match value {
        serde_json::Value::String(text) => {
            use zeroize::Zeroize;
            let clean = redact_known(text, secrets);
            text.zeroize();
            *text = clean;
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact_json(value, secrets);
            }
        }
        serde_json::Value::Object(values) => {
            use zeroize::Zeroize;
            let original = std::mem::take(values);
            for (mut key, mut value) in original {
                let clean_key = redact_known(&key, secrets);
                key.zeroize();
                redact_json(&mut value, secrets);
                let mut unique_key = clean_key.clone();
                let mut index = 1;
                while values.contains_key(&unique_key) {
                    unique_key = format!("{clean_key}_{index}");
                    index += 1;
                }
                values.insert(unique_key, value);
            }
        }
        _ => {}
    }
}
