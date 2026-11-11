use super::*;

#[test]
fn model_urls_accept_base_and_chat_endpoints() {
    for endpoint in [
        " https://example.test/coding/v1/ ",
        "https://example.test/coding/v1/chat/completions",
        "https://example.test/coding/v1/chat/completions/",
    ] {
        assert_eq!(
            models_endpoint(endpoint).unwrap(),
            "https://example.test/coding/v1/models"
        );
    }
    assert_eq!(
        models_endpoint("http://localhost:11434/v1").unwrap(),
        "http://localhost:11434/v1/models"
    );
    assert_eq!(
        models_endpoint("https://example.test/v1/chat/completions?version=1").unwrap(),
        "https://example.test/v1/models?version=1"
    );
    for endpoint in ["", "invalid", "file:///tmp/models", "ftp://example.test/v1"] {
        assert!(models_endpoint(endpoint).is_err());
    }
}

#[test]
fn model_list_is_sorted_unique_and_validated() {
    assert_eq!(
        parse_models(&json!({"data": [{"id": "z-model"}, {"id": "a-model"}, {"id": " a-model "}]}))
            .unwrap(),
        ["a-model", "z-model"]
    );
    assert!(parse_models(&json!({"data": []})).unwrap().is_empty());
    for value in [
        json!({}),
        json!({"data": {}}),
        json!({"data": [{"id": ""}]}),
        json!({"data": [{"id": 1}]}),
        json!({"data": [{}]}),
    ] {
        assert!(parse_models(&value).is_err());
    }
}

#[test]
fn models_are_loaded_with_optional_auth_and_bounded_responses() {
    use std::{
        io::{BufRead, BufReader, Write},
        net::TcpListener,
    };
    for (status, body, key, expected) in [
        (
            200,
            r#"{"data":[{"id":"installed-model"}]}"#.to_owned(),
            "",
            Ok(vec!["installed-model".to_owned()]),
        ),
        (200, r#"{"data":[]}"#.to_owned(), "test-key", Ok(vec![])),
        (
            401,
            r#"{"error":{"message":"Invalid key"}}"#.to_owned(),
            "test-key",
            Err("HTTP 401"),
        ),
        (404, "Not found".to_owned(), "", Err("HTTP 404")),
        (200, "not json".to_owned(), "", Err("invalid JSON")),
        (
            200,
            " ".repeat(MAX_RESPONSE_BYTES + 1),
            "",
            Err("1 MiB limit"),
        ),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "No model list request received"
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("Cannot accept test request: {error}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut headers = String::new();
            let mut reader = BufReader::new(&mut socket);
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                headers.push_str(&line);
            }
            write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            headers
        });
        let config = Config {
            endpoint: format!("http://{address}/v1/chat/completions"),
            model: String::new(),
            instructions: String::new(),
            api_key: key.into(),
            store_api_key: false,
        };
        let result = list_models(&config);
        let headers = server.join().unwrap();
        assert!(headers.starts_with("GET /v1/models HTTP/1.1\r\n"));
        let headers = headers.to_ascii_lowercase();
        if key.is_empty() {
            assert!(!headers.contains("authorization:"));
        } else {
            assert!(headers.contains("authorization: bearer test-key\r\n"));
        }
        match expected {
            Ok(models) => assert_eq!(result.unwrap(), models),
            Err(detail) => assert!(result.unwrap_err().contains(detail)),
        }
    }
}

#[test]
fn cancellation_closes_requests_waiting_for_headers_or_body() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        sync::{mpsc, Arc},
    };

    for partial_body in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let (ready_sender, ready_receiver) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(std::time::Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("Cannot accept test request: {error}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut socket);
            let mut length = 0;
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
            }
            reader.read_exact(&mut vec![0; length]).unwrap();
            if partial_body {
                socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\n{")
                    .unwrap();
                socket.flush().unwrap();
            }
            ready_sender.send(()).unwrap();
            let result = socket.read(&mut [0; 1]);
            assert!(
                matches!(result, Ok(0))
                    || matches!(result, Err(ref error) if error.kind() == std::io::ErrorKind::ConnectionReset),
                "Request was not closed: {result:?}"
            );
        });
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (sender, receiver) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let config = Config {
                endpoint: format!("http://{address}/v1"),
                model: "test-model".into(),
                ..Config::default()
            };
            sender
                .send(generate(&config, "test patch", &worker_cancel))
                .unwrap();
        });
        ready_receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        cancel.store(true, Ordering::Relaxed);
        let result = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(result.unwrap_err(), "Commit message generation cancelled");
        worker.join().unwrap();
        server.join().unwrap();
    }
}

#[test]
fn recognizes_only_loopback_servers_as_local() {
    for endpoint in [
        "http://localhost:11434/v1",
        " http://127.0.0.1:1234/v1/ ",
        "http://127.0.0.2:8080/v1",
        "http://[::1]:8080/v1",
    ] {
        assert!(Config {
            endpoint: endpoint.into(),
            ..Config::default()
        }
        .is_local());
    }
    for endpoint in [
        DEFAULT_ENDPOINT,
        "http://localhost.example/v1",
        "http://192.168.1.2/v1",
        "invalid",
    ] {
        assert!(!Config {
            endpoint: endpoint.into(),
            ..Config::default()
        }
        .is_local());
    }
}

#[test]
fn local_generation_sends_patch_without_auth_and_handles_server_failures() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;

    for (status, body, expected) in [
        (
            200,
            r#"{"choices":[{"message":{"content":"  Update local generation\n"}}]}"#,
            Ok("Update local generation"),
        ),
        (
            404,
            r#"{"error":{"message":"model is not installed"}}"#,
            Err("model is not installed"),
        ),
        (
            200,
            r#"{"choices":[{"message":{"content":" "}}]}"#,
            Err("does not contain a commit message"),
        ),
        (200, "invalid json", Err("invalid JSON")),
        (302, "", Err("HTTP 302")),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "No local LLM request received"
                        );
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("Cannot accept test request: {error}"),
                }
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut socket);
            let mut headers = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse::<usize>().unwrap();
                }
                headers.push_str(&line);
            }
            let mut request = vec![0; length];
            reader.read_exact(&mut request).unwrap();
            write!(socket, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            (headers, serde_json::from_slice::<Value>(&request).unwrap())
        });
        let config = Config {
            endpoint: format!("http://{address}/v1"),
            model: "installed-local-model".into(),
            store_api_key: false,
            ..Config::default()
        };
        let result = generate(&config, "diff --git a/file b/file", &AtomicBool::new(false));
        let (headers, request) = server.join().unwrap();
        assert!(headers.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"));
        assert!(!headers.to_ascii_lowercase().contains("authorization:"));
        assert_eq!(request["model"], "installed-local-model");
        assert_eq!(request["stream"], false);
        assert!(request["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("diff --git a/file b/file"));
        match expected {
            Ok(message) => assert_eq!(result.unwrap(), message),
            Err(detail) => assert!(result.unwrap_err().contains(detail)),
        }
    }
}

#[test]
fn parses_openai_compatible_response() {
    let value = json!({"choices": [{"message": {"content": "  Add LLM commit messages\n"}}]});
    assert_eq!(parse_message(&value).unwrap(), "Add LLM commit messages");
    assert!(parse_message(&json!({"choices": []})).is_err());
}

#[test]
fn accepts_base_urls_and_full_chat_completions_endpoints() {
    for (input, expected) in [
        (
            "https://api.kimi.ai/coding/v1",
            "https://api.kimi.ai/coding/v1/chat/completions",
        ),
        (
            "https://api.kimi.ai/coding/v1/",
            "https://api.kimi.ai/coding/v1/chat/completions",
        ),
        (
            " https://api.kimi.ai/coding/v1/chat/completions ",
            "https://api.kimi.ai/coding/v1/chat/completions",
        ),
        (DEFAULT_ENDPOINT, DEFAULT_ENDPOINT),
    ] {
        assert_eq!(chat_completions_endpoint(input), expected);
    }
}

#[test]
fn provider_presets_have_unique_endpoints_and_models() {
    let mut endpoints = std::collections::HashSet::new();
    for provider in PROVIDER_PRESETS {
        assert!(!provider.name.is_empty());
        assert!(!provider.model.is_empty() || provider.name.ends_with("(local)"));
        assert!(provider.endpoint.ends_with("/chat/completions"));
        assert!(endpoints.insert(provider.endpoint));
    }
    assert_eq!(PROVIDER_PRESETS[0].endpoint, DEFAULT_ENDPOINT);
    assert_eq!(PROVIDER_PRESETS[0].model, DEFAULT_MODEL);
}

#[test]
fn api_key_credentials_are_scoped_to_normalized_endpoints() {
    assert_eq!(
        credential_user(" https://api.kimi.ai/coding/v1/ ", None),
        credential_user("https://api.kimi.ai/coding/v1/chat/completions", None)
    );
    assert_ne!(
        credential_user(DEFAULT_ENDPOINT, None),
        credential_user("https://api.deepseek.com/v1/chat/completions", None)
    );
}

#[test]
fn repository_credentials_are_isolated_from_global_and_other_repositories() {
    let global = credential_user(DEFAULT_ENDPOINT, None);
    let first = credential_user(DEFAULT_ENDPOINT, Some("/work/first"));
    let second = credential_user(DEFAULT_ENDPOINT, Some("/work/second"));
    assert_eq!(global, format!("llm-api-key:{DEFAULT_ENDPOINT}"));
    assert_ne!(global, first);
    assert_ne!(first, second);
    assert_eq!(
        first,
        credential_user("https://api.openai.com/v1/", Some("/work/first"))
    );
    assert_ne!(
        first,
        credential_user("https://other.example/v1", Some("/work/first"))
    );
}

#[test]
fn protected_key_does_not_read_the_login_keychain() {
    let result = load_api_key_from_stores(
        || Ok(Some("protected-key".into())),
        || panic!("must not read the login keychain after Touch ID"),
        || panic!("must not migrate after Touch ID"),
    );
    assert_eq!(result, Ok(Some("protected-key".into())));
}

#[test]
fn protected_read_failures_never_fall_back_to_password_authentication() {
    for error in [
        "Touch ID cancelled",
        "missing entitlement",
        "authentication failed",
    ] {
        let result = load_api_key_from_stores(
            || Err(error.into()),
            || panic!("must not ask for the login keychain password"),
            || panic!("must not migrate after a protected read error"),
        );
        assert_eq!(result, Err(error.into()));
    }
}

#[test]
fn storage_lookup_stops_at_the_first_existing_key() {
    let calls = std::cell::RefCell::new(Vec::new());
    let result = load_api_key_from_stores(
        || {
            calls.borrow_mut().push("protected");
            Ok(None)
        },
        || {
            calls.borrow_mut().push("scoped");
            Ok(None)
        },
        || {
            calls.borrow_mut().push("legacy");
            Ok(Some("migrated-key".into()))
        },
    );
    assert_eq!(result, Ok(Some("migrated-key".into())));
    assert_eq!(*calls.borrow(), ["protected", "scoped", "legacy"]);
}

#[test]
fn scoped_read_success_or_failure_does_not_read_an_unrelated_key() {
    for result in [
        Ok(Some("scoped-key".into())),
        Err("authentication cancelled".into()),
    ] {
        let expected = result.clone();
        assert_eq!(
            load_api_key_from_stores(
                || Ok(None),
                || result,
                || panic!("must not read a different key after scoped read"),
            ),
            expected,
        );
    }
}

#[test]
fn scoped_key_is_read_once_without_accessing_legacy_storage() {
    let reads = std::cell::Cell::new(0);
    let result = load_api_key_from_stores(
        || with_keychain_fallback(|| Err(ProtectedKeychainError::Unavailable), || Ok(None)),
        || {
            reads.set(reads.get() + 1);
            Ok(Some("scoped-key".into()))
        },
        || panic!("must stop after reading the scoped key"),
    );
    assert_eq!(result, Ok(Some("scoped-key".into())));
    assert_eq!(reads.get(), 1);
}

#[test]
fn missing_or_cancelled_scoped_read_only_falls_back_when_absent() {
    for result in [Ok(None), Err("authentication cancelled".into())] {
        let legacy_reads = std::cell::Cell::new(0);
        let expected = result.clone();
        assert_eq!(
            load_api_key_from_stores(
                || Ok(None),
                || result,
                || {
                    legacy_reads.set(legacy_reads.get() + 1);
                    Ok(None)
                },
            ),
            expected,
        );
        assert_eq!(legacy_reads.get(), usize::from(expected.is_ok()));
    }
}

#[test]
fn legacy_lookup_reads_once_and_returns_authentication_errors_without_retrying() {
    for result in [
        Ok(Some("legacy-key".into())),
        Ok(None),
        Err("authentication cancelled".into()),
    ] {
        let reads = std::cell::Cell::new(0);
        let expected = result.clone();
        let actual = load_api_key_from_stores(
            || Ok(None),
            || Ok(None),
            || {
                reads.set(reads.get() + 1);
                result
            },
        );
        assert_eq!(actual, expected);
        assert_eq!(reads.get(), 1);
    }
}

#[test]
fn saved_credentials_restore_the_model_when_no_model_is_configured() {
    let secret = serde_json::to_string(&SavedCredential {
        model: "custom-model".into(),
        api_key: "test-key".into(),
    })
    .unwrap();
    let mut config = Config {
        model: String::new(),
        ..Config::default()
    };
    restore_credential(&mut config, secret).unwrap();
    assert_eq!(config.model, "custom-model");
    assert_eq!(config.api_key, "test-key");
}

#[test]
fn legacy_keys_keep_the_configured_model() {
    let mut config = Config {
        model: "existing-model".into(),
        ..Config::default()
    };
    restore_credential(&mut config, "legacy-key".into()).unwrap();
    assert_eq!(config.model, "existing-model");
    assert_eq!(config.api_key, "legacy-key");
}

#[test]
fn invalid_saved_credentials_do_not_change_the_configuration_or_expose_secrets() {
    let mut config = Config::default();
    let original = config.clone();
    let error = restore_credential(&mut config, r#"{"api_key":"secret"}"#.into()).unwrap_err();
    assert_eq!(config, original);
    assert!(!error.contains("secret"));
}

#[test]
fn explicitly_provided_api_key_is_not_replaced() {
    let mut config = Config {
        api_key: "from-environment".into(),
        ..Config::default()
    };
    load_api_key_if_needed(&mut config, None).unwrap();
    assert_eq!(config.api_key, "from-environment");

    config.api_key.clear();
    config.store_api_key = false;
    load_api_key_if_needed(&mut config, None).unwrap();
    assert!(config.api_key.is_empty());
}

#[test]
fn omits_temperature_from_generation_requests() {
    let body = request_body(&Config::default(), "diff --git a/file b/file");
    assert!(body.get("temperature").is_none());
    assert_eq!(body.get("model"), Some(&json!(DEFAULT_MODEL)));
}

#[test]
fn includes_api_error_details_in_failures() {
    assert_eq!(
        api_error(
            400,
            br#"{"error":{"message":"temperature must be omitted"}}"#
        ),
        "LLM API returned HTTP 400: temperature must be omitted"
    );
    assert_eq!(
        api_error(429, b"  rate limit exceeded\n"),
        "LLM API returned HTTP 429: rate limit exceeded"
    );
    assert_eq!(
        api_error(500, b""),
        "LLM API returned HTTP 500. Check the endpoint, model, API key and provider limits."
    );
    let truncated = api_error(400, "x".repeat(2001).as_bytes());
    assert!(truncated.ends_with('…'));
    assert_eq!(
        truncated.chars().count(),
        "LLM API returned HTTP 400: ".chars().count() + 2001
    );
}

#[test]
fn validates_configuration_without_exposing_key() {
    let mut config = Config::default();
    assert!(config.validate().is_ok());
    config.api_key = "secret".into();
    assert!(config.validate().is_ok());
    let saved = serde_json::to_string(&config).unwrap();
    assert!(!saved.contains("secret"));
    let saved: Value = serde_json::from_str(&saved).unwrap();
    assert!(saved.get("api_key").is_none());
    assert_eq!(saved.get("store_api_key"), Some(&Value::Bool(true)));
}

#[test]
fn keychain_fallback_is_only_used_when_protected_storage_is_unavailable() {
    assert_eq!(
        with_keychain_fallback(
            || Ok("protected-key"),
            || panic!("must not read the login keychain after protected access"),
        ),
        Ok("protected-key"),
    );
    assert_eq!(
        with_keychain_fallback(
            || Err(ProtectedKeychainError::Unavailable),
            || Ok("login-key"),
        ),
        Ok("login-key"),
    );
    for error in [
        "Touch ID cancelled",
        "authentication failed",
        "invalid UTF-8",
    ] {
        assert_eq!(
            with_keychain_fallback(
                || Err::<(), _>(ProtectedKeychainError::Failed(error.into())),
                || panic!("must not fall back after a protected access failure"),
            ),
            Err(error.into()),
        );
    }
}

#[test]
fn keychain_fallback_reports_login_storage_failures() {
    let result = with_keychain_fallback(
        || Err::<(), _>(ProtectedKeychainError::Unavailable),
        || Err("login keychain locked".into()),
    );
    assert_eq!(result, Err("login keychain locked".into()));
}

#[test]
fn loading_saved_key_preserves_the_configured_model() {
    let mut config = Config {
        model: "new-selection".into(),
        ..Config::default()
    };
    restore_credential(
        &mut config,
        r#"{"model":"old-selection","api_key":"saved-key"}"#.into(),
    )
    .unwrap();
    assert_eq!(config.model, "new-selection");
    assert_eq!(config.api_key, "saved-key");
}
