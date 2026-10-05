#![allow(
    clippy::unwrap_used,
    reason = "Test fixtures fail fast without logging secrets."
)]

#[test]
fn json_errors_do_not_echo_bad_configuration_or_arguments() {
    let marker = format!("private-{}", std::process::id());
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_brews"))
        .args(["accounts", "list", "--json"])
        .env("BREWS_API_ORIGIN", &marker)
        .env("BREWS_DEV_CLI_KEY", &marker)
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!String::from_utf8_lossy(&result.stderr).contains(&marker));
    let error: serde_json::Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(error["error"]["code"], "configuration");
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_brews"))
        .args(["accounts", "get", &marker, "--json"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(!String::from_utf8_lossy(&result.stderr).contains(&marker));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&result.stderr).unwrap()["error"]["code"],
        "invalid_input"
    );
}

#[test]
fn confirmation_is_checked_before_configuration() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_brews"))
        .args(["accounts", "delete", "01890f3e-53b7-7d28-9b05-4f65092d5711"])
        .env_remove("BREWS_API_ORIGIN")
        .env_remove("BREWS_DEV_CLI_KEY")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--yes"));
}

#[test]
fn binary_help_is_available_without_credentials() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_brews"))
        .arg("--help")
        .env_remove("BREWS_API_ORIGIN")
        .env_remove("BREWS_DEV_CLI_KEY")
        .output()
        .unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    assert!(output.contains("accounts") && output.contains("audit") && output.contains("--json"));
}
