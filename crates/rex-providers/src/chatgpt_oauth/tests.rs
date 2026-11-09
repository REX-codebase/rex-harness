//! The sign-in flow against a scripted OpenAI that enforces the contract:
//! it recomputes the PKCE challenge from the verifier, checks the exact
//! parameters of each request, refuses a client secret, and signs real ES256
//! ID tokens. The browser is a real loopback request.

use super::*;
use crate::secrets::MemorySecretStore;
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING};
use serde_json::json;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

const JWKS_URI: &str = "https://auth.openai.com/.well-known/jwks.json";
const REVOKE_URI: &str = "https://auth.openai.com/api/accounts/oauth/revoke";
const ISSUED: &str = "oaiapp_issued";
const PLAN_ALL: &str =
    "chatgpt.tokens.use.direct email offline_access openid profile resource.invoke";

/// A fake identity provider: a signing key whose public half is the JWKS.
#[derive(Clone)]
struct Idp {
    pkcs8: Vec<u8>,
    jwks: Value,
}

impl Idp {
    fn new() -> Idp {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
            .unwrap();
        let p = pair.public_key().as_ref().to_vec();
        let jwks = json!({"keys":[{"kty":"EC","crv":"P-256","kid":"k1","use":"sig","alg":"ES256",
            "x": URL_SAFE_NO_PAD.encode(&p[1..33]),"y": URL_SAFE_NO_PAD.encode(&p[33..65])}]});
        Idp {
            pkcs8: pkcs8.as_ref().to_vec(),
            jwks,
        }
    }

    /// An ID token carrying `claims`, signed with this provider's key.
    fn sign(&self, claims: &Value) -> String {
        let rng = SystemRandom::new();
        let pair =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &self.pkcs8, &rng).unwrap();
        let h = URL_SAFE_NO_PAD.encode(json!({"alg":"ES256","kid":"k1"}).to_string());
        let p = URL_SAFE_NO_PAD.encode(claims.to_string());
        let signing = format!("{h}.{p}");
        let sig = pair.sign(&rng, signing.as_bytes()).unwrap();
        format!("{signing}.{}", URL_SAFE_NO_PAD.encode(sig.as_ref()))
    }
}

fn id_claims(aud: &str, nonce: &str, sub: &str) -> Value {
    let now = store::now_secs();
    json!({"iss":ISSUER,"sub":sub,"aud":[aud],"nonce":nonce,"iat":now,"exp":now+3600,
        "sid":"sid_1","email":"alex@example.com"})
}

type Handler = Box<dyn Fn(&HashMap<String, String>) -> (u16, String) + Send + Sync>;

struct Net {
    idp: Idp,
    posts: Mutex<Vec<(String, HashMap<String, String>)>>,
    token: Mutex<Handler>,
    discovery: Mutex<(u16, String)>,
    revocations: Mutex<Vec<Result<u16, ()>>>,
}

fn fake() -> Arc<Net> {
    Arc::new(Net {
        idp: Idp::new(),
        posts: Mutex::new(vec![]),
        token: Mutex::new(Box::new(|_| (500, String::new()))),
        discovery: Mutex::new((
            200,
            json!({"issuer":ISSUER,"jwks_uri":JWKS_URI,"revocation_endpoint":REVOKE_URI})
                .to_string(),
        )),
        revocations: Mutex::new(vec![]),
    })
}

impl Net {
    fn on_token(
        &self,
        f: impl Fn(&HashMap<String, String>) -> (u16, String) + Send + Sync + 'static,
    ) {
        *self.token.lock().unwrap() = Box::new(f);
    }
    fn posts_to(&self, url: &str) -> Vec<HashMap<String, String>> {
        self.posts
            .lock()
            .unwrap()
            .iter()
            .filter(|(u, _)| u == url)
            .map(|(_, f)| f.clone())
            .collect()
    }
}

impl Transport for Net {
    fn get(&self, url: &str, _h: &[(String, String)]) -> Result<(u16, String), ProviderError> {
        match url {
            DISCOVERY_URL => Ok(self.discovery.lock().unwrap().clone()),
            JWKS_URI => Ok((200, self.idp.jwks.to_string())),
            _ => Ok((404, String::new())),
        }
    }
    fn post(
        &self,
        url: &str,
        _h: &[(String, String)],
        body: &str,
    ) -> Result<(u16, String), ProviderError> {
        let form: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        self.posts
            .lock()
            .unwrap()
            .push((url.to_string(), form.clone()));
        match url {
            TOKEN_URL => Ok((self.token.lock().unwrap())(&form)),
            REVOKE_URI => {
                let mut q = self.revocations.lock().unwrap();
                if q.is_empty() {
                    return Ok((200, String::new()));
                }
                match q.remove(0) {
                    Ok(s) => Ok((s, String::new())),
                    Err(()) => Err(ProviderError::Network("down".into())),
                }
            }
            _ => Ok((404, String::new())),
        }
    }
}

fn param(url: &str, key: &str) -> Option<String> {
    url::Url::parse(url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

/// What a browser does at the end of sign-in: follow the redirect.
fn browse(authorize_url: &str, extra: &str, code: &str) {
    let redirect = param(authorize_url, "redirect_uri").unwrap();
    let state = param(authorize_url, "state").unwrap();
    let u = url::Url::parse(&redirect).unwrap();
    let port = u.port().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(60));
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let req = format!(
        "GET {}?code={code}&state={state}{extra} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n",
        u.path()
    );
    s.write_all(req.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
}

fn run(
    net: &Net,
    store: &MemorySecretStore,
    signin: &SignIn,
    extra: &str,
) -> Result<SignedIn, OAuthError> {
    std::thread::scope(|sc| {
        sc.spawn(|| browse(signin.authorize_url(), extra, "CODE-1"));
        complete(
            net,
            store,
            signin,
            Duration::from_secs(5),
            &AtomicBool::new(false),
        )
    })
}

/// A scripted token endpoint that honours the contract for `sub`.
fn contract_endpoint(net: &Net, signin: &SignIn, sub: &'static str, scope: &'static str) {
    let challenge = param(signin.authorize_url(), "code_challenge").unwrap();
    let nonce = param(signin.authorize_url(), "nonce").unwrap();
    let redirect = signin.redirect_uri().to_string();
    let idp = net.idp.clone();
    net.on_token(move |f| {
        assert_eq!(f["grant_type"], "authorization_code");
        assert_eq!(
            f["client_id"], ISSUED,
            "the exchange uses the issued id, never the entry point"
        );
        assert_eq!(f["code"], "CODE-1");
        assert_eq!(
            pkce_challenge(&f["code_verifier"]),
            challenge,
            "the verifier must hash to the challenge sent"
        );
        assert_eq!(
            f["redirect_uri"], redirect,
            "the exact same URI, port included"
        );
        assert_eq!(f["resource"], RESOURCE);
        assert!(!f.contains_key("client_secret"), "this is a public client");
        let id = idp.sign(&id_claims(ISSUED, &nonce, sub));
        (
            200,
            json!({"access_token":"AT-1","refresh_token":"RT-1","id_token":id,"token_type":"Bearer",
                "expires_in":3600,"scope":scope}).to_string(),
        )
    });
}

fn signed_in_record(store: &MemorySecretStore, expires_in: u64, saved_ago: u64) {
    let now = store::now_secs();
    store::save(
        store,
        &Record {
            ext_agent_host_id: store::host_id(store).unwrap(),
            registration: Registration {
                client_id: ISSUED.into(),
                subject: "sub-1".into(),
                email: Some("alex@example.com".into()),
                issuer: ISSUER.into(),
            },
            tokens: Some(Tokens {
                id_token: "ID-OLD".into(),
                access_token: "AT-OLD".into(),
                refresh_token: Some("RT-OLD".into()),
                token_type: "Bearer".into(),
                expires_in,
                scopes: PLAN_ALL.split(' ').map(str::to_string).collect(),
                saved_at: now - saved_ago,
                earliest_refresh_at: None,
            }),
        },
    )
    .unwrap();
}

// ---- the first sign-in: dynamic registration ------------------------------

#[test]
fn the_first_sign_in_registers_dynamically_and_follows_the_contract_end_to_end() {
    let net = fake();
    let store = MemorySecretStore::new();
    let signin = begin(&store, false).unwrap();
    let url = signin.authorize_url().to_string();
    assert!(url.starts_with("https://auth.openai.com/api/accounts/authorize?"));
    assert_eq!(param(&url, "client_id").as_deref(), Some(DYNAMIC_CLIENT_ID));
    assert_eq!(param(&url, "agent_name_hint").as_deref(), Some("REX"));
    assert_eq!(param(&url, "response_type").as_deref(), Some("code"));
    assert_eq!(
        param(&url, "scope").as_deref(),
        Some("openid profile email offline_access resource.invoke chatgpt.tokens.use.direct")
    );
    assert_eq!(param(&url, "resource").as_deref(), Some(RESOURCE));
    assert_eq!(
        param(&url, "code_challenge_method").as_deref(),
        Some("S256")
    );
    let host = param(&url, "ext_agent_host_id").unwrap();
    assert!(host.starts_with("urn:uuid:"), "{host}");
    assert_eq!(
        store::host_id(&store).unwrap(),
        host,
        "the host id was persisted before sign-in"
    );
    let redirect = param(&url, "redirect_uri").unwrap();
    assert!(redirect.starts_with("http://127.0.0.1:") && redirect.ends_with("/auth/callback"));
    for absent in ["id_token_hint", "login_hint", "prompt", "client_secret"] {
        assert!(
            param(&url, absent).is_none(),
            "{absent} does not belong on a first sign-in"
        );
    }
    // fresh, unguessable values each attempt
    let again = begin(&store, false).unwrap();
    assert_ne!(param(again.authorize_url(), "state"), param(&url, "state"));
    assert_ne!(param(again.authorize_url(), "nonce"), param(&url, "nonce"));
    assert_ne!(
        param(again.authorize_url(), "code_challenge"),
        param(&url, "code_challenge")
    );
    drop(again);

    contract_endpoint(&net, &signin, "sub-1", PLAN_ALL);
    let done = run(
        &net,
        &store,
        &signin,
        &format!("&client_id={ISSUED}&scope=openid"),
    )
    .unwrap();
    assert_eq!(
        done,
        SignedIn {
            email: Some("alex@example.com".into()),
            plan_usage: true
        }
    );

    let rec = store::load(&store).unwrap().unwrap();
    assert_eq!(
        rec.registration.client_id, ISSUED,
        "the issued id is saved, not the entry point"
    );
    assert_eq!(rec.registration.subject, "sub-1");
    assert_eq!(rec.ext_agent_host_id, host);
    let t = rec.tokens.unwrap();
    assert_eq!(
        (t.access_token.as_str(), t.refresh_token.as_deref()),
        ("AT-1", Some("RT-1"))
    );
    assert!(t.plan_usage());
    assert_eq!(net.posts_to(TOKEN_URL).len(), 1);
}

#[test]
fn a_declined_consent_stops_before_any_code_is_exchanged() {
    let net = fake();
    let store = MemorySecretStore::new();
    let signin = begin(&store, false).unwrap();
    let r = std::thread::scope(|sc| {
        sc.spawn(|| {
            let redirect = param(signin.authorize_url(), "redirect_uri").unwrap();
            let state = param(signin.authorize_url(), "state").unwrap();
            let port = url::Url::parse(&redirect).unwrap().port().unwrap();
            std::thread::sleep(Duration::from_millis(60));
            let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let _ = write!(s, "GET /auth/callback?error=access_denied&state={state} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n");
            let mut o = String::new();
            let _ = s.read_to_string(&mut o);
        });
        complete(
            &*net,
            &store,
            &signin,
            Duration::from_secs(5),
            &AtomicBool::new(false),
        )
    });
    assert_eq!(r, Err(OAuthError::Denied));
    assert!(
        net.posts.lock().unwrap().is_empty(),
        "no token request for a declined consent"
    );
    assert!(store::load(&store).unwrap().is_none());
}

#[test]
fn a_new_registration_without_an_issued_client_id_is_incomplete() {
    for extra in ["", &format!("&client_id={DYNAMIC_CLIENT_ID}")] {
        let net = fake();
        let store = MemorySecretStore::new();
        let signin = begin(&store, false).unwrap();
        contract_endpoint(&net, &signin, "sub-1", PLAN_ALL);
        assert_eq!(
            run(&net, &store, &signin, extra),
            Err(OAuthError::RegistrationIncomplete)
        );
        assert!(net.posts.lock().unwrap().is_empty());
        assert!(store::load(&store).unwrap().is_none());
    }
}

#[test]
fn a_sign_in_without_the_plan_scope_is_kept_but_cannot_spend_the_plan() {
    let net = fake();
    let store = MemorySecretStore::new();
    let signin = begin(&store, false).unwrap();
    contract_endpoint(&net, &signin, "sub-1", "email openid profile");
    let done = run(&net, &store, &signin, &format!("&client_id={ISSUED}")).unwrap();
    assert!(!done.plan_usage);
    assert!(status(&store).unwrap().signed_in);
    assert!(!status(&store).unwrap().plan_usage);
    assert_eq!(
        access_token(&*net, &store),
        Err(OAuthError::PlanUsageDisabled)
    );
    // no refresh was attempted on its account
    assert!(net.posts_to(TOKEN_URL).len() == 1);
}

#[test]
fn an_id_token_that_fails_verification_saves_nothing() {
    for (label, claims) in [
        (
            "wrong audience",
            Box::new(|n: &str| id_claims("oaiapp_other", n, "sub-1")) as Box<dyn Fn(&str) -> Value>,
        ),
        (
            "wrong nonce",
            Box::new(|_| id_claims(ISSUED, "replayed", "sub-1")),
        ),
        (
            "expired",
            Box::new(|n| {
                let mut c = id_claims(ISSUED, n, "sub-1");
                c["exp"] = json!(store::now_secs() - 600);
                c
            }),
        ),
        (
            "wrong issuer",
            Box::new(|n| {
                let mut c = id_claims(ISSUED, n, "sub-1");
                c["iss"] = json!("https://evil.example");
                c
            }),
        ),
    ] {
        let net = fake();
        let store = MemorySecretStore::new();
        let signin = begin(&store, false).unwrap();
        let nonce = param(signin.authorize_url(), "nonce").unwrap();
        let id = net.idp.sign(&claims(&nonce));
        net.on_token(move |_| {
            (
                200,
                json!({"access_token":"AT","refresh_token":"RT","id_token":id,
                "expires_in":3600,"scope":PLAN_ALL})
                .to_string(),
            )
        });
        let r = run(&net, &store, &signin, &format!("&client_id={ISSUED}"));
        assert!(matches!(r, Err(OAuthError::IdToken(_))), "{label}: {r:?}");
        assert!(
            store::load(&store).unwrap().is_none(),
            "{label}: nothing may be saved"
        );
    }
}

#[test]
fn an_id_token_signed_by_someone_else_is_refused() {
    let net = fake();
    let store = MemorySecretStore::new();
    let signin = begin(&store, false).unwrap();
    let nonce = param(signin.authorize_url(), "nonce").unwrap();
    let forged = Idp::new().sign(&id_claims(ISSUED, &nonce, "sub-1")); // a different key
    net.on_token(move |_| {
        (
            200,
            json!({"access_token":"AT","refresh_token":"RT","id_token":forged,
            "expires_in":3600,"scope":PLAN_ALL})
            .to_string(),
        )
    });
    let r = run(&net, &store, &signin, &format!("&client_id={ISSUED}"));
    assert!(
        matches!(r, Err(OAuthError::IdToken(m)) if m.contains("signing key") || m.contains("signature"))
    );
    assert!(store::load(&store).unwrap().is_none());
}

#[test]
fn discovery_is_only_trusted_while_it_stays_on_openais_auth_host() {
    for doc in [
        json!({"issuer":"https://evil.example","jwks_uri":JWKS_URI}),
        json!({"issuer":ISSUER,"jwks_uri":"https://evil.example/jwks"}),
        json!({"issuer":ISSUER,"jwks_uri":"http://auth.openai.com/jwks"}),
        json!({"issuer":ISSUER}),
    ] {
        let net = fake();
        *net.discovery.lock().unwrap() = (200, doc.to_string());
        assert!(
            matches!(discover(&*net), Err(OAuthError::Discovery(_))),
            "{doc}"
        );
    }
    // a revocation endpoint elsewhere is dropped rather than trusted
    let net = fake();
    *net.discovery.lock().unwrap() = (
        200,
        json!({"issuer":ISSUER,"jwks_uri":JWKS_URI,"revocation_endpoint":"https://evil.example/revoke"}).to_string(),
    );
    assert!(discover(&*net).unwrap().revocation_endpoint.is_none());
    assert!(
        trusted_url("https://auth.openai.com/x")
            && !trusted_url("https://auth.openai.com.evil.example/x")
    );
}

// ---- signing in again -------------------------------------------------------

#[test]
fn signing_in_again_reuses_the_client_sends_hints_and_omits_the_agent_name() {
    let net = fake();
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 0);
    let signin = begin(&store, false).unwrap();
    let url = signin.authorize_url();
    assert_eq!(param(url, "client_id").as_deref(), Some(ISSUED));
    assert!(
        param(url, "agent_name_hint").is_none(),
        "only on the first registration"
    );
    assert_eq!(param(url, "id_token_hint").as_deref(), Some("ID-OLD"));
    assert_eq!(
        param(url, "login_hint").as_deref(),
        Some("alex@example.com")
    );
    assert_eq!(
        param(url, "ext_agent_host_id").unwrap(),
        store::host_id(&store).unwrap(),
        "the same host id as before"
    );
    // the version that may be shown or logged hides the hints
    let shown = signin.display_url();
    assert!(
        !shown.contains("ID-OLD") && !shown.contains("alex%40example.com"),
        "{shown}"
    );
    // and what a browser launcher is given on a command line names nothing
    let launch = signin.launch_url();
    assert!(
        launch.starts_with("http://127.0.0.1:")
            && !launch.contains("ID-OLD")
            && !launch.contains("alex")
            && !launch.contains("hint")
            && !launch.contains("auth.openai.com"),
        "{launch}"
    );
    // a callback without a client id reuses the saved one
    contract_endpoint(&net, &signin, "sub-1", PLAN_ALL);
    assert!(run(&net, &store, &signin, "").unwrap().plan_usage);
    // re-enabling plan usage asks for consent again, and only then
    assert_eq!(
        param(begin(&store, true).unwrap().authorize_url(), "prompt").as_deref(),
        Some("consent")
    );
    assert!(param(begin(&store, false).unwrap().authorize_url(), "prompt").is_none());
}

#[test]
fn a_different_account_or_client_never_overwrites_the_saved_credentials() {
    let net = fake();
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 0);
    let before = store::load(&store).unwrap().unwrap();
    // another account comes back
    let signin = begin(&store, false).unwrap();
    contract_endpoint(&net, &signin, "someone-else", PLAN_ALL);
    assert_eq!(
        run(&net, &store, &signin, "").unwrap_err(),
        OAuthError::AccountMismatch
    );
    assert_eq!(store::load(&store).unwrap().unwrap(), before);
    // a callback naming a different client is rejected before any exchange
    let net2 = fake();
    let signin = begin(&store, false).unwrap();
    contract_endpoint(&net2, &signin, "sub-1", PLAN_ALL);
    assert_eq!(
        run(&net2, &store, &signin, "&client_id=oaiapp_somebody_else").unwrap_err(),
        OAuthError::ClientMismatch
    );
    assert!(net2.posts.lock().unwrap().is_empty());
    assert_eq!(store::load(&store).unwrap().unwrap(), before);
}

// ---- refreshing ---------------------------------------------------------------

fn refresh_endpoint(net: &Net, scope: &'static str) {
    net.on_token(move |f| {
        assert_eq!(f["grant_type"], "refresh_token");
        assert_eq!(
            f["client_id"], ISSUED,
            "the saved issued id, not dynamic_agent_client"
        );
        assert_eq!(f["refresh_token"], "RT-OLD");
        assert_eq!(f["resource"], RESOURCE);
        assert!(
            !f.contains_key("scope"),
            "scope is omitted to retain the grant"
        );
        assert!(!f.contains_key("client_secret"));
        (
            200,
            json!({"access_token":"AT-NEW","refresh_token":"RT-NEW","token_type":"Bearer",
                "expires_in":3600,"scope":scope})
            .to_string(),
        )
    });
}

#[test]
fn a_fresh_token_is_used_without_touching_the_network() {
    let net = fake();
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 10);
    assert_eq!(access_token(&*net, &store).unwrap(), "AT-OLD");
    assert!(net.posts.lock().unwrap().is_empty());
}

#[test]
fn a_token_near_expiry_is_refreshed_and_everything_is_replaced_together() {
    let net = fake();
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 3600 - 60); // a minute left: inside the skew
    refresh_endpoint(&net, PLAN_ALL);
    assert_eq!(access_token(&*net, &store).unwrap(), "AT-NEW");
    let t = store::load(&store).unwrap().unwrap().tokens.unwrap();
    assert_eq!(t.access_token, "AT-NEW");
    assert_eq!(
        t.refresh_token.as_deref(),
        Some("RT-NEW"),
        "the rotated refresh token"
    );
    assert_eq!(
        t.id_token, "ID-OLD",
        "an absent ID token keeps the old one for later hints"
    );
    assert!(t.expires_at() > store::now_secs() + 3000);
    assert!(t.plan_usage());
    assert_eq!(net.posts_to(TOKEN_URL).len(), 1);
}

#[test]
fn a_refresh_that_loses_the_plan_scope_stops_plan_usage() {
    let net = fake();
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 3600);
    refresh_endpoint(&net, "openid email");
    assert_eq!(
        access_token(&*net, &store),
        Err(OAuthError::PlanUsageDisabled)
    );
    assert!(!status(&store).unwrap().plan_usage);
    assert_eq!(
        access_token(&*net, &store),
        Err(OAuthError::PlanUsageDisabled)
    );
}

#[test]
fn a_server_hold_on_refreshing_is_respected_while_the_token_is_still_valid() {
    let net = fake();
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 3600 - 100);
    let mut rec = store::load(&store).unwrap().unwrap();
    rec.tokens.as_mut().unwrap().earliest_refresh_at = Some(store::now_secs() + 600);
    store::save(&store, &rec).unwrap();
    assert_eq!(access_token(&*net, &store).unwrap(), "AT-OLD");
    assert!(net.posts.lock().unwrap().is_empty());
    // but an expired token is refreshed regardless
    signed_in_record(&store, 3600, 3700);
    let mut rec = store::load(&store).unwrap().unwrap();
    rec.tokens.as_mut().unwrap().earliest_refresh_at = Some(store::now_secs() + 600);
    store::save(&store, &rec).unwrap();
    refresh_endpoint(&net, PLAN_ALL);
    assert_eq!(access_token(&*net, &store).unwrap(), "AT-NEW");
}

#[test]
fn an_unusable_refresh_token_clears_the_credentials_but_keeps_the_registration() {
    for code in [
        "invalid_grant",
        "invalid_refresh_token",
        "token_expired",
        "refresh_token_expired",
        "refresh_token_invalidated",
        "refresh_token_reused",
    ] {
        let net = fake();
        let store = MemorySecretStore::new();
        signed_in_record(&store, 3600, 3700);
        net.on_token(move |_| (400, json!({"error": code}).to_string()));
        assert_eq!(
            access_token(&*net, &store),
            Err(OAuthError::NeedsSignIn(code.into())),
            "{code}"
        );
        let rec = store::load(&store).unwrap().unwrap();
        assert!(rec.tokens.is_none(), "{code}: unusable tokens are cleared");
        assert_eq!(
            rec.registration.client_id, ISSUED,
            "{code}: the client survives"
        );
    }
}

#[test]
fn a_network_or_server_failure_during_refresh_never_erases_credentials() {
    for status_code in [500u16, 502, 503, 429] {
        let net = fake();
        let store = MemorySecretStore::new();
        signed_in_record(&store, 3600, 3700);
        let before = store::load(&store).unwrap().unwrap();
        net.on_token(move |_| (status_code, "<html>busy</html>".into()));
        assert!(
            matches!(access_token(&*net, &store), Err(OAuthError::Transient(_))),
            "{status_code}"
        );
        assert_eq!(store::load(&store).unwrap().unwrap(), before);
    }
    // an invalid client is a configuration fault, reported, credentials kept
    let net = fake();
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 3700);
    net.on_token(|_| (401, json!({"error":"invalid_client"}).to_string()));
    assert_eq!(
        access_token(&*net, &store),
        Err(OAuthError::Provider("invalid_client".into()))
    );
    assert!(store::load(&store).unwrap().unwrap().tokens.is_some());
}

#[test]
fn concurrent_callers_cause_exactly_one_refresh() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    signed_in_record(&store, 3600, 3700);
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let c = calls.clone();
    net.on_token(move |f| {
        assert_eq!(f["refresh_token"], "RT-OLD", "a second refresh would present a spent token");
        c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(60));
        (
            200,
            json!({"access_token":"AT-NEW","refresh_token":"RT-NEW","expires_in":3600,"scope":PLAN_ALL}).to_string(),
        )
    });
    let handles: Vec<_> = (0..5)
        .map(|_| {
            let (net, store) = (net.clone(), store.clone());
            std::thread::spawn(move || access_token(&*net, &*store).unwrap())
        })
        .collect();
    for h in handles {
        assert_eq!(h.join().unwrap(), "AT-NEW");
    }
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn there_is_nothing_to_refresh_when_not_signed_in() {
    let net = fake();
    let store = MemorySecretStore::new();
    assert_eq!(access_token(&*net, &store), Err(OAuthError::NotSignedIn));
    signed_in_record(&store, 3600, 0);
    store::clear_tokens(&store).unwrap();
    assert_eq!(access_token(&*net, &store), Err(OAuthError::NotSignedIn));
    assert!(net.posts.lock().unwrap().is_empty());
}

// ---- signing out ----------------------------------------------------------------

#[test]
fn signing_out_revokes_the_refresh_token_then_clears_locally() {
    let net = fake();
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 0);
    let report = sign_out(&*net, &store).unwrap();
    assert_eq!(
        report,
        SignOutReport {
            revoked: true,
            note: None
        }
    );
    let reqs = net.posts_to(REVOKE_URI);
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0]["token"], "RT-OLD");
    assert_eq!(reqs[0]["token_type_hint"], "refresh_token");
    assert_eq!(reqs[0]["client_id"], ISSUED);
    let rec = store::load(&store).unwrap().unwrap();
    assert!(rec.tokens.is_none());
    assert_eq!(
        rec.registration.client_id, ISSUED,
        "reused at the next sign-in"
    );
    assert!(store::host_id(&store).unwrap().starts_with("urn:uuid:"));
    assert!(!status(&store).unwrap().signed_in);
}

#[test]
fn an_unconfirmed_revocation_is_retried_and_then_reported_honestly() {
    let net = fake();
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 0);
    // two failures, then success: retried
    *net.revocations.lock().unwrap() = vec![Err(()), Ok(503), Ok(200)];
    assert!(sign_out(&*net, &store).unwrap().revoked);
    assert_eq!(net.posts_to(REVOKE_URI).len(), 3);

    // never confirmed: tokens are cleared anyway and the user is told
    let net = fake();
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 0);
    *net.revocations.lock().unwrap() = vec![Err(()), Err(()), Err(())];
    let report = sign_out(&*net, &store).unwrap();
    assert!(!report.revoked);
    assert!(report.note.unwrap().contains("ChatGPT settings"));
    assert!(
        store::load(&store).unwrap().unwrap().tokens.is_none(),
        "cleared regardless"
    );
    // signing out when signed out is a no-op, not an error
    assert!(sign_out(&*net, &MemorySecretStore::new()).unwrap().revoked);
}

#[test]
fn the_status_the_ui_sees_carries_no_credential() {
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 0);
    let shown = serde_json::to_string(&status(&store).unwrap()).unwrap();
    for secret in ["AT-OLD", "RT-OLD", "ID-OLD"] {
        assert!(!shown.contains(secret), "{shown}");
    }
    assert!(shown.contains("alex@example.com") && shown.contains("\"plan_usage\":true"));
}

#[test]
fn a_sign_in_that_nobody_completes_times_out_and_can_be_cancelled() {
    let net = fake();
    let store = MemorySecretStore::new();
    let signin = begin(&store, false).unwrap();
    assert_eq!(
        complete(
            &*net,
            &store,
            &signin,
            Duration::from_millis(100),
            &AtomicBool::new(false)
        ),
        Err(OAuthError::TimedOut)
    );
    let cancel = AtomicBool::new(true);
    assert_eq!(
        complete(&*net, &store, &signin, Duration::from_secs(5), &cancel),
        Err(OAuthError::Cancelled)
    );
}

#[test]
fn pkce_matches_the_rfc_7636_example() {
    // RFC 7636, appendix B
    assert_eq!(
        pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn error_text_never_carries_a_token_and_codes_are_filtered() {
    let all = [
        OAuthError::Denied,
        OAuthError::Provider("invalid_grant".into()),
        OAuthError::NeedsSignIn("refresh_token_reused".into()),
        OAuthError::Transient("HTTP 503".into()),
    ];
    for e in all {
        let t = e.to_string();
        assert!(!t.contains("Bearer") && !t.contains("eyJ"), "{t}");
    }
    // a hostile error code from the server is not repeated
    let net = fake();
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 3700);
    net.on_token(|_| (400, json!({"error":"x y\nINJECT: secret"}).to_string()));
    match access_token(&*net, &store) {
        Err(OAuthError::Provider(code)) => assert!(!code.contains("INJECT"), "{code}"),
        other => panic!("{other:?}"),
    }
}

// ---- through the provider service ------------------------------------------
//
// The service is what runs, the CLI and the UI use: the same credential flow,
// behind `get_key`, `summaries`, `set_key` and `clear_key`.

use crate::catalog::CatalogSource;
use crate::service::ProviderService;

const MODELS_URL: &str = "https://api.openai.com/v1/models";

/// `Net` shared by handle, plus the one plan-token API call a catalog makes.
struct Shared {
    net: Arc<Net>,
    model_auth: Mutex<Vec<String>>,
}

impl Transport for Shared {
    fn get(&self, url: &str, h: &[(String, String)]) -> Result<(u16, String), ProviderError> {
        if url == MODELS_URL {
            let auth = h
                .iter()
                .find(|(k, _)| k == "authorization")
                .map(|(_, v)| v.clone())
                .unwrap_or_default();
            self.model_auth.lock().unwrap().push(auth);
            return Ok((
                200,
                json!({"models":[
                    {"slug":"gpt-first","display_name":"First","visibility":"list"},
                    {"slug":"gpt-hidden","display_name":"Hidden","visibility":"hide"},
                    {"slug":"gpt-second","display_name":"Second","visibility":"list"}
                ]})
                .to_string(),
            ));
        }
        self.net.get(url, h)
    }
    fn post(
        &self,
        url: &str,
        h: &[(String, String)],
        body: &str,
    ) -> Result<(u16, String), ProviderError> {
        self.net.post(url, h, body)
    }
}

fn service_over(
    net: &Arc<Net>,
    store: &Arc<MemorySecretStore>,
) -> ProviderService<Arc<MemorySecretStore>, Shared> {
    ProviderService::new(
        store.clone(),
        Shared {
            net: net.clone(),
            model_auth: Mutex::new(vec![]),
        },
    )
}

fn summary_of(svc: &ProviderService<Arc<MemorySecretStore>, Shared>) -> crate::ProviderSummary {
    svc.summaries()
        .into_iter()
        .find(|s| s.id == PROVIDER_ID)
        .expect("the ChatGPT plan provider is registered")
}

#[test]
fn the_service_hands_out_a_renewed_token_and_shows_a_credential_free_sign_in() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    signed_in_record(&store, 3600, 3600 - 60);
    refresh_endpoint(&net, PLAN_ALL);
    let svc = service_over(&net, &store);

    assert_eq!(svc.get_key(PROVIDER_ID).unwrap().as_deref(), Some("AT-NEW"));
    assert_eq!(net.posts_to(TOKEN_URL).len(), 1);
    // a second call within the window needs no network
    assert_eq!(svc.get_key(PROVIDER_ID).unwrap().as_deref(), Some("AT-NEW"));
    assert_eq!(net.posts_to(TOKEN_URL).len(), 1);

    let summary = summary_of(&svc);
    assert!(summary.has_key);
    let signin = summary.signin.clone().expect("sign-in status is shown");
    assert!(signin.signed_in && signin.plan_usage);
    assert_eq!(signin.email.as_deref(), Some("alex@example.com"));
    let shown = serde_json::to_string(&svc.summaries()).unwrap();
    for secret in ["AT-NEW", "RT-NEW", "AT-OLD", "RT-OLD", "ID-OLD"] {
        assert!(
            !shown.contains(secret),
            "{secret} must not reach the UI summary"
        );
    }
}

#[test]
fn the_service_lists_models_with_the_plan_token_in_the_servers_order() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    signed_in_record(&store, 3600, 10);
    let svc = service_over(&net, &store);
    let catalog = svc.refresh(PROVIDER_ID).unwrap();
    assert_eq!(catalog.source, CatalogSource::Live);
    let ids: Vec<_> = catalog.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        ["gpt-first", "gpt-second"],
        "hidden entries are left out"
    );
    assert_eq!(
        svc.transport().model_auth.lock().unwrap().as_slice(),
        ["Bearer AT-OLD"]
    );
}

#[test]
fn without_a_sign_in_the_service_says_to_sign_in_not_to_paste_a_key() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    let svc = service_over(&net, &store);
    let err = svc.get_key(PROVIDER_ID).unwrap_err();
    assert!(
        matches!(&err, ProviderError::Credential(m) if m.contains("not signed in")),
        "{err}"
    );
    assert!(!summary_of(&svc).has_key);
    assert!(matches!(
        svc.set_key(PROVIDER_ID, "sk-pasted", None),
        Err(ProviderError::Unsupported(_))
    ));
    assert!(
        store.get_key(PROVIDER_ID).unwrap().is_none(),
        "nothing pasted was stored"
    );
    assert!(net.posts.lock().unwrap().is_empty());
}

#[test]
fn a_sign_in_without_plan_usage_is_reported_as_such_and_is_not_connected() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    let signin = begin(&*store, false).unwrap();
    contract_endpoint(&net, &signin, "sub-1", "email openid profile");
    run(&net, &store, &signin, &format!("&client_id={ISSUED}")).unwrap();
    let svc = service_over(&net, &store);
    let err = svc.get_key(PROVIDER_ID).unwrap_err();
    assert!(
        matches!(&err, ProviderError::Credential(m) if m.contains("not enabled")),
        "{err}"
    );
    let summary = summary_of(&svc);
    assert!(!summary.has_key, "no spendable plan: not connected");
    assert!(summary.signin.unwrap().signed_in);
}

#[test]
fn a_flaky_network_is_a_network_error_and_costs_no_credentials() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    signed_in_record(&store, 3600, 3600 - 60);
    net.on_token(|_| (503, "{}".into()));
    let svc = service_over(&net, &store);
    assert!(matches!(
        svc.get_key(PROVIDER_ID),
        Err(ProviderError::Network(_))
    ));
    let t = store::load(&*store).unwrap().unwrap().tokens.unwrap();
    assert_eq!(t.refresh_token.as_deref(), Some("RT-OLD"));
}

#[test]
fn disconnecting_the_provider_signs_out_and_revokes() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    signed_in_record(&store, 3600, 10);
    let svc = service_over(&net, &store);
    svc.refresh(PROVIDER_ID).unwrap();
    svc.clear_key(PROVIDER_ID).unwrap();
    let revoke = net.posts_to(REVOKE_URI);
    assert_eq!(revoke.len(), 1);
    assert_eq!(revoke[0]["token"], "RT-OLD");
    assert!(!summary_of(&svc).has_key);
    assert!(
        svc.catalog(PROVIDER_ID).is_none(),
        "the cached list goes too"
    );
    assert!(
        store::load(&*store).unwrap().unwrap().tokens.is_none(),
        "tokens are gone, the registration stays for the next sign-in"
    );
}

#[test]
fn the_service_signs_in_end_to_end() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    let svc = service_over(&net, &store);
    let signin = svc.chatgpt_begin_sign_in(false, false).unwrap();
    contract_endpoint(&net, &signin, "sub-1", PLAN_ALL);
    let cancel = AtomicBool::new(false);
    let done = std::thread::scope(|sc| {
        sc.spawn(|| {
            browse(
                signin.authorize_url(),
                &format!("&client_id={ISSUED}"),
                "CODE-1",
            )
        });
        svc.chatgpt_complete(&signin, Duration::from_secs(5), &cancel)
    })
    .unwrap();
    assert!(done.plan_usage);
    assert_eq!(svc.get_key(PROVIDER_ID).unwrap().as_deref(), Some("AT-1"));
    let status = svc.chatgpt_status().unwrap();
    assert!(status.signed_in && status.plan_usage);
    let report = svc.chatgpt_sign_out().unwrap();
    assert!(report.revoked);
    assert!(!svc.chatgpt_status().unwrap().signed_in);
}

// ---- background sign-ins (the desktop app and its sidecar) ---------------------

use super::launch::{SignInSessions, SignInState};

/// What a launcher's browser does with the launch URL: fetch it and read the
/// redirect it answers with.
fn follow_launch(launch: &str) -> Option<String> {
    let u = url::Url::parse(launch).unwrap();
    let port = u.port().unwrap();
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(
        s,
        "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n",
        u.path()
    )
    .unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out.lines()
        .find_map(|l| l.strip_prefix("Location: "))
        .map(str::to_string)
}

fn wait_state(
    sessions: &SignInSessions,
    id: &str,
    done: impl Fn(&SignInState) -> bool,
) -> SignInState {
    let start = std::time::Instant::now();
    loop {
        let state = sessions.poll(id).expect("the attempt is the current one");
        if done(&state) {
            return state;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "stuck in {state:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn start_over(
    sessions: &SignInSessions,
    net: &Arc<Net>,
    svc: &Arc<ProviderService<Arc<MemorySecretStore>, Shared>>,
    timeout: Duration,
) -> super::launch::SignInTicket {
    let (begin_svc, run_svc, net) = (svc.clone(), svc.clone(), net.clone());
    sessions
        .start(
            move || begin_svc.chatgpt_begin_sign_in(false, false),
            move |signin, cancel| {
                contract_endpoint(&net, signin, "sub-1", PLAN_ALL);
                run_svc.chatgpt_complete(signin, timeout, cancel)
            },
            timeout,
            false,
        )
        .unwrap()
}

#[test]
fn a_background_sign_in_runs_through_the_launch_link_and_reports_done() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    let svc = Arc::new(service_over(&net, &store));
    let sessions = SignInSessions::new();
    let ticket = start_over(&sessions, &net, &svc, Duration::from_secs(10));

    assert!(ticket.launch_url.starts_with("http://127.0.0.1:"));
    assert!(ticket.registering && !ticket.opened);
    assert_eq!(
        sessions.poll(&ticket.attempt),
        Some(SignInState::Waiting),
        "nothing has come back yet"
    );
    // what a browser does: open the launch link, follow the redirect, sign
    // in, and come back to the loopback callback
    let authorize = follow_launch(&ticket.launch_url).expect("the launch link redirects");
    assert!(authorize.starts_with(AUTHORIZE_URL));
    assert!(
        follow_launch(&ticket.launch_url).is_none(),
        "the launch link works once"
    );
    browse(&authorize, &format!("&client_id={ISSUED}"), "CODE-1");

    let state = wait_state(&sessions, &ticket.attempt, |s| *s != SignInState::Waiting);
    assert_eq!(
        state,
        SignInState::Done {
            email: Some("alex@example.com".into()),
            plan_usage: true
        }
    );
    assert_eq!(svc.get_key(PROVIDER_ID).unwrap().as_deref(), Some("AT-1"));
    // nothing credential-shaped is in what the caller can serialize
    let shown = serde_json::to_string(&(&ticket, &state)).unwrap();
    for secret in ["AT-1", "RT-1", "CODE-1", "id_token", "hint"] {
        assert!(!shown.contains(secret), "{secret} in {shown}");
    }
    assert_eq!(sessions.poll("someone-elses-attempt"), None);
}

#[test]
fn a_declined_background_sign_in_reports_why_and_keeps_nothing() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    let svc = Arc::new(service_over(&net, &store));
    let sessions = SignInSessions::new();
    let ticket = start_over(&sessions, &net, &svc, Duration::from_secs(10));
    let authorize = follow_launch(&ticket.launch_url).unwrap();
    let redirect = param(&authorize, "redirect_uri").unwrap();
    let state = param(&authorize, "state").unwrap();
    let port = url::Url::parse(&redirect).unwrap().port().unwrap();
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    let _ = write!(
        s,
        "GET /auth/callback?error=access_denied&state={state} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"
    );
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);

    match wait_state(&sessions, &ticket.attempt, |s| *s != SignInState::Waiting) {
        SignInState::Failed {
            error: ProviderError::Credential(m),
        } => assert!(m.contains("declined"), "{m}"),
        other => panic!("{other:?}"),
    }
    assert!(store::load(&*store).unwrap().is_none());
    assert!(net.posts.lock().unwrap().is_empty());
}

#[test]
fn cancelling_stops_the_wait_and_a_new_attempt_replaces_the_old_one() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    let svc = Arc::new(service_over(&net, &store));
    let sessions = SignInSessions::new();

    let first = start_over(&sessions, &net, &svc, Duration::from_secs(60));
    assert!(sessions.cancel(&first.attempt));
    assert_eq!(
        wait_state(&sessions, &first.attempt, |s| *s != SignInState::Waiting),
        SignInState::Cancelled
    );
    assert!(!sessions.cancel("not-an-attempt"));

    let second = start_over(&sessions, &net, &svc, Duration::from_secs(60));
    let third = start_over(&sessions, &net, &svc, Duration::from_secs(60));
    assert_ne!(second.attempt, third.attempt);
    assert_eq!(
        sessions.poll(&second.attempt),
        None,
        "only the newest attempt is tracked"
    );
    assert!(sessions.cancel(&third.attempt));
    wait_state(&sessions, &third.attempt, |s| *s == SignInState::Cancelled);
}

// ---- switching to a different ChatGPT account ---------------------------------

#[test]
fn a_different_account_registers_afresh_and_replaces_the_saved_one() {
    let net = fake();
    let store = MemorySecretStore::new();
    // account sub-1 signed in once, then signed out: registration kept, tokens gone
    signed_in_record(&store, 3600, 10);
    store::clear_tokens(&store).unwrap();
    let host = store::host_id(&store).unwrap();

    // the same-account path cannot be used by someone else
    let same = begin(&store, false).unwrap();
    contract_endpoint(&net, &same, "sub-2", PLAN_ALL);
    assert_eq!(
        run(&net, &store, &same, &format!("&client_id={ISSUED}")),
        Err(OAuthError::AccountMismatch)
    );
    assert!(
        OAuthError::AccountMismatch
            .to_string()
            .contains("different account"),
        "the message says how to switch"
    );

    // the different-account path starts from scratch
    let fresh = begin_with(&store, false, true).unwrap();
    assert!(fresh.is_registration());
    assert_eq!(
        param(fresh.authorize_url(), "client_id").as_deref(),
        Some(DYNAMIC_CLIENT_ID),
        "not the saved client"
    );
    assert_eq!(
        param(fresh.authorize_url(), "agent_name_hint").as_deref(),
        Some(AGENT_NAME)
    );
    for hint in ["id_token_hint", "login_hint"] {
        assert!(
            param(fresh.authorize_url(), hint).is_none(),
            "{hint} would steer the sign-in back to the old account"
        );
    }
    assert_eq!(
        param(fresh.authorize_url(), "ext_agent_host_id").unwrap(),
        host
    );
    // a new account's sign-in needs its own issued client id
    let net2 = fake();
    net2.on_token({
        let challenge = param(fresh.authorize_url(), "code_challenge").unwrap();
        let nonce = param(fresh.authorize_url(), "nonce").unwrap();
        let redirect = fresh.redirect_uri().to_string();
        let idp = net2.idp.clone();
        move |f| {
            assert_eq!(f["client_id"], "oaiapp_second");
            assert_eq!(pkce_challenge(&f["code_verifier"]), challenge);
            assert_eq!(f["redirect_uri"], redirect);
            let id = idp.sign(&id_claims("oaiapp_second", &nonce, "sub-2"));
            (
                200,
                json!({"access_token":"AT-2","refresh_token":"RT-2","id_token":id,
                    "token_type":"Bearer","expires_in":3600,"scope":PLAN_ALL})
                .to_string(),
            )
        }
    });
    let done = run(&net2, &store, &fresh, "&client_id=oaiapp_second").unwrap();
    assert!(done.plan_usage);
    let rec = store::load(&store).unwrap().unwrap();
    assert_eq!(rec.registration.client_id, "oaiapp_second");
    assert_eq!(rec.registration.subject, "sub-2");
    assert_eq!(
        rec.ext_agent_host_id, host,
        "the host id is this computer's, not the account's"
    );
}

#[test]
fn a_cancelled_account_switch_keeps_the_saved_account() {
    let store = MemorySecretStore::new();
    signed_in_record(&store, 3600, 10);
    store::clear_tokens(&store).unwrap();
    let before = store::load(&store).unwrap().unwrap();
    let attempt = begin_with(&store, false, true).unwrap();
    drop(attempt); // abandoned: nothing was saved
    let after = store::load(&store).unwrap().unwrap();
    assert_eq!(after.registration.client_id, before.registration.client_id);
    assert_eq!(after.registration.subject, before.registration.subject);
}

#[test]
fn the_service_refuses_a_different_account_while_signed_in() {
    let net = fake();
    let store = Arc::new(MemorySecretStore::new());
    signed_in_record(&store, 3600, 10);
    let svc = service_over(&net, &store);
    let err = svc
        .chatgpt_begin_sign_in(false, true)
        .err()
        .expect("refused");
    assert!(
        matches!(&err, ProviderError::Credential(m) if m.contains("sign out")),
        "{err}"
    );
    // after signing out it is allowed
    svc.chatgpt_sign_out().unwrap();
    assert!(svc.chatgpt_begin_sign_in(false, true).is_ok());
}
