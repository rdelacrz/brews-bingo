//! Strict configured HTTPS origin validation.
use super::super::{ConfigError, InvalidValueKind};
use url::Url;

pub(in crate::env) fn validate_origin(
    value: &str,
    key: &'static str,
    max_bytes: usize,
) -> Result<String, ConfigError> {
    if value.len() > max_bytes {
        return Err(ConfigError::InvalidValue {
            key,
            kind: InvalidValueKind::TooLong,
        });
    }
    let invalid = ConfigError::InvalidValue {
        key,
        kind: InvalidValueKind::Origin,
    };
    let url = Url::parse(value).map_err(|_| invalid)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid);
    }
    let origin = url.origin().ascii_serialization();
    if value != origin && value.strip_suffix('/') != Some(origin.as_str()) {
        return Err(invalid);
    }
    Ok(origin)
}
