use crate::{error::CliError, operations::Prepared};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use brews_config::env::cli::CliConfig;
use brews_contracts::management::ManagementResponse;
use reqwest::{
    blocking::Client,
    header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue},
};
use std::io::Read;
use zeroize::Zeroizing;

const MAX_RESPONSE_BYTES: u64 = 128 * 1024;
const MAX_REQUEST_BYTES: usize = 4 * 1024;
const MAX_CA_PEM_BYTES: u64 = 64 * 1024;
const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub(crate) struct ManagementClient {
    client: Client,
    endpoint: String,
    authorization: HeaderValue,
}
impl ManagementClient {
    pub(crate) fn new(config: &CliConfig) -> Result<Self, CliError> {
        let origin = url::Url::parse(&config.api_origin).map_err(|_| CliError::Configuration)?;
        if origin.scheme() != "https"
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.path() != "/"
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.origin().ascii_serialization() != config.api_origin
        {
            return Err(CliError::Configuration);
        }
        let mut builder = Client::builder()
            .https_only(true)
            .http1_only()
            .no_proxy()
            .retry(reqwest::retry::never())
            .timeout(HTTP_TIMEOUT)
            .connect_timeout(HTTP_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none());
        if let Some(path) = &config.tls_ca_file {
            let mut pem = Vec::new();
            std::fs::File::open(path)
                .map_err(|_| CliError::Configuration)?
                .take(MAX_CA_PEM_BYTES + 1)
                .read_to_end(&mut pem)
                .map_err(|_| CliError::Configuration)?;
            if pem.len() as u64 > MAX_CA_PEM_BYTES {
                return Err(CliError::Configuration);
            }
            let certificates =
                reqwest::Certificate::from_pem_bundle(&pem).map_err(|_| CliError::Configuration)?;
            if certificates.is_empty() {
                return Err(CliError::Configuration);
            }
            for certificate in certificates {
                builder = builder.add_root_certificate(certificate);
            }
        }
        let client = builder.build().map_err(|_| CliError::Configuration)?;
        let encoded = Zeroizing::new(URL_SAFE_NO_PAD.encode(config.dev_cli_key.expose_secret()));
        let mut bearer = Zeroizing::new(String::from("Bearer "));
        bearer.push_str(&encoded);
        let mut authorization =
            HeaderValue::from_str(&bearer).map_err(|_| CliError::Configuration)?;
        authorization.set_sensitive(true);
        Ok(Self {
            client,
            endpoint: format!("{}/_dev/commands", config.api_origin),
            authorization,
        })
    }
    pub(crate) fn execute(&self, prepared: &Prepared) -> Result<ManagementResponse, CliError> {
        if prepared.command.is_mutating() != prepared.command_id.is_some() {
            return Err(CliError::InvalidInput);
        }
        let payload = serde_json::to_vec(&prepared.command).map_err(|_| CliError::InvalidInput)?;
        if payload.len() > MAX_REQUEST_BYTES {
            return Err(CliError::InvalidInput);
        }
        let request = self
            .client
            .post(&self.endpoint)
            .header(AUTHORIZATION, self.authorization.clone())
            .header(CONTENT_TYPE, "application/json")
            .body(payload);
        let request = if let Some(command_id) = prepared.command_id {
            request.header("Idempotency-Key", command_id.to_string())
        } else {
            request
        };
        let response = request.send().map_err(|_| CliError::Transport)?;
        let status = response.status().as_u16();
        if status != 200 && status != 202 {
            return Err(CliError::HttpStatus(status));
        }
        if response.headers().contains_key(reqwest::header::SET_COOKIE) {
            return Err(CliError::Protocol);
        }
        let mut content_types = response.headers().get_all(CONTENT_TYPE).iter();
        let content_type = content_types
            .next()
            .and_then(|value| value.to_str().ok())
            .ok_or(CliError::Protocol)?;
        if content_types.next().is_some()
            || !content_type
                .split(';')
                .next()
                .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"))
        {
            return Err(CliError::Protocol);
        }
        let mut body = Zeroizing::new(Vec::new());
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut body)
            .map_err(|_| CliError::Transport)?;
        if body.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(CliError::ResponseTooLarge);
        }
        let mut decoded = ManagementResponse::decode_json(&body).map_err(|_| CliError::Protocol)?;
        let validation = if (status == 202) != matches!(decoded, ManagementResponse::Pending { .. })
        {
            Err(CliError::Protocol)
        } else {
            crate::protocol::validate_response(prepared, &decoded)
        };
        if let Err(error) = validation {
            if let ManagementResponse::Issued { url, .. } = &mut decoded {
                use zeroize::Zeroize;
                url.zeroize();
            }
            return Err(error);
        }
        Ok(decoded)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Test fixtures fail fast without logging secrets."
)]
#[path = "../tests/client/mod.rs"]
mod tests;
