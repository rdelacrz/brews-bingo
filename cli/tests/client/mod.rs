use super::*;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use clap::Parser;
use rcgen::generate_simple_self_signed;
use rustls::{ServerConfig, ServerConnection, StreamOwned, pki_types::PrivatePkcs8KeyDer};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

struct Request {
    headers: Zeroizing<String>,
    body: Zeroizing<Vec<u8>>,
}
struct Server {
    config: CliConfig,
    captured: mpsc::Receiver<Option<Request>>,
    thread: Option<thread::JoinHandle<usize>>,
    _dir: tempfile::TempDir,
}
impl Server {
    fn start(status: u16, headers: &str, body: Vec<u8>, delay: Duration) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let certified =
            generate_simple_self_signed(vec!["localhost".into(), "127.0.0.1".into()]).unwrap();
        let ca = dir.path().join("ca.pem");
        std::fs::write(&ca, certified.cert.pem()).unwrap();
        let key = PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der());
        let tls = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(vec![certified.cert.der().clone()], key.into())
                .unwrap(),
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!(
            "https://localhost:{}",
            listener.local_addr().unwrap().port()
        );
        let mut secret = Zeroizing::new(Vec::with_capacity(32));
        secret.extend_from_slice(uuid::Uuid::now_v7().as_bytes());
        secret.extend_from_slice(uuid::Uuid::now_v7().as_bytes());
        let config: CliConfig = serde_json::from_value(serde_json::json!({
            "brews_api_origin": origin,
            "brews_dev_cli_key": URL_SAFE_NO_PAD.encode(&*secret),
            "brews_tls_ca_file": ca,
        }))
        .unwrap();
        let headers = headers.to_owned();
        let body = Zeroizing::new(body);
        let (tx, captured) = mpsc::channel();
        let handle = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(5);
            let tcp = loop {
                match listener.accept() {
                    Ok((tcp, _)) => break tcp,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => {
                        let _ = tx.send(None);
                        return 0;
                    }
                }
            };
            tcp.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            tcp.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut stream = StreamOwned::new(ServerConnection::new(tls).unwrap(), tcp);
            let mut bytes = Zeroizing::new(Vec::new());
            let mut one = [0u8; 1];
            while bytes.len() < 8192 && !bytes.ends_with(b"\r\n\r\n") {
                if stream.read_exact(&mut one).is_err() {
                    let _ = tx.send(None);
                    return 1;
                }
                bytes.push(one[0]);
            }
            let headers_text = Zeroizing::new(String::from_utf8(bytes.to_vec()).unwrap());
            let length: usize = headers_text
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|n| n.parse().ok())
                })
                .unwrap_or(0);
            let mut request_body = Zeroizing::new(vec![0; length]);
            if stream.read_exact(&mut request_body).is_err() {
                let _ = tx.send(None);
                return 1;
            }
            let _ = tx.send(Some(Request {
                headers: headers_text,
                body: request_body,
            }));
            thread::sleep(delay);
            let content_type = if headers.to_ascii_lowercase().contains("content-type:") {
                ""
            } else {
                "Content-Type: application/json\r\n"
            };
            let head = format!(
                "HTTP/1.1 {status} Test\r\n{content_type}Content-Length: {}\r\nConnection: close\r\n{headers}\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
            let _ = stream.flush();
            drop(stream);
            let mut connections = 1;
            let deadline = Instant::now() + Duration::from_millis(100);
            while Instant::now() < deadline {
                if listener.accept().is_ok() {
                    connections += 1;
                }
                thread::sleep(Duration::from_millis(2));
            }
            connections
        });
        Self {
            config,
            captured,
            thread: Some(handle),
            _dir: dir,
        }
    }
    fn request(&self) -> Request {
        self.captured
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap()
    }
    fn connections(mut self) -> usize {
        self.thread.take().unwrap().join().unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}
fn list() -> Prepared {
    crate::args::Cli::try_parse_from(["brews", "accounts", "list"])
        .unwrap()
        .prepare()
        .unwrap()
}
#[test]
fn audit_reads_preserve_required_targets_and_read_operations() {
    use brews_contracts::management::{
        AuditActor, AuditEvent, AuditOperation, AuditOutcome, AuditTarget,
    };
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let events = vec![
        AuditEvent {
            audit_id: id.parse().unwrap(),
            actor: AuditActor::DeveloperCli,
            operation: AuditOperation::ListAccounts,
            target: AuditTarget::AccountsOwner,
            outcome: AuditOutcome::Succeeded,
            occurred_at: 1,
            expires_at: 2,
        },
        AuditEvent {
            audit_id: "01890f3e-53b7-7d28-9b05-4f65092d5712".parse().unwrap(),
            actor: AuditActor::DeveloperCli,
            operation: AuditOperation::GetAccount,
            target: AuditTarget::Account(id.parse().unwrap()),
            outcome: AuditOutcome::Rejected,
            occurred_at: 1,
            expires_at: 2,
        },
    ];
    for json in [true, false] {
        let body = serde_json::to_vec(&ManagementResponse::Audit {
            events: events.clone(),
            next_cursor: Some(id.parse().unwrap()),
        })
        .unwrap();
        let server = Server::start(200, "", body, Duration::ZERO);
        let args = if json {
            vec![
                "brews", "audit", "list", "--after", id, "--limit", "2", "--json",
            ]
        } else {
            vec!["brews", "audit", "list", "--after", id, "--limit", "2"]
        };
        let mut out = Vec::new();
        let mut notice = Vec::new();
        crate::operations::run(
            &server.config,
            crate::args::Cli::try_parse_from(args).unwrap(),
            &mut out,
            &mut notice,
        )
        .unwrap();
        assert!(notice.is_empty());
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("accounts_owner")
                && text.contains("get_account")
                && text.contains("list_accounts")
        );
        if json {
            let metadata: serde_json::Value = serde_json::from_str(&text).unwrap();
            assert_eq!(metadata["events"][1]["target"]["account"], id);
            assert!(metadata["events"][0].get("target_account_id").is_none());
        }
        let request = server.request();
        assert!(
            !request
                .headers
                .to_ascii_lowercase()
                .contains("idempotency-key:")
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&request.body).unwrap()["after"],
            id
        );
        assert_eq!(server.connections(), 1);
    }
}

#[test]
fn unknown_ca_stops_before_any_authenticated_http_request() {
    let mut server = Server::start(200, "", vec![], Duration::ZERO);
    server.config.tls_ca_file = None;
    let result = ManagementClient::new(&server.config)
        .unwrap()
        .execute(&list());
    assert!(matches!(result, Err(CliError::Transport)));
    assert!(
        server
            .captured
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .is_none()
    );
    assert_eq!(server.connections(), 1);
}
#[test]
fn authorization_debug_is_redacted_and_server_errors_discard_bodies() {
    let private = crate::fixtures::enrollment_url();
    let server = Server::start(403, "", private.as_bytes().to_vec(), Duration::ZERO);
    let client = ManagementClient::new(&server.config).unwrap();
    assert!(client.authorization.is_sensitive());
    let encoded = Zeroizing::new(URL_SAFE_NO_PAD.encode(server.config.dev_cli_key.expose_secret()));
    assert!(!format!("{:?}", client.authorization).contains(&*encoded));
    let error = client.execute(&list()).unwrap_err();
    assert_eq!(error, CliError::HttpStatus(403));
    assert!(!format!("{error} {error:?}").contains(&*private));
    assert!(std::error::Error::source(&error).is_none());
    assert_eq!(server.connections(), 1);
}
#[test]
fn existing_output_target_prevents_any_http_mutation() {
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let server = Server::start(
        200,
        "",
        link_response(id, &crate::fixtures::enrollment_url(), true),
        Duration::ZERO,
    );
    let path = server._dir.path().join("existing");
    std::fs::write(&path, b"existing").unwrap();
    let cli = crate::args::Cli::try_parse_from([
        "brews",
        "accounts",
        "create",
        "--role",
        "host",
        "--username",
        "hostusername",
        "--link-output",
        path.to_str().unwrap(),
    ])
    .unwrap();
    assert!(matches!(
        crate::operations::run(&server.config, cli, &mut Vec::new(), &mut Vec::new()),
        Err(CliError::LinkFile)
    ));
    assert!(std::fs::read(&path).unwrap() == b"existing");
    assert_eq!(server.connections(), 0);
}
#[test]
fn stdout_failure_after_issuance_keeps_durable_private_link() {
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let url = crate::fixtures::enrollment_url();
    let server = Server::start(200, "", link_response(id, &url, true), Duration::ZERO);
    let path = server._dir.path().join("link");
    let cli = crate::args::Cli::try_parse_from([
        "brews",
        "accounts",
        "create",
        "--role",
        "host",
        "--username",
        "hostusername",
        "--command-id",
        id,
        "--link-output",
        path.to_str().unwrap(),
        "--json",
    ])
    .unwrap();
    assert!(matches!(
        crate::operations::run(&server.config, cli, &mut BrokenPipe, &mut Vec::new()),
        Err(CliError::Output)
    ));
    assert!(std::fs::read_to_string(&path).unwrap() == format!("{}\n", *url));
    assert_eq!(server.connections(), 1);
}

#[test]
fn read_account_controls_are_preserved_in_json_but_never_execute_in_terminal() {
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let username = "name\u{0}\u{0b}\u{1b}[31m\u{7f}";
    for json in [false, true] {
        let body = serde_json::to_vec(&serde_json::json!({"result":"account", "account":{"account_id":id,"username":username,"role":"Host","status":"Verified","disabled":false,"created_at":1}})).unwrap();
        let server = Server::start(200, "", body, Duration::ZERO);
        let args = if json {
            vec!["brews", "accounts", "get", id, "--json"]
        } else {
            vec!["brews", "accounts", "get", id]
        };
        let mut out = Vec::new();
        crate::operations::run(
            &server.config,
            crate::args::Cli::try_parse_from(args).unwrap(),
            &mut out,
            &mut Vec::new(),
        )
        .unwrap();
        assert!(
            !String::from_utf8_lossy(&out)
                .chars()
                .any(|c| c.is_control() && c != '\n')
        );
        if json {
            assert!(
                serde_json::from_slice::<serde_json::Value>(&out).unwrap()["account"]["username"]
                    == username
            );
        }
        assert_eq!(server.connections(), 1);
    }
}

#[test]
fn response_content_type_must_be_single_application_json() {
    for headers in [
        "Content-Type: text/plain\r\n",
        "Content-Type: application/json\r\nContent-Type: application/json\r\n",
    ] {
        let server = Server::start(
            200,
            headers,
            br#"{"result":"accounts","accounts":[],"next_cursor":null}"#.to_vec(),
            Duration::ZERO,
        );
        assert!(matches!(
            ManagementClient::new(&server.config)
                .unwrap()
                .execute(&list()),
            Err(CliError::Protocol)
        ));
        assert_eq!(server.connections(), 1);
    }
}

#[test]
fn ca_file_read_is_bounded_and_errors_are_redacted() {
    let server = Server::start(200, "", vec![], Duration::ZERO);
    let ca = server.config.tls_ca_file.as_ref().unwrap();
    let mut pem = std::fs::read(ca).unwrap();
    pem.resize(64 * 1024 + 1, b' ');
    std::fs::write(ca, &pem).unwrap();
    assert!(matches!(
        ManagementClient::new(&server.config),
        Err(CliError::Configuration)
    ));
    std::fs::write(ca, format!("malformed-{}", uuid::Uuid::now_v7())).unwrap();
    assert!(matches!(
        ManagementClient::new(&server.config),
        Err(CliError::Configuration)
    ));
    assert_eq!(server.connections(), 0);
}

#[test]
fn request_body_and_mutation_identity_are_checked_before_network() {
    let server = Server::start(
        200,
        "",
        br#"{"result":"accounts","accounts":[],"next_cursor":null}"#.to_vec(),
        Duration::ZERO,
    );
    let client = ManagementClient::new(&server.config).unwrap();
    let mut prepared = list();
    prepared.command_id = Some("01890f3e-53b7-7d28-9b05-4f65092d5711".parse().unwrap());
    assert!(matches!(
        client.execute(&prepared),
        Err(CliError::InvalidInput)
    ));
    prepared.command = brews_contracts::management::ManagementCommand::CreateAccount {
        username: "x".repeat(4096),
        role: brews_domain::accounts::AccountRole::Host,
    };
    assert!(matches!(
        client.execute(&prepared),
        Err(CliError::InvalidInput)
    ));
    prepared.command_id = None;
    assert!(matches!(
        client.execute(&prepared),
        Err(CliError::InvalidInput)
    ));
    assert_eq!(server.connections(), 0);
}

fn assert_issued_handoff(operation: &str, url: &str, accepted: bool) {
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let (receipt_operation, purpose) = match operation {
        "create" => ("create_account", "Enrollment"),
        "reissue-enrollment" => ("reissue_enrollment", "Enrollment"),
        "reset-password" => ("reset_password", "PasswordReset"),
        _ => unreachable!(),
    };
    let receipt = serde_json::json!({"version":1,"command_id":id,"operation":receipt_operation,"account_id":id,"link_id":id,"purpose":purpose,"link_expires_at":2,"completed_at":1,"expires_at":3});
    let body =
        serde_json::to_vec(&serde_json::json!({"result":"issued","receipt":receipt,"url":url}))
            .unwrap();
    let server = Server::start(200, "", body, Duration::ZERO);
    // The app handoff origin is intentionally independent from the API origin.
    assert!(!url.starts_with(&server.config.api_origin));
    let path = server._dir.path().join("link");
    let mut args = vec!["brews", "accounts", operation];
    if operation == "create" {
        args.extend(["--role", "host", "--username", "hostusername"]);
    } else {
        args.push(id);
    }
    args.extend([
        "--command-id",
        id,
        "--link-output",
        path.to_str().unwrap(),
        "--json",
    ]);
    let cli = crate::args::Cli::try_parse_from(args).unwrap();
    let mut out = Vec::new();
    let mut notice = Vec::new();
    let result = crate::operations::run(&server.config, cli, &mut out, &mut notice);
    let saved = Zeroizing::new(std::fs::read_to_string(&path).unwrap());
    if accepted {
        assert!(result.is_ok());
        assert!(*saved == format!("{url}\n"));
        assert!(serde_json::from_slice::<serde_json::Value>(&out).unwrap()["link_written"] == true);
    } else {
        assert!(matches!(result, Err(CliError::Protocol)));
        assert!(saved.is_empty());
        assert!(out.is_empty());
    }
    assert!(!String::from_utf8_lossy(&out).contains(url));
    assert!(!String::from_utf8_lossy(&notice).contains(url));
    assert_eq!(server.connections(), 1);
}

#[test]
fn issued_paths_are_bound_to_the_command_before_private_handoff() {
    let token = crate::fixtures::link_token();
    for (operation, correct_path, wrong_path) in [
        ("create", "/enroll", "/password-reset"),
        ("reissue-enrollment", "/enroll", "/password-reset"),
        ("reset-password", "/password-reset", "/enroll"),
    ] {
        let url = Zeroizing::new(format!("https://example.test{correct_path}#{}", *token));
        assert_issued_handoff(operation, &url, true);
        for path in [wrong_path, "/", "/unrelated"] {
            let url = Zeroizing::new(format!("https://example.test{path}#{}", *token));
            assert_issued_handoff(operation, &url, false);
        }
    }
}

#[test]
fn issued_urls_require_exact_canonical_spelling_without_queries() {
    let token = crate::fixtures::link_token();
    for (operation, path) in [
        ("create", "/enroll"),
        ("reissue-enrollment", "/enroll"),
        ("reset-password", "/password-reset"),
    ] {
        for submitted in [
            format!("https://example.test{path}?handoff={}#{}", *token, *token),
            format!("https://example.test{path}?#{}", *token),
            format!("HTTPS://example.test{path}#{}", *token),
            format!("https://EXAMPLE.test{path}#{}", *token),
            format!("https://example.test:443{path}#{}", *token),
            format!("https://example.test/nested/..{path}#{}", *token),
            format!("https:///example.test{path}#{}", *token),
            format!("https://@example.test{path}#{}", *token),
            format!("https://example.test\\{path}#{}", *token),
        ] {
            let submitted = Zeroizing::new(submitted);
            assert_issued_handoff(operation, &submitted, false);
        }
        for origin in ["https://example.test:8443", "https://[::1]:8443"] {
            let url = Zeroizing::new(format!("{origin}{path}#{}", *token));
            assert_issued_handoff(operation, &url, true);
        }
    }
}

#[test]
fn issued_fragments_must_be_canonical_32_byte_base64url() {
    let token = crate::fixtures::link_token();
    let decoded = Zeroizing::new(URL_SAFE_NO_PAD.decode(&*token).unwrap());
    let short = Zeroizing::new(URL_SAFE_NO_PAD.encode(&decoded[..31]));
    let mut long_bytes = Zeroizing::new(decoded.to_vec());
    long_bytes.push(decoded[0]);
    let long = Zeroizing::new(URL_SAFE_NO_PAD.encode(&*long_bytes));
    let mut noncanonical = Zeroizing::new(token.to_string());
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let last = token.as_bytes()[42];
    let index = alphabet.iter().position(|&byte| byte == last).unwrap();
    noncanonical.pop();
    noncanonical.push(char::from(alphabet[index + 1]));
    for (operation, path) in [
        ("create", "/enroll"),
        ("reissue-enrollment", "/enroll"),
        ("reset-password", "/password-reset"),
    ] {
        for submitted in [
            format!("https://example.test{path}"),
            format!("https://example.test{path}#"),
            format!("https://example.test{path}#{}", *short),
            format!("https://example.test{path}#{}", *long),
            format!("https://example.test{path}#{}", uuid::Uuid::now_v7()),
            format!("https://example.test{path}#{}=", *token),
            format!("https://example.test{path}#{}=", *short),
            format!(
                "https://example.test{path}#%{:02X}{}",
                token.as_bytes()[0],
                &token[1..]
            ),
            format!(
                "https://example.test{path}#%{:02X}{}",
                token.as_bytes()[0],
                &token[3..]
            ),
            format!("https://example.test{path}#{}", *noncanonical),
            format!("https://example.test{path}#+{}", &token[1..]),
            format!("https://example.test{path}#/{}", &token[1..]),
            format!("https://example.test{path}#{}", token.repeat(256)),
        ] {
            let submitted = Zeroizing::new(submitted);
            assert_issued_handoff(operation, &submitted, false);
        }
        let url = Zeroizing::new(format!("https://example.test{path}#{}", *token));
        assert_issued_handoff(operation, &url, true);
    }
}

#[test]
fn malformed_link_urls_cannot_be_delivered() {
    let token = crate::fixtures::link_token();
    for (operation, path) in [
        ("create", "/enroll"),
        ("reissue-enrollment", "/enroll"),
        ("reset-password", "/password-reset"),
    ] {
        for submitted in [
            format!("http://example.test{path}#{}", *token),
            format!("https://user:{}@example.test{path}#{}", *token, *token),
            format!("https://{}@example.test{path}#{}", *token, *token),
            format!("https://example.test{path}#{}\u{1b}", *token),
            format!("https://example.test{path}#{} bad", *token),
            format!(" https://example.test{path}#{}", *token),
            format!("https://example.test{path}#{}\t", *token),
            format!("not-a-url-{}", *token),
        ] {
            let submitted = Zeroizing::new(submitted);
            assert_issued_handoff(operation, &submitted, false);
        }
    }
}

#[test]
fn response_sequence_root_is_rejected_over_https() {
    let server = Server::start(200, "", br#"["accounts",[],null]"#.to_vec(), Duration::ZERO);
    assert!(matches!(
        ManagementClient::new(&server.config)
            .unwrap()
            .execute(&list()),
        Err(CliError::Protocol)
    ));
    assert_eq!(server.connections(), 1);
}

#[test]
fn response_shape_and_page_are_bound_to_the_requested_operation() {
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let account = serde_json::json!({"account_id":id,"username":"hostusername","role":"Host","status":"Verified","disabled":false,"created_at":1});
    for body in [
        serde_json::json!({"result":"audit","events":[],"next_cursor":null}),
        serde_json::json!({"result":"accounts","accounts":vec![account; 51],"next_cursor":null}),
        serde_json::json!({"result":"pending","operation_id":id}),
        serde_json::json!({"result":"accounts","accounts":[],"next_cursor":null,"unexpected":"private"}),
        serde_json::json!([{"result":"accounts","accounts":[],"next_cursor":null}]),
    ] {
        let status = if body["result"] == "pending" {
            202
        } else {
            200
        };
        let server = Server::start(
            status,
            "",
            serde_json::to_vec(&body).unwrap(),
            Duration::ZERO,
        );
        assert!(matches!(
            ManagementClient::new(&server.config)
                .unwrap()
                .execute(&list()),
            Err(CliError::Protocol)
        ));
        assert_eq!(server.connections(), 1);
    }
}

#[test]
fn malformed_receipts_are_rejected_before_link_delivery() {
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let different = "01890f3e-53b7-7d28-9b05-4f65092d5712";
    let url = crate::fixtures::enrollment_url();
    for (field, value) in [
        ("version", serde_json::json!(2)),
        ("command_id", serde_json::json!(different)),
        ("operation", serde_json::json!("reset_password")),
        ("link_id", serde_json::Value::Null),
        ("purpose", serde_json::json!("PasswordReset")),
        ("link_expires_at", serde_json::json!(0)),
        ("expires_at", serde_json::json!(0)),
    ] {
        let mut body: serde_json::Value =
            serde_json::from_slice(&link_response(id, &url, true)).unwrap();
        body["receipt"][field] = value;
        let server = Server::start(200, "", serde_json::to_vec(&body).unwrap(), Duration::ZERO);
        let path = server._dir.path().join("link");
        let cli = crate::args::Cli::try_parse_from([
            "brews",
            "accounts",
            "create",
            "--role",
            "host",
            "--username",
            "hostusername",
            "--command-id",
            id,
            "--link-output",
            path.to_str().unwrap(),
        ])
        .unwrap();
        let result = crate::operations::run(&server.config, cli, &mut Vec::new(), &mut Vec::new());
        assert!(matches!(result, Err(CliError::Protocol)));
        assert!(std::fs::read(&path).unwrap().is_empty());
        assert_eq!(server.connections(), 1);
    }
}

fn link_response(id: &str, url: &str, issued: bool) -> Vec<u8> {
    let receipt = serde_json::json!({"version":1,"command_id":id,"operation":"create_account","account_id":id,"link_id":id,"purpose":"Enrollment","link_expires_at":2,"completed_at":1,"expires_at":3});
    serde_json::to_vec(&if issued {
        serde_json::json!({"result":"issued", "receipt":receipt,"url":url})
    } else {
        serde_json::json!({"result":"committed", "receipt":receipt})
    })
    .unwrap()
}
#[test]
fn issued_link_is_private_and_replay_has_no_url() {
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let url = crate::fixtures::enrollment_url();
    for issued in [true, false] {
        let server = Server::start(200, "", link_response(id, &url, issued), Duration::ZERO);
        let path = server._dir.path().join("link");
        let cli = crate::args::Cli::try_parse_from([
            "brews",
            "accounts",
            "create",
            "--role",
            "host",
            "--username",
            "hostusername",
            "--command-id",
            id,
            "--link-output",
            path.to_str().unwrap(),
            "--json",
        ])
        .unwrap();
        let mut out = Vec::new();
        let mut notice = Vec::new();
        crate::operations::run(&server.config, cli, &mut out, &mut notice).unwrap();
        let saved = Zeroizing::new(std::fs::read_to_string(&path).unwrap());
        if issued {
            assert!(*saved == format!("{}\n", *url));
        } else {
            assert!(saved.is_empty());
            assert!(String::from_utf8_lossy(&out).contains("reissue-enrollment"));
        }
        assert!(!String::from_utf8_lossy(&out).contains(&*url));
        assert!(!String::from_utf8_lossy(&notice).contains(&*url));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&notice).unwrap()["command_id"],
            id
        );
        assert_eq!(server.connections(), 1);
    }
}

struct BrokenPipe;
impl Write for BrokenPipe {
    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }
}
#[test]
fn mutation_notice_is_flushed_before_network_and_failure_aborts() {
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let server = Server::start(
        202,
        "",
        format!(r#"{{"result":"pending","operation_id":"{id}"}}"#).into_bytes(),
        Duration::ZERO,
    );
    let cli =
        crate::args::Cli::try_parse_from(["brews", "accounts", "delete", id, "--yes"]).unwrap();
    let result = crate::operations::run(&server.config, cli, &mut Vec::new(), &mut BrokenPipe);
    assert!(matches!(result, Err(CliError::Output)));
    assert_eq!(server.connections(), 0);
}

#[test]
fn command_runner_exercises_real_https_to_safe_metadata() {
    let server = Server::start(
        200,
        "",
        br#"{"result":"accounts","accounts":[],"next_cursor":null}"#.to_vec(),
        Duration::ZERO,
    );
    let cli = crate::args::Cli::try_parse_from(["brews", "accounts", "list", "--json"]).unwrap();
    let mut out = Vec::new();
    let mut notice = Vec::new();
    crate::operations::run(&server.config, cli, &mut out, &mut notice).unwrap();
    assert!(notice.is_empty());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&out).unwrap()["result"],
        "accounts"
    );
    assert_eq!(server.connections(), 1);
}

#[test]
fn network_timeout_is_30_seconds_without_retry() {
    let server = Server::start(
        200,
        "",
        br#"{"result":"accounts","accounts":[],"next_cursor":null}"#.to_vec(),
        Duration::from_secs(31),
    );
    let started = Instant::now();
    let result = ManagementClient::new(&server.config)
        .unwrap()
        .execute(&list());
    assert!(matches!(result, Err(CliError::Transport)));
    assert!(started.elapsed() >= Duration::from_secs(29));
    assert!(started.elapsed() < Duration::from_secs(32));
    assert_eq!(server.connections(), 1);
}

#[test]
fn mutation_transmits_supplied_id_exactly_once() {
    let id = "01890f3e-53b7-7d28-9b05-4f65092d5711";
    let prepared = crate::args::Cli::try_parse_from([
        "brews",
        "accounts",
        "delete",
        id,
        "--yes",
        "--command-id",
        id,
    ])
    .unwrap()
    .prepare()
    .unwrap();
    let server = Server::start(
        202,
        "",
        format!(r#"{{"result":"pending","operation_id":"{id}"}}"#).into_bytes(),
        Duration::ZERO,
    );
    let result = ManagementClient::new(&server.config)
        .unwrap()
        .execute(&prepared);
    assert!(matches!(result, Ok(ManagementResponse::Pending { .. })));
    let request = server.request();
    assert!(
        request
            .headers
            .contains(&format!("idempotency-key: {id}\r\n"))
    );
    assert_eq!(
        request
            .headers
            .to_ascii_lowercase()
            .matches("idempotency-key:")
            .count(),
        1
    );
    assert_eq!(server.connections(), 1);
}

#[test]
fn client_rejects_noncanonical_or_non_https_origins_before_network() {
    let mut server = Server::start(200, "", vec![], Duration::ZERO);
    for origin in [
        "http://localhost",
        "https://user:private@localhost",
        "https://localhost/path",
        "https://localhost?private",
        "https://localhost/#private",
        "HTTPS://localhost",
        "https://LOCALHOST",
    ] {
        server.config.api_origin = origin.into();
        assert!(ManagementClient::new(&server.config).is_err());
    }
    assert_eq!(server.connections(), 0);
}

#[test]
fn native_client_does_not_use_environment_proxies() {
    if std::env::var_os("BREWS_TEST_PROXY_CHILD").is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "client::tests::native_client_does_not_use_environment_proxies",
            ])
            .env("BREWS_TEST_PROXY_CHILD", "1")
            .env("HTTPS_PROXY", "http://127.0.0.1:1")
            .env("https_proxy", "http://127.0.0.1:1")
            .env("ALL_PROXY", "http://127.0.0.1:1")
            .env("all_proxy", "http://127.0.0.1:1")
            .env_remove("NO_PROXY")
            .env_remove("no_proxy")
            .output()
            .unwrap();
        assert!(output.status.success());
        return;
    }
    let server = Server::start(
        200,
        "",
        br#"{"result":"accounts","accounts":[],"next_cursor":null}"#.to_vec(),
        Duration::ZERO,
    );
    assert!(
        ManagementClient::new(&server.config)
            .unwrap()
            .execute(&list())
            .is_ok()
    );
    assert_eq!(server.connections(), 1);
}

#[test]
fn response_size_is_bounded_at_128_kib() {
    let base = br#"{"result":"accounts","accounts":[],"next_cursor":null}"#;
    for (size, accepted) in [(128 * 1024, true), (128 * 1024 + 1, false)] {
        let mut body = base.to_vec();
        body.resize(size, b' ');
        let server = Server::start(200, "", body, Duration::ZERO);
        let result = ManagementClient::new(&server.config)
            .unwrap()
            .execute(&list());
        assert_eq!(result.is_ok(), accepted);
        assert_eq!(server.connections(), 1);
    }
}

#[test]
fn http_status_and_cookie_policy_are_fail_closed() {
    for (status, headers) in [
        (201, ""),
        (202, ""),
        (302, "Location: https://example.invalid/\r\n"),
        (403, ""),
        (404, ""),
        (503, ""),
        (200, "Set-Cookie: forbidden=secret; Secure\r\n"),
    ] {
        let server = Server::start(
            status,
            headers,
            br#"{"result":"accounts","accounts":[],"next_cursor":null}"#.to_vec(),
            Duration::ZERO,
        );
        let result = ManagementClient::new(&server.config)
            .unwrap()
            .execute(&list());
        if status == 200 || status == 202 {
            assert!(matches!(result, Err(CliError::Protocol)));
        } else {
            assert!(matches!(result, Err(CliError::HttpStatus(code)) if code == status));
        }
        assert_eq!(server.connections(), 1);
    }
}

#[test]
fn real_tls_client_sends_authenticated_read_without_browser_headers() {
    let server = Server::start(
        200,
        "",
        br#"{"result":"accounts","accounts":[],"next_cursor":null}"#.to_vec(),
        Duration::ZERO,
    );
    let result = ManagementClient::new(&server.config)
        .unwrap()
        .execute(&list());
    assert!(matches!(result, Ok(ManagementResponse::Accounts { .. })));
    let request = server.request();
    assert!(
        request
            .headers
            .starts_with("POST /_dev/commands HTTP/1.1\r\n")
    );
    let lower = request.headers.to_ascii_lowercase();
    assert!(lower.contains("\r\nauthorization: bearer "));
    assert!(
        !lower.contains("cookie:")
            && !lower.contains("origin:")
            && !lower.contains("idempotency-key:")
    );
    let payload: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(payload["operation"], "list_accounts");
    assert_eq!(server.connections(), 1);
}
