#![cfg(feature = "management")]
#![allow(clippy::unwrap_used, reason = "Test fixtures fail fast.")]
use brews_contracts::management::ManagementCommand;

#[test]
fn management_commands_are_closed_and_have_no_client_actor() {
    let command: ManagementCommand = serde_json::from_str(
        r#"{"operation":"create_account","username":"HostPerson01","role":"host"}"#,
    )
    .unwrap();
    assert!(command.is_mutating());
    assert!(serde_json::from_str::<ManagementCommand>(r#"{"operation":"create_account","username":"HostPerson01","role":"host","actor":"developer_cli"}"#).is_err());
}

#[test]
fn receipts_reject_unrecognized_versions() {
    use brews_contracts::management::ManagementReceipt;
    let value = serde_json::json!({"version":2,"command_id":"01890f3e-53b7-7d28-9b05-4f65092d5711","operation":"enable_account","account_id":"01890f3e-53b7-7d28-9b05-4f65092d5712","link_id":null,"purpose":null,"link_expires_at":null,"completed_at":0,"expires_at":86400000});
    assert!(serde_json::from_value::<ManagementReceipt>(value).is_err());
}
