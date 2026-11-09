use rex_providers::*;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Mutex;

/// Scripted transport: returns queued (status, body) answers and records the
/// URLs and header names it was asked for (values stay unrecorded).
struct ScriptedTransport {
    answers: Mutex<VecDeque<(u16, String)>>,
    seen_urls: Mutex<Vec<String>>,
    seen_header_names: Mutex<Vec<String>>,
}

impl ScriptedTransport {
    fn new(answers: Vec<(u16, String)>) -> Self {
        Self {
            answers: Mutex::new(answers.into()),
            seen_urls: Mutex::new(vec![]),
            seen_header_names: Mutex::new(vec![]),
        }
    }
}

impl Transport for ScriptedTransport {
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<(u16, String), ProviderError> {
        self.seen_urls.lock().unwrap().push(url.to_string());
        for (name, _) in headers {
            self.seen_header_names.lock().unwrap().push(name.clone());
        }
        self.answers
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| ProviderError::Network("script exhausted".into()))
    }
}

fn service_with(
    transport: ScriptedTransport,
) -> ProviderService<MemorySecretStore, ScriptedTransport> {
    ProviderService::new(MemorySecretStore::new(), transport)
}

#[test]
fn gemini_catalog_normalizes_and_filters() {
    let body = serde_json::json!({
        "models": [
            {"name": "models/gemini-2.5-flash", "displayName": "Gemini 2.5 Flash", "supportedGenerationMethods": ["generateContent"]},
            {"name": "models/gemini-embedding-001", "displayName": "Embedding", "supportedGenerationMethods": ["embedContent"]},
            {"name": "models/gemini-2.5-pro", "displayName": "Gemini 2.5 Pro", "supportedGenerationMethods": ["generateContent", "countTokens"]}
        ]
    })
    .to_string();
    let svc = service_with(ScriptedTransport::new(vec![(200, body)]));
    svc.set_key("gemini", "test-key", None).unwrap();
    let catalog = svc.refresh("gemini").unwrap();
    assert_eq!(catalog.source, CatalogSource::Live);
    assert_eq!(
        catalog.models.len(),
        2,
        "embedding-only model must be filtered out"
    );
    assert_eq!(catalog.models[0].id, "gemini-2.5-flash");
    assert_eq!(catalog.models[0].label, "Gemini 2.5 Flash");
    assert_eq!(catalog.models[1].id, "gemini-2.5-pro");
    assert!(
        svc.transport()
            .seen_header_names
            .lock()
            .unwrap()
            .contains(&"x-goog-api-key".to_string()),
        "gemini auth must use the x-goog-api-key header"
    );
}

#[test]
fn gemini_paginates() {
    let page1 = serde_json::json!({
        "models": [{"name": "models/a-1", "supportedGenerationMethods": ["generateContent"]}],
        "nextPageToken": "tok2"
    })
    .to_string();
    let page2 = serde_json::json!({
        "models": [{"name": "models/a-2", "supportedGenerationMethods": ["generateContent"]}]
    })
    .to_string();
    let svc = service_with(ScriptedTransport::new(vec![(200, page1), (200, page2)]));
    svc.set_key("gemini", "test-key", None).unwrap();
    let catalog = svc.refresh("gemini").unwrap();
    assert_eq!(catalog.models.len(), 2);
    let urls = svc.transport().seen_urls.lock().unwrap();
    assert_eq!(urls.len(), 2);
    assert!(urls[1].contains("pageToken=tok2"));
}

#[test]
fn anthropic_catalog_normalizes() {
    let body = serde_json::json!({
        "data": [
            {"type": "model", "id": "claude-sonnet-4-5", "display_name": "Claude Sonnet 4.5"},
            {"type": "model", "id": "claude-haiku-4-5", "display_name": "Claude Haiku 4.5"}
        ],
        "has_more": false
    })
    .to_string();
    let svc = service_with(ScriptedTransport::new(vec![(200, body)]));
    svc.set_key("anthropic", "test-key", None).unwrap();
    let catalog = svc.refresh("anthropic").unwrap();
    assert_eq!(catalog.models.len(), 2);
    assert_eq!(catalog.models[0].label, "Claude Haiku 4.5");
    let names = svc.transport().seen_header_names.lock().unwrap();
    assert!(names.contains(&"x-api-key".to_string()));
    assert!(names.contains(&"anthropic-version".to_string()));
}

#[test]
fn openai_compatible_catalog_normalizes() {
    let body = serde_json::json!({
        "object": "list",
        "data": [{"id": "deepseek-chat"}, {"id": "deepseek-reasoner"}]
    })
    .to_string();
    let svc = service_with(ScriptedTransport::new(vec![(200, body)]));
    svc.set_key("deepseek", "test-key", None).unwrap();
    let catalog = svc.refresh("deepseek").unwrap();
    assert_eq!(catalog.models.len(), 2);
    assert_eq!(catalog.models[0].id, "deepseek-chat");
    let urls = svc.transport().seen_urls.lock().unwrap();
    assert_eq!(urls[0], "https://api.deepseek.com/models");
}

#[test]
fn openai_base_url_override_is_used() {
    let body = serde_json::json!({"data": [{"id": "local-model"}]}).to_string();
    let svc = service_with(ScriptedTransport::new(vec![(200, body)]));
    svc.set_key("local", "not-needed", Some("http://127.0.0.1:1234/v1/"))
        .unwrap();
    let catalog = svc.refresh("local").unwrap();
    assert_eq!(catalog.models[0].id, "local-model");
    let urls = svc.transport().seen_urls.lock().unwrap();
    assert_eq!(
        urls[0], "http://127.0.0.1:1234/v1/models",
        "trailing slash must be trimmed"
    );
}

#[test]
fn manual_discovery_reports_unsupported() {
    let svc = service_with(ScriptedTransport::new(vec![]));
    svc.set_key("custom", "test-key", Some("https://example.com/v1"))
        .unwrap();
    let err = svc.refresh("custom").unwrap_err();
    assert!(matches!(err, ProviderError::Unsupported(_)));
}

#[test]
fn missing_key_is_not_configured() {
    let svc = service_with(ScriptedTransport::new(vec![]));
    let err = svc.refresh("gemini").unwrap_err();
    assert_eq!(err, ProviderError::NotConfigured);
}

#[test]
fn auth_failure_is_mapped() {
    let svc = service_with(ScriptedTransport::new(vec![(401, "{}".into())]));
    svc.set_key("gemini", "bad-key", None).unwrap();
    let err = svc.refresh("gemini").unwrap_err();
    assert_eq!(err, ProviderError::AuthFailed);
    let summary = svc
        .summaries()
        .into_iter()
        .find(|s| s.id == "gemini")
        .unwrap();
    assert_eq!(summary.last_error, Some(ProviderError::AuthFailed));
}

#[test]
fn rate_limit_is_mapped() {
    let svc = service_with(ScriptedTransport::new(vec![(429, "{}".into())]));
    svc.set_key("gemini", "k", None).unwrap();
    assert_eq!(
        svc.refresh("gemini").unwrap_err(),
        ProviderError::RateLimited
    );
}

#[test]
fn empty_catalog_is_mapped() {
    let svc = service_with(ScriptedTransport::new(vec![(
        200,
        "{\"models\": []}".into(),
    )]));
    svc.set_key("gemini", "k", None).unwrap();
    assert_eq!(
        svc.refresh("gemini").unwrap_err(),
        ProviderError::EmptyCatalog
    );
}

#[test]
fn invalid_json_is_mapped() {
    let svc = service_with(ScriptedTransport::new(vec![(200, "not json".into())]));
    svc.set_key("gemini", "k", None).unwrap();
    assert!(matches!(
        svc.refresh("gemini").unwrap_err(),
        ProviderError::InvalidResponse(_)
    ));
}

#[test]
fn failed_refresh_keeps_previous_catalog() {
    let good = serde_json::json!({
        "models": [{"name": "models/keep-me", "supportedGenerationMethods": ["generateContent"]}]
    })
    .to_string();
    let svc = service_with(ScriptedTransport::new(vec![
        (200, good),
        (500, "{}".into()),
    ]));
    svc.set_key("gemini", "k", None).unwrap();
    svc.refresh("gemini").unwrap();
    assert!(matches!(
        svc.refresh("gemini").unwrap_err(),
        ProviderError::InvalidResponse(_)
    ));
    let catalog = svc
        .catalog("gemini")
        .expect("previous catalog must survive a failed refresh");
    assert_eq!(catalog.models[0].id, "keep-me");
}

#[test]
fn clear_key_drops_catalog() {
    let good = serde_json::json!({
        "models": [{"name": "models/x", "supportedGenerationMethods": ["generateContent"]}]
    })
    .to_string();
    let svc = service_with(ScriptedTransport::new(vec![(200, good)]));
    svc.set_key("gemini", "k", None).unwrap();
    svc.refresh("gemini").unwrap();
    svc.clear_key("gemini").unwrap();
    assert!(svc.catalog("gemini").is_none());
    let summary = svc
        .summaries()
        .into_iter()
        .find(|s| s.id == "gemini")
        .unwrap();
    assert!(!summary.has_key);
}

#[test]
fn file_secret_store_roundtrip_and_permissions() {
    let dir = std::env::temp_dir().join(format!("rex-test-secrets-{}", std::process::id()));
    let store = FileSecretStore::new(dir.clone()).unwrap();
    store.set_key("gemini", "s3cr3t").unwrap();
    assert_eq!(store.get_key("gemini").unwrap().as_deref(), Some("s3cr3t"));
    assert!(store.has_key("gemini"));
    store.clear_key("gemini").unwrap();
    assert!(!store.has_key("gemini"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join("secrets.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "secrets file must be owner-only");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn live_fixture_parses_through_normalizer() {
    // Recorded from the real Gemini ListModels API (see fixtures/README).
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/fixtures/gemini-list-models-live.json"
    );
    if let Ok(body) = std::fs::read_to_string(path) {
        let svc = service_with(ScriptedTransport::new(vec![(200, body)]));
        svc.set_key("gemini", "recorded", None).unwrap();
        let catalog = svc.refresh("gemini").unwrap();
        assert!(!catalog.models.is_empty(), "live fixture must yield models");
        for model in &catalog.models {
            assert!(!model.id.is_empty());
            assert!(!model.id.starts_with("models/"), "prefix must be stripped");
        }
    }
}

// ---- Error taxonomy against a real socket through the real HTTP client ----

struct OneShotServer {
    base: String,
    handle: Option<std::thread::JoinHandle<Vec<String>>>,
}

impl OneShotServer {
    /// Serve `responses` in order, one per connection; returns recorded request targets.
    fn start(responses: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for response in responses {
                if let Ok((stream, _)) = listener.accept() {
                    let mut reader = BufReader::new(stream);
                    let mut line = String::new();
                    if reader.read_line(&mut line).is_ok() {
                        seen.push(line.trim().to_string());
                    }
                    // Drain headers.
                    loop {
                        let mut h = String::new();
                        match reader.read_line(&mut h) {
                            Ok(0) | Err(_) => break,
                            _ if h.trim().is_empty() => break,
                            _ => {}
                        }
                    }
                    let mut stream = reader.into_inner();
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.flush();
                }
            }
            seen
        });
        Self {
            base: format!("http://127.0.0.1:{port}"),
            handle: Some(handle),
        }
    }
}

impl Drop for OneShotServer {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn http_response(status: u16, body: &str) -> String {
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Status",
    };
    format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[test]
fn real_http_client_maps_statuses() {
    let server = OneShotServer::start(vec![
        http_response(401, "{}"),
        http_response(429, "{}"),
        http_response(200, "{\"data\": [{\"id\": \"m1\"}]}"),
    ]);
    let svc = ProviderService::new(MemorySecretStore::new(), UreqTransport::new());
    svc.set_key("local", "k", Some(&format!("{}/v1", server.base)))
        .unwrap();
    assert_eq!(svc.refresh("local").unwrap_err(), ProviderError::AuthFailed);
    assert_eq!(
        svc.refresh("local").unwrap_err(),
        ProviderError::RateLimited
    );
    let catalog = svc.refresh("local").unwrap();
    assert_eq!(catalog.models[0].id, "m1");
}

#[test]
fn real_http_client_reports_network_error() {
    // Port 1 is reserved and refuses connections: a genuine network failure.
    let svc = ProviderService::new(MemorySecretStore::new(), UreqTransport::new());
    svc.set_key("local", "k", Some("http://127.0.0.1:1/v1"))
        .unwrap();
    assert!(matches!(
        svc.refresh("local").unwrap_err(),
        ProviderError::Network(_)
    ));
}

// ---- Access policy: which account types REX may connect, and why ----

#[test]
fn every_registry_provider_has_a_policy() {
    for spec in registry() {
        let policies = access_policies(spec.id);
        assert!(
            !policies.is_empty(),
            "provider {} must carry an access policy",
            spec.id
        );
    }
}

#[test]
fn non_supported_routes_always_cite_official_sources() {
    for spec in registry() {
        for policy in access_policies(spec.id) {
            if policy.status == PolicyStatus::Supported {
                continue;
            }
            assert!(
                !policy.sources.is_empty(),
                "{} / {} is {:?} but cites no source",
                spec.id,
                policy.label,
                policy.status
            );
            for source in policy.sources {
                assert!(
                    source.starts_with("https://"),
                    "sources must be canonical URLs: {source}"
                );
            }
        }
    }
}

#[test]
fn supported_subscription_routes_are_grounded() {
    let mut found = 0;
    for spec in registry() {
        for policy in access_policies(spec.id) {
            let is_subscription = matches!(
                policy.kind,
                AccessKind::SubscriptionKey | AccessKind::SubscriptionOauth
            );
            if is_subscription && policy.status == PolicyStatus::Supported {
                found += 1;
                assert!(
                    policy.sources.len() >= 2,
                    "{} / {} is an offered subscription hook and needs primary sources",
                    spec.id,
                    policy.label
                );
            }
        }
    }
    assert!(
        found >= 1,
        "at least one legitimately offered subscription hook must exist"
    );
}

#[test]
fn subscription_oauth_is_offered_only_on_openais_documented_route() {
    // Consumer-subscription sign-in is not documented for third-party
    // harnesses by any provider except OpenAI, whose Sign in with ChatGPT is
    // open to open-source, locally hosted apps. Any other route offered as
    // supported would be a quiet add; this test forces a sourced review.
    let mut openai_rows = 0;
    for spec in registry() {
        for policy in access_policies(spec.id) {
            if policy.kind != AccessKind::SubscriptionOauth {
                continue;
            }
            let openai_route = spec.id == "openai" || spec.id == "openai-chatgpt";
            if policy.status == PolicyStatus::Supported {
                assert!(
                    openai_route,
                    "{} / {}: consumer-subscription OAuth must not be offered",
                    spec.id, policy.label
                );
            }
            if openai_route {
                openai_rows += 1;
                assert!(
                    policy.sources.len() >= 2
                        && policy
                            .sources
                            .iter()
                            .any(|s| s.starts_with("https://developers.openai.com/siwc/")),
                    "{} / {}: the OpenAI route must cite its Sign in with ChatGPT documentation",
                    spec.id,
                    policy.label
                );
                // Open while REX is released as open source, and exactly then.
                let expected = if OPEN_SOURCE_RELEASE {
                    PolicyStatus::Supported
                } else {
                    PolicyStatus::OpenSourceOnly
                };
                assert_eq!(policy.status, expected, "{} / {}", spec.id, policy.label);
            }
        }
    }
    assert!(
        openai_rows >= 2,
        "both the OpenAI and the ChatGPT-plan providers describe the route"
    );
}

#[test]
fn the_release_switch_cannot_be_thrown_before_the_release() {
    // `OPEN_SOURCE_RELEASE` is the claim the whole ChatGPT-plan route rests
    // on: OpenAI offers it to open-source, locally hosted clients. While any
    // crate still declares a proprietary licence, or the repository has no
    // licence file, the claim is false and the switch must stay off.
    if !OPEN_SOURCE_RELEASE {
        return;
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let has_licence_file = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| {
            let name = e.file_name().to_string_lossy().to_ascii_uppercase();
            name.starts_with("LICENSE") || name.starts_with("COPYING")
        });
    assert!(
        has_licence_file,
        "OPEN_SOURCE_RELEASE is true but the repository has no LICENSE file"
    );
    for entry in std::fs::read_dir(root.join("crates")).unwrap().flatten() {
        let manifest = entry.path().join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        assert!(
            !text.lines().any(|l| {
                let l = l.replace(' ', "").to_ascii_lowercase();
                l.starts_with("license=") && l.contains("proprietary")
            }),
            "OPEN_SOURCE_RELEASE is true but {} still declares a proprietary licence",
            manifest.display()
        );
    }
}

#[test]
fn every_crate_in_the_repository_carries_the_open_source_licence() {
    // REX is dual licensed. A crate added without the field, or with an old
    // proprietary one, would leave the repository half-licensed, and the
    // ChatGPT-plan route is only open to open-source apps.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in ["LICENSE-MIT", "LICENSE-APACHE"] {
        let text = std::fs::read_to_string(root.join(name)).expect(name);
        assert!(text.len() > 1000, "{name} looks empty");
    }
    assert!(std::fs::read_to_string(root.join("LICENSE-MIT"))
        .unwrap()
        .contains("Permission is hereby granted, free of charge"));
    assert!(std::fs::read_to_string(root.join("LICENSE-APACHE"))
        .unwrap()
        .contains("Apache License"));
    let mut manifests: Vec<_> = std::fs::read_dir(root.join("crates"))
        .unwrap()
        .flatten()
        .map(|e| e.path().join("Cargo.toml"))
        .collect();
    manifests.push(root.join("src-tauri/Cargo.toml"));
    for manifest in manifests {
        let text = std::fs::read_to_string(&manifest).expect("a crate manifest");
        assert!(
            text.lines()
                .any(|l| l.trim() == r#"license = "MIT OR Apache-2.0""#),
            "{} does not declare MIT OR Apache-2.0",
            manifest.display()
        );
    }
}

#[test]
fn the_chatgpt_plan_route_is_closed_until_release() {
    // Before the release nothing may start a sign-in, hand out a token, or
    // accept a pasted one; the provider shows as unconnected.
    if OPEN_SOURCE_RELEASE {
        return;
    }
    let svc = service_with(ScriptedTransport::new(vec![]));
    assert!(matches!(
        svc.chatgpt_begin_sign_in(false, false),
        Err(ProviderError::Credential(m)) if m.contains("not available")
    ));
    assert!(matches!(
        svc.get_key("openai-chatgpt"),
        Err(ProviderError::Credential(_))
    ));
    assert!(matches!(
        svc.chatgpt_status(),
        Err(ProviderError::Credential(_))
    ));
    let summary = svc
        .summaries()
        .into_iter()
        .find(|s| s.id == "openai-chatgpt")
        .expect("the ChatGPT plan provider is in the registry");
    assert!(!summary.has_key);
    assert!(summary.signin.is_none());
    assert!(svc.transport().seen_urls.lock().unwrap().is_empty());
}

#[test]
fn a_pasted_key_is_never_accepted_for_the_chatgpt_plan() {
    // Whatever the release state: the only credential for this provider is
    // the one a sign-in issued.
    let svc = service_with(ScriptedTransport::new(vec![]));
    assert!(matches!(
        svc.set_key("openai-chatgpt", "sk-pasted-token", None),
        Err(ProviderError::Unsupported(_))
    ));
    assert!(
        !svc.summaries()
            .into_iter()
            .find(|s| s.id == "openai-chatgpt")
            .unwrap()
            .has_key
    );
}

#[test]
fn after_release_the_chatgpt_plan_asks_for_a_sign_in_not_a_key() {
    if !OPEN_SOURCE_RELEASE {
        return;
    }
    let svc = service_with(ScriptedTransport::new(vec![]));
    let err = svc.get_key("openai-chatgpt").unwrap_err();
    assert!(
        matches!(&err, ProviderError::Credential(m) if m.contains("not signed in")),
        "{err}"
    );
    assert!(svc.chatgpt_status().is_ok());
}

#[test]
fn anthropic_subscription_signin_stays_not_permitted() {
    // Anthropic's legal page explicitly forbids routing requests through
    // Free/Pro/Max plan credentials in third-party products. Guard it.
    let policies = access_policies("anthropic");
    assert!(policies
        .iter()
        .any(|p| p.kind == AccessKind::SubscriptionOauth
            && p.status == PolicyStatus::NotPermitted
            && !p.sources.is_empty()));
}

#[test]
fn verified_dates_are_real_dates() {
    let valid = |d: &str| {
        d.len() == 10
            && d.chars().enumerate().all(|(i, c)| {
                if i == 4 || i == 7 {
                    c == '-'
                } else {
                    c.is_ascii_digit()
                }
            })
    };
    assert!(valid(VERIFIED_ON));
    assert!(valid(OPENAI_SIWC_VERIFIED_ON));
    assert!(OPENAI_SIWC_VERIFIED_ON >= VERIFIED_ON);
    for spec in registry() {
        for policy in access_policies(spec.id) {
            assert!(valid(policy.verified_on));
            // a row re-verified later than the matrix carries its own date,
            // and only the dates this crate knows are allowed
            assert!(
                policy.verified_on == VERIFIED_ON || policy.verified_on == OPENAI_SIWC_VERIFIED_ON,
                "{} / {}: unknown verification date {}",
                spec.id,
                policy.label,
                policy.verified_on
            );
        }
    }
}

#[test]
fn summaries_expose_access_policies() {
    let svc = service_with(ScriptedTransport::new(vec![]));
    for summary in svc.summaries() {
        assert_eq!(summary.access.len(), access_policies(&summary.id).len());
    }
}

#[test]
fn openai_and_xai_and_qwen_use_openai_models_discovery() {
    let body = serde_json::json!({"data": [{"id": "m-1"}]}).to_string();
    for (provider, expected_url) in [
        ("openai", "https://api.openai.com/v1/models"),
        ("xai", "https://api.x.ai/v1/models"),
        (
            "qwen",
            "https://dashscope.aliyuncs.com/compatible-mode/v1/models",
        ),
    ] {
        let svc = service_with(ScriptedTransport::new(vec![(200, body.clone())]));
        svc.set_key(provider, "test-key", None).unwrap();
        let catalog = svc.refresh(provider).unwrap();
        assert_eq!(catalog.models[0].id, "m-1");
        let urls = svc.transport().seen_urls.lock().unwrap();
        assert_eq!(urls[0], expected_url, "wrong listing URL for {provider}");
    }
}

#[test]
fn kimi_coding_honors_manual_discovery_without_http() {
    // The Kimi for Coding endpoint has no documented model-listing API; REX
    // must say so truthfully instead of probing an unsupported route.
    let svc = service_with(ScriptedTransport::new(vec![]));
    svc.set_key("kimi-coding", "test-key", None).unwrap();
    let err = svc.refresh("kimi-coding").unwrap_err();
    assert!(matches!(err, ProviderError::Unsupported(_)));
    assert!(
        svc.transport().seen_urls.lock().unwrap().is_empty(),
        "no HTTP call may be attempted"
    );
}

#[derive(Clone)]
struct SearchTransport {
    status: u16,
    body: String,
}
impl Transport for SearchTransport {
    fn get(&self, _: &str, _: &[(String, String)]) -> Result<(u16, String), ProviderError> {
        Ok((self.status, self.body.clone()))
    }
    fn post(
        &self,
        _: &str,
        _: &[(String, String)],
        _: &str,
    ) -> Result<(u16, String), ProviderError> {
        Ok((self.status, self.body.clone()))
    }
}
#[test]
fn search_router_defaults_to_private_rex_and_requires_keys_for_external_selection() {
    let router = SearchRouter::new(
        MemorySecretStore::new(),
        SearchTransport {
            status: 200,
            body: "{}".into(),
        },
    );
    assert_eq!(router.active(), SearchProvider::Rex);
    assert_eq!(
        router.select(SearchProvider::Exa),
        Err(ProviderError::NotConfigured)
    );
    router.set_key(SearchProvider::Exa, "secret").unwrap();
    router.select(SearchProvider::Exa).unwrap();
    assert_eq!(router.active(), SearchProvider::Exa);
    router.clear_key(SearchProvider::Exa).unwrap();
    assert_eq!(router.active(), SearchProvider::Rex);
}
#[test]
fn exa_and_tinyfish_normalize_to_one_evidence_contract() {
    let body =
        r#"{"results":[{"url":"https://example.com/a","title":"A","text":"body","score":0.9}]}"#;
    for provider in [SearchProvider::Exa, SearchProvider::Tinyfish] {
        let router = SearchRouter::new(
            MemorySecretStore::new(),
            SearchTransport {
                status: 200,
                body: body.into(),
            },
        );
        router.set_key(provider, "secret").unwrap();
        router.select(provider).unwrap();
        let response = router
            .search(rex_search::SearchRequest {
                query: "a".into(),
                seeds: vec![],
                max_pages: 2,
                max_results: 2,
                allow_subdomains: false,
                discover_sitemaps: false,
                discover_feeds: false,
            })
            .unwrap();
        assert_eq!(response.evidence[0].url, "https://example.com/a");
        assert_eq!(response.evidence[0].state, rex_search::FetchState::Fetched);
        assert!(response.coverage.model.starts_with("hosted:"));
    }
}
#[test]
fn hosted_rate_limits_stay_truthful() {
    let router = SearchRouter::new(
        MemorySecretStore::new(),
        SearchTransport {
            status: 429,
            body: String::new(),
        },
    );
    router.set_key(SearchProvider::Tinyfish, "secret").unwrap();
    router.select(SearchProvider::Tinyfish).unwrap();
    let result = router.search(rex_search::SearchRequest {
        query: "a".into(),
        seeds: vec![],
        max_pages: 1,
        max_results: 1,
        allow_subdomains: false,
        discover_sitemaps: false,
        discover_feeds: false,
    });
    assert_eq!(result.unwrap_err(), ProviderError::RateLimited);
}

// ---- base URL and key sharing across service instances -----------------
//
// The desktop app and the dev sidecar build several provider services (UI,
// run service, agent). They share one credential store, but each used to keep
// the Connect dialog's base URL only in its own memory, so a custom endpoint
// saved in the UI was invisible to the agent ("no base URL configured").

fn shared_pair() -> (
    ProviderService<std::sync::Arc<MemorySecretStore>, ScriptedTransport>,
    ProviderService<std::sync::Arc<MemorySecretStore>, ScriptedTransport>,
) {
    let store = std::sync::Arc::new(MemorySecretStore::new());
    (
        ProviderService::new(store.clone(), ScriptedTransport::new(vec![])),
        ProviderService::new(store, ScriptedTransport::new(vec![])),
    )
}

#[test]
fn a_custom_endpoint_saved_by_one_service_is_visible_to_another() {
    let (ui, agent) = shared_pair();
    assert!(
        agent.base_url("custom").is_err(),
        "a custom provider has no default endpoint"
    );
    ui.set_key("custom", "k", Some("https://api.example.test/v1/"))
        .unwrap();
    assert_eq!(
        agent.base_url("custom").unwrap(),
        "https://api.example.test/v1",
        "the agent's service must resolve the endpoint the UI saved (trailing slash trimmed)"
    );
    assert_eq!(agent.get_key("custom").unwrap().as_deref(), Some("k"));
}

#[test]
fn a_non_default_local_server_url_reaches_the_other_service() {
    let (ui, agent) = shared_pair();
    assert_eq!(
        agent.base_url("local").unwrap(),
        "http://127.0.0.1:11434/v1"
    );
    ui.set_key("local", "placeholder", Some("http://127.0.0.1:1234/v1"))
        .unwrap();
    assert_eq!(agent.base_url("local").unwrap(), "http://127.0.0.1:1234/v1");
}

#[test]
fn an_empty_base_url_restores_the_provider_default() {
    let (ui, agent) = shared_pair();
    ui.set_key("local", "k", Some("http://127.0.0.1:9/v1"))
        .unwrap();
    ui.set_key("local", "k", Some("  ")).unwrap();
    assert_eq!(
        agent.base_url("local").unwrap(),
        "http://127.0.0.1:11434/v1"
    );
}

#[test]
fn omitting_the_base_url_keeps_the_saved_one() {
    let (ui, agent) = shared_pair();
    ui.set_key("custom", "k1", Some("https://a.example.test/v1"))
        .unwrap();
    ui.set_key("custom", "k2", None).unwrap();
    assert_eq!(
        agent.base_url("custom").unwrap(),
        "https://a.example.test/v1"
    );
}

#[test]
fn disconnecting_forgets_the_custom_endpoint_everywhere() {
    let (ui, agent) = shared_pair();
    ui.set_key("custom", "k", Some("https://api.example.test/v1"))
        .unwrap();
    ui.clear_key("custom").unwrap();
    assert!(agent.base_url("custom").is_err());
    assert_eq!(agent.get_key("custom").unwrap(), None);
    assert!(
        !ui.summaries()
            .iter()
            .any(|s| s.id == "custom" && s.base_url.is_some()),
        "the summary must not advertise a forgotten endpoint"
    );
}

#[test]
fn the_endpoint_slot_never_looks_like_a_connected_provider() {
    let (ui, _agent) = shared_pair();
    ui.set_key("custom", "k", Some("https://api.example.test/v1"))
        .unwrap();
    let ids: Vec<_> = ui
        .summaries()
        .into_iter()
        .filter(|s| s.has_key)
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, vec!["custom".to_string()]);
}

#[test]
fn services_over_separate_handles_to_one_key_file_agree_too() {
    // The desktop app: each service opens its own FileSecretStore on the same
    // directory. The file is re-read on every call, so they stay in step.
    let tmp = tempfile::tempdir().unwrap();
    let a = ProviderService::new(
        FileSecretStore::new(tmp.path().to_path_buf()).unwrap(),
        ScriptedTransport::new(vec![]),
    );
    let b = ProviderService::new(
        FileSecretStore::new(tmp.path().to_path_buf()).unwrap(),
        ScriptedTransport::new(vec![]),
    );
    a.set_key("custom", "k", Some("https://api.example.test/v1"))
        .unwrap();
    assert_eq!(b.base_url("custom").unwrap(), "https://api.example.test/v1");
    a.clear_key("custom").unwrap();
    assert!(b.base_url("custom").is_err());
}

#[test]
fn services_with_unshared_memory_stores_stay_independent() {
    // Documents why the sidecar must hand every service the same Arc: two
    // separate in-memory stores genuinely cannot see each other.
    let a = ProviderService::new(MemorySecretStore::new(), ScriptedTransport::new(vec![]));
    let b = ProviderService::new(MemorySecretStore::new(), ScriptedTransport::new(vec![]));
    a.set_key("custom", "k", Some("https://x.example.test/v1"))
        .unwrap();
    assert!(b.base_url("custom").is_err());
    assert_eq!(b.get_key("custom").unwrap(), None);
}
