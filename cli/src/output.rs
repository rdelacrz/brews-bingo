//! Allocation-conscious terminal metadata rendering.

use crate::error::CliError;
use brews_contracts::management::ManagementResponse;
use std::io::Write;

pub(crate) fn write_error(
    sink: &mut impl Write,
    error: CliError,
    json: bool,
) -> Result<(), CliError> {
    write_metadata(
        sink,
        &serde_json::json!({ "error": { "code": error.code(), "message": error.to_string() } }),
        json,
    )
}

pub(crate) fn write_notice(
    sink: &mut impl Write,
    command_id: brews_domain::ids::CommandId,
    json: bool,
) -> Result<(), CliError> {
    write_metadata(
        sink,
        &serde_json::json!({ "event": "command", "command_id": command_id, "guidance": "Reuse this exact command ID for an explicit uncertain-outcome retry. Never automatically retry. On failure or replay the reserved link file may remain empty; use a NEW target for an explicit retry." }),
        json,
    )
}

pub(crate) fn write_response(
    sink: &mut impl Write,
    response: &ManagementResponse,
    json: bool,
) -> Result<(), CliError> {
    if !json {
        match response {
            ManagementResponse::Account { account } => writeln!(
                sink,
                "Account {} username=\"{}\"",
                account.account_id,
                terminal_text(&account.username)
            )
            .map_err(|_| CliError::Output)?,
            ManagementResponse::Accounts { accounts, .. } => {
                writeln!(sink, "{} accounts", accounts.len()).map_err(|_| CliError::Output)?
            }
            ManagementResponse::Audit { events, .. } => {
                writeln!(sink, "{} audit events", events.len()).map_err(|_| CliError::Output)?
            }
            ManagementResponse::Pending { .. } => {
                writeln!(sink, "Removal is pending, NOT complete.").map_err(|_| CliError::Output)?
            }
            _ => {}
        }
    }
    let metadata = match response {
        ManagementResponse::Issued { receipt, .. } => {
            serde_json::json!({"result": "issued", "receipt": receipt, "link_written": true})
        }
        ManagementResponse::Committed { receipt } => {
            serde_json::json!({"result": "committed", "receipt": receipt, "link_written": false, "guidance": replay_guidance(receipt.operation)})
        }
        _ => serde_json::to_value(response).map_err(|_| CliError::Output)?,
    };
    write_metadata(sink, &metadata, json)
}
fn replay_guidance(operation: brews_contracts::management::ReceiptOperation) -> &'static str {
    use brews_contracts::management::ReceiptOperation;
    match operation {
        ReceiptOperation::CreateAccount | ReceiptOperation::ReissueEnrollment => {
            "No URL is available in a receipt replay. Explicitly run accounts reissue-enrollment with a new command ID and NEW --link-output file."
        }
        ReceiptOperation::ResetPassword => {
            "No URL is available in a receipt replay. Explicitly run accounts reset-password with a new command ID and NEW --link-output file."
        }
        _ => "Committed; no link was issued.",
    }
}
fn write_metadata(
    sink: &mut impl Write,
    metadata: &serde_json::Value,
    json: bool,
) -> Result<(), CliError> {
    let text = if json {
        serde_json::to_string(metadata)
    } else {
        serde_json::to_string_pretty(metadata)
    }
    .map_err(|_| CliError::Output)?;
    let mut safe_text = String::with_capacity(text.len());
    for c in text.chars() {
        if (c.is_control() && c != '\n')
            || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            use std::fmt::Write as _;
            write!(&mut safe_text, "\\u{:04x}", u32::from(c)).map_err(|_| CliError::Output)?;
        } else {
            safe_text.push(c);
        }
    }
    sink.write_all(safe_text.as_bytes())
        .and_then(|()| sink.write_all(b"\n"))
        .and_then(|()| sink.flush())
        .map_err(|_| CliError::Output)
}

fn terminal_text(value: &str) -> String {
    value.chars().flat_map(char::escape_default).collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "Test fixtures fail fast without logging secrets."
)]
mod tests {
    use super::*;

    #[test]
    fn human_and_json_account_output_escape_all_controls() {
        let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
        let response: ManagementResponse = serde_json::from_value(serde_json::json!({"result":"account", "account":{"account_id":id,"username":"user\u{1b}[31m\u{0}\u{7f}","role":"Host","status":"Verified","disabled":false,"created_at":1}})).unwrap();
        for json in [true, false] {
            let mut out = Vec::new();
            write_response(&mut out, &response, json).unwrap();
            let text = String::from_utf8(out).unwrap();
            assert!(!text.chars().any(|c| c.is_control() && c != '\n'));
            if json {
                assert!(serde_json::from_str::<serde_json::Value>(&text).is_ok());
            }
        }
    }

    #[test]
    fn issued_output_projects_metadata_without_link_secrets() {
        let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
        let url = crate::fixtures::enrollment_url();
        let response: ManagementResponse = serde_json::from_value(serde_json::json!({ "result": "issued", "url": &*url, "receipt": { "version": 1, "command_id": id, "operation": "create_account", "account_id": id, "link_id": id, "purpose": "Enrollment", "link_expires_at": 2, "completed_at": 1, "expires_at": 3 } })).unwrap();
        for json in [true, false] {
            let mut out = Vec::new();
            write_response(&mut out, &response, json).unwrap();
            let text = String::from_utf8(out).unwrap();
            assert!(text.contains("issued"));
            assert!(!text.contains(&*url));
            assert!(!text.contains("example.test"));
            if json {
                assert!(
                    serde_json::from_str::<serde_json::Value>(&text).unwrap()["link_written"]
                        == true
                );
            }
        }
    }

    #[test]
    fn terminal_text_escapes_username_controls() {
        let output = terminal_text("user\u{1b}[31m\u{0}\r\n\u{7f}");
        assert!(!output.chars().any(char::is_control));
        assert!(output.contains("\\u{1b}"));
    }
}
