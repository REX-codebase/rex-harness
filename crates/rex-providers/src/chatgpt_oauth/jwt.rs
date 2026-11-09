//! Verifying the ID token OpenAI issues at sign-in.
//!
//! OpenAI's contract (Sign in with ChatGPT, *Registration and sign-in*,
//! step 4) is to verify the signature against the issuer's JWKS, then check
//! the issuer, the audience against the issued client id, the expiry and the
//! nonce this attempt sent, and to use the validated `sub` as the account's
//! identity. This module does exactly that and nothing cleverer.
//!
//! It fails closed. Only the algorithms a JWKS can sensibly publish for
//! signing are accepted (RS256, ES256, EdDSA); `none` and every HMAC
//! algorithm are refused outright, which also closes the classic "sign with
//! the public key as an HMAC secret" confusion. A token whose `kid` is not in
//! the key set is refused rather than tried against some other key, and a
//! key whose type does not fit the header's algorithm is never used.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use ring::signature::{self, RsaPublicKeyComponents, UnparsedPublicKey};
use serde_json::Value;

/// Clock skew tolerated on `exp`, `iat` and `nbf`, in seconds. The docs ask
/// for a small tolerance; their own example uses five.
pub const LEEWAY_SECS: u64 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JwtError {
    Malformed(&'static str),
    UnsupportedAlgorithm(String),
    UnknownKey,
    BadSignature,
    Claim(String),
}

impl std::fmt::Display for JwtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JwtError::Malformed(w) => write!(f, "the ID token is malformed ({w})"),
            JwtError::UnsupportedAlgorithm(a) => {
                write!(
                    f,
                    "the ID token is signed with an unsupported algorithm ({a})"
                )
            }
            JwtError::UnknownKey => {
                write!(f, "the ID token's signing key is not in OpenAI's key set")
            }
            JwtError::BadSignature => write!(f, "the ID token's signature did not verify"),
            JwtError::Claim(c) => write!(f, "the ID token failed validation: {c}"),
        }
    }
}

/// What a valid ID token must be.
pub struct Expect<'a> {
    pub issuer: &'a str,
    /// The issued client id the token must be addressed to.
    pub audience: &'a str,
    /// The nonce this sign-in attempt sent.
    pub nonce: &'a str,
    /// Now, in Unix seconds.
    pub now: u64,
}

fn b64(part: &str, what: &'static str) -> Result<Vec<u8>, JwtError> {
    URL_SAFE_NO_PAD
        .decode(part.trim_end_matches('='))
        .map_err(|_| JwtError::Malformed(what))
}

fn json_of(bytes: &[u8], what: &'static str) -> Result<Value, JwtError> {
    serde_json::from_slice(bytes).map_err(|_| JwtError::Malformed(what))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Alg {
    Rs256,
    Es256,
    EdDsa,
}

impl Alg {
    fn parse(name: &str) -> Result<Alg, JwtError> {
        match name {
            "RS256" => Ok(Alg::Rs256),
            "ES256" => Ok(Alg::Es256),
            "EdDSA" => Ok(Alg::EdDsa),
            other => Err(JwtError::UnsupportedAlgorithm(
                other.chars().take(16).collect(),
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Alg::Rs256 => "RS256",
            Alg::Es256 => "ES256",
            Alg::EdDsa => "EdDSA",
        }
    }

    /// The JWK `kty` a key must have to be used with this algorithm.
    fn kty(self) -> &'static str {
        match self {
            Alg::Rs256 => "RSA",
            Alg::Es256 => "EC",
            Alg::EdDsa => "OKP",
        }
    }
}

/// The key to verify with: matched by `kid`, signing-capable, of the right
/// type, and (when the JWK states one) for the same algorithm.
fn pick_key<'a>(jwks: &'a Value, kid: Option<&str>, alg: Alg) -> Result<&'a Value, JwtError> {
    let keys = jwks
        .get("keys")
        .and_then(Value::as_array)
        .ok_or(JwtError::UnknownKey)?;
    let usable = |k: &&Value| {
        k.get("kty").and_then(Value::as_str) == Some(alg.kty())
            && k.get("use")
                .and_then(Value::as_str)
                .is_none_or(|u| u == "sig")
            && k.get("alg")
                .and_then(Value::as_str)
                .is_none_or(|a| a == alg.name())
    };
    match kid {
        Some(kid) => keys
            .iter()
            .filter(|k| k.get("kid").and_then(Value::as_str) == Some(kid))
            .find(usable)
            .ok_or(JwtError::UnknownKey),
        // no kid: only when exactly one key could have signed it
        None => {
            let mut candidates = keys.iter().filter(usable);
            match (candidates.next(), candidates.next()) {
                (Some(k), None) => Ok(k),
                _ => Err(JwtError::UnknownKey),
            }
        }
    }
}

fn jwk_bytes(key: &Value, field: &str) -> Result<Vec<u8>, JwtError> {
    let s = key
        .get(field)
        .and_then(Value::as_str)
        .ok_or(JwtError::Malformed("key field"))?;
    b64(s, "key field")
}

/// Check `signature` over `message` with the JWK `key`.
fn verify_signature(alg: Alg, key: &Value, message: &[u8], sig: &[u8]) -> Result<(), JwtError> {
    let ok = match alg {
        Alg::Rs256 => {
            let (n, e) = (jwk_bytes(key, "n")?, jwk_bytes(key, "e")?);
            RsaPublicKeyComponents { n: &n, e: &e }
                .verify(&signature::RSA_PKCS1_2048_8192_SHA256, message, sig)
                .is_ok()
        }
        Alg::Es256 => {
            if key.get("crv").and_then(Value::as_str) != Some("P-256") {
                return Err(JwtError::UnknownKey);
            }
            let (x, y) = (jwk_bytes(key, "x")?, jwk_bytes(key, "y")?);
            if x.len() != 32 || y.len() != 32 {
                return Err(JwtError::Malformed("EC key size"));
            }
            let mut point = Vec::with_capacity(65);
            point.push(0x04);
            point.extend_from_slice(&x);
            point.extend_from_slice(&y);
            UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_FIXED, point)
                .verify(message, sig)
                .is_ok()
        }
        Alg::EdDsa => {
            if key.get("crv").and_then(Value::as_str) != Some("Ed25519") {
                return Err(JwtError::UnknownKey);
            }
            let x = jwk_bytes(key, "x")?;
            UnparsedPublicKey::new(&signature::ED25519, x)
                .verify(message, sig)
                .is_ok()
        }
    };
    if ok {
        Ok(())
    } else {
        Err(JwtError::BadSignature)
    }
}

fn claim_err(msg: &str) -> JwtError {
    JwtError::Claim(msg.to_string())
}

fn check_claims(claims: &Value, expect: &Expect) -> Result<(), JwtError> {
    if claims.get("iss").and_then(Value::as_str) != Some(expect.issuer) {
        return Err(claim_err("issuer is not OpenAI's"));
    }
    let aud_ok = match claims.get("aud") {
        Some(Value::String(a)) => a == expect.audience,
        Some(Value::Array(list)) => list.iter().any(|a| a.as_str() == Some(expect.audience)),
        _ => false,
    };
    if !aud_ok {
        return Err(claim_err("audience is not this client"));
    }
    let exp = claims
        .get("exp")
        .and_then(Value::as_u64)
        .ok_or_else(|| claim_err("exp is missing"))?;
    if expect.now > exp.saturating_add(LEEWAY_SECS) {
        return Err(claim_err("the token has expired"));
    }
    let iat = claims
        .get("iat")
        .and_then(Value::as_u64)
        .ok_or_else(|| claim_err("iat is missing"))?;
    if iat > expect.now.saturating_add(LEEWAY_SECS) {
        return Err(claim_err("the token was issued in the future"));
    }
    if let Some(nbf) = claims.get("nbf").and_then(Value::as_u64) {
        if nbf > expect.now.saturating_add(LEEWAY_SECS) {
            return Err(claim_err("the token is not valid yet"));
        }
    }
    if claims.get("nonce").and_then(Value::as_str) != Some(expect.nonce) {
        return Err(claim_err("the nonce does not match this sign-in"));
    }
    match claims.get("sub").and_then(Value::as_str) {
        Some(s) if !s.is_empty() => {}
        _ => return Err(claim_err("the subject is missing")),
    }
    if let Some(sid) = claims.get("sid") {
        if !sid.as_str().is_some_and(|s| !s.trim().is_empty()) {
            return Err(claim_err("the session identifier is invalid"));
        }
    }
    Ok(())
}

/// Verify `token` against `jwks` and `expect`, returning its claims.
pub fn verify(token: &str, jwks: &Value, expect: &Expect) -> Result<Value, JwtError> {
    let mut parts = token.split('.');
    let (Some(h), Some(p), Some(s), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(JwtError::Malformed("not three parts"));
    };
    let header = json_of(&b64(h, "header")?, "header")?;
    let alg = Alg::parse(
        header
            .get("alg")
            .and_then(Value::as_str)
            .ok_or(JwtError::Malformed("no alg"))?,
    )?;
    let key = pick_key(jwks, header.get("kid").and_then(Value::as_str), alg)?;
    let signature = b64(s, "signature")?;
    verify_signature(alg, key, format!("{h}.{p}").as_bytes(), &signature)?;
    let claims = json_of(&b64(p, "payload")?, "payload")?;
    check_claims(&claims, expect)?;
    Ok(claims)
}

#[cfg(test)]
#[path = "rs256_vector.rs"]
mod rs256_vector;

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{EcdsaKeyPair, Ed25519KeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING};
    use serde_json::json;

    const ISS: &str = "https://auth.openai.com";
    const AUD: &str = "oaiapp_test";
    const NONCE: &str = "nonce-1";
    const NOW: u64 = 1_790_000_000;

    fn enc(v: &Value) -> String {
        URL_SAFE_NO_PAD.encode(v.to_string())
    }

    fn claims() -> Value {
        json!({"iss":ISS,"sub":"user-1","aud":[AUD],"nonce":NONCE,"iat":NOW-10,"exp":NOW+3600,
            "sid":"sid_1","email":"a@example.com"})
    }

    fn expect() -> Expect<'static> {
        Expect {
            issuer: ISS,
            audience: AUD,
            nonce: NONCE,
            now: NOW,
        }
    }

    struct Es {
        pair: EcdsaKeyPair,
        jwks: Value,
    }

    fn es_key(kid: &str) -> Es {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let pair = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
            .unwrap();
        let point = pair.public_key().as_ref().to_vec();
        let jwks = json!({"keys":[{"kty":"EC","crv":"P-256","kid":kid,"use":"sig","alg":"ES256",
            "x": URL_SAFE_NO_PAD.encode(&point[1..33]),"y": URL_SAFE_NO_PAD.encode(&point[33..65])}]});
        Es { pair, jwks }
    }

    fn es_token(k: &Es, header: Value, claims: &Value) -> String {
        let signing = format!("{}.{}", enc(&header), enc(claims));
        let sig = k
            .pair
            .sign(&SystemRandom::new(), signing.as_bytes())
            .unwrap();
        format!("{signing}.{}", URL_SAFE_NO_PAD.encode(sig.as_ref()))
    }

    fn good_header() -> Value {
        json!({"alg":"ES256","kid":"k1","typ":"JWT"})
    }

    #[test]
    fn a_valid_es256_token_verifies_and_returns_its_claims() {
        let k = es_key("k1");
        let t = es_token(&k, good_header(), &claims());
        let c = verify(&t, &k.jwks, &expect()).unwrap();
        assert_eq!(c["sub"], "user-1");
        assert_eq!(c["email"], "a@example.com");
    }

    #[test]
    fn a_valid_eddsa_token_verifies() {
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let jwks = json!({"keys":[{"kty":"OKP","crv":"Ed25519","kid":"e1",
            "x": URL_SAFE_NO_PAD.encode(pair.public_key().as_ref())}]});
        let signing = format!(
            "{}.{}",
            enc(&json!({"alg":"EdDSA","kid":"e1"})),
            enc(&claims())
        );
        let t = format!(
            "{signing}.{}",
            URL_SAFE_NO_PAD.encode(pair.sign(signing.as_bytes()).as_ref())
        );
        assert!(verify(&t, &jwks, &expect()).is_ok());
    }

    #[test]
    fn rs256_verifies_against_a_signature_from_an_independent_implementation() {
        use super::rs256_vector::*;
        let key = json!({"kty":"RSA","n":N,"e":E,"kid":"r1","use":"sig"});
        let sig = URL_SAFE_NO_PAD.decode(SIGNATURE).unwrap();
        assert!(verify_signature(Alg::Rs256, &key, MESSAGE.as_bytes(), &sig).is_ok());
        // any change to the message or the signature breaks it
        assert_eq!(
            verify_signature(Alg::Rs256, &key, b"rex-rs256-test-vectoR", &sig),
            Err(JwtError::BadSignature)
        );
        let mut bad = sig.clone();
        bad[10] ^= 1;
        assert_eq!(
            verify_signature(Alg::Rs256, &key, MESSAGE.as_bytes(), &bad),
            Err(JwtError::BadSignature)
        );
    }

    #[test]
    fn a_tampered_token_does_not_verify() {
        let k = es_key("k1");
        let t = es_token(&k, good_header(), &claims());
        let mut parts: Vec<&str> = t.split('.').collect();
        let forged = enc(
            &json!({"iss":ISS,"sub":"someone-else","aud":[AUD],"nonce":NONCE,"iat":NOW-10,"exp":NOW+3600}),
        );
        parts[1] = &forged;
        assert_eq!(
            verify(&parts.join("."), &k.jwks, &expect()),
            Err(JwtError::BadSignature)
        );
        // a token signed by some other key is refused too
        let other = es_key("k1");
        let t2 = es_token(&other, good_header(), &claims());
        assert_eq!(verify(&t2, &k.jwks, &expect()), Err(JwtError::BadSignature));
    }

    #[test]
    fn unsupported_and_dangerous_algorithms_are_refused() {
        let k = es_key("k1");
        for alg in ["none", "HS256", "HS512", "RS512", "ES384", "PS256", ""] {
            let t = format!(
                "{}.{}.{}",
                enc(&json!({"alg":alg,"kid":"k1"})),
                enc(&claims()),
                URL_SAFE_NO_PAD.encode(b"x")
            );
            assert!(
                matches!(
                    verify(&t, &k.jwks, &expect()),
                    Err(JwtError::UnsupportedAlgorithm(_))
                ),
                "{alg}"
            );
        }
        // an unsigned token with an empty signature part
        let t = format!("{}.{}.", enc(&json!({"alg":"none"})), enc(&claims()));
        assert!(verify(&t, &k.jwks, &expect()).is_err());
    }

    #[test]
    fn a_key_of_the_wrong_type_is_never_used_for_the_header_algorithm() {
        // the header says RS256, the only key is an EC key: not tried
        let k = es_key("k1");
        let t = format!(
            "{}.{}.{}",
            enc(&json!({"alg":"RS256","kid":"k1"})),
            enc(&claims()),
            URL_SAFE_NO_PAD.encode(b"x")
        );
        assert_eq!(verify(&t, &k.jwks, &expect()), Err(JwtError::UnknownKey));
        // a JWK that states a different algorithm is not used either
        let mut jwks = k.jwks.clone();
        jwks["keys"][0]["alg"] = json!("RS256");
        let t = es_token(&k, good_header(), &claims());
        assert_eq!(verify(&t, &jwks, &expect()), Err(JwtError::UnknownKey));
        // nor a key not meant for signatures
        let mut jwks = k.jwks.clone();
        jwks["keys"][0]["use"] = json!("enc");
        assert_eq!(verify(&t, &jwks, &expect()), Err(JwtError::UnknownKey));
    }

    #[test]
    fn an_unknown_kid_is_refused_rather_than_tried_against_another_key() {
        let k = es_key("k1");
        let t = es_token(&k, json!({"alg":"ES256","kid":"rotated-away"}), &claims());
        assert_eq!(verify(&t, &k.jwks, &expect()), Err(JwtError::UnknownKey));
    }

    #[test]
    fn without_a_kid_only_an_unambiguous_key_is_used() {
        let k = es_key("k1");
        let t = es_token(&k, json!({"alg":"ES256"}), &claims());
        assert!(verify(&t, &k.jwks, &expect()).is_ok());
        let mut two = k.jwks.clone();
        let dup = two["keys"][0].clone();
        two["keys"].as_array_mut().unwrap().push(dup);
        assert_eq!(verify(&t, &two, &expect()), Err(JwtError::UnknownKey));
    }

    #[test]
    fn every_claim_check_rejects_what_it_should() {
        let k = es_key("k1");
        let check = |mutate: &dyn Fn(&mut Value)| {
            let mut c = claims();
            mutate(&mut c);
            verify(&es_token(&k, good_header(), &c), &k.jwks, &expect())
        };
        assert!(check(&|c| c["iss"] = json!("https://evil.example")).is_err());
        assert!(check(&|c| c["aud"] = json!(["oaiapp_other"])).is_err());
        assert!(check(&|c| c["aud"] = json!("oaiapp_other")).is_err());
        assert!(check(&|c| c["nonce"] = json!("replayed")).is_err());
        assert!(check(&|c| {
            c.as_object_mut().unwrap().remove("nonce");
        })
        .is_err());
        assert!(check(&|c| c["exp"] = json!(NOW - 60)).is_err());
        assert!(check(&|c| c["iat"] = json!(NOW + 600)).is_err());
        assert!(check(&|c| c["nbf"] = json!(NOW + 600)).is_err());
        assert!(check(&|c| c["sub"] = json!("")).is_err());
        assert!(check(&|c| {
            c.as_object_mut().unwrap().remove("sub");
        })
        .is_err());
        assert!(check(&|c| c["sid"] = json!("  ")).is_err());
        assert!(check(&|c| c["sid"] = json!(42)).is_err());
        assert!(check(&|c| {
            c.as_object_mut().unwrap().remove("exp");
        })
        .is_err());
        assert!(check(&|c| {
            c.as_object_mut().unwrap().remove("iat");
        })
        .is_err());
        // a plain string audience is accepted when it is ours
        assert!(check(&|c| c["aud"] = json!(AUD)).is_ok());
        // sid is optional
        assert!(check(&|c| {
            c.as_object_mut().unwrap().remove("sid");
        })
        .is_ok());
    }

    #[test]
    fn a_few_seconds_of_clock_skew_are_tolerated_and_no_more() {
        let k = es_key("k1");
        let at = |exp: u64| {
            let mut c = claims();
            c["exp"] = json!(exp);
            verify(&es_token(&k, good_header(), &c), &k.jwks, &expect())
        };
        assert!(at(NOW - LEEWAY_SECS).is_ok());
        assert!(at(NOW - LEEWAY_SECS - 1).is_err());
    }

    #[test]
    fn malformed_tokens_are_errors_not_panics() {
        let k = es_key("k1");
        for bad in [
            "",
            "a",
            "a.b",
            "a.b.c.d",
            "!!.!!.!!",
            "e30.e30.e30",
            ".e30.e30",
        ] {
            assert!(verify(bad, &k.jwks, &expect()).is_err(), "{bad:?}");
        }
        // a token with a header that is not JSON
        let t = format!(
            "{}.{}.{}",
            URL_SAFE_NO_PAD.encode("nope"),
            enc(&claims()),
            "AA"
        );
        assert!(matches!(
            verify(&t, &k.jwks, &expect()),
            Err(JwtError::Malformed(_))
        ));
        // a key set that is not a key set
        let t = es_token(&k, good_header(), &claims());
        assert_eq!(verify(&t, &json!({}), &expect()), Err(JwtError::UnknownKey));
        assert_eq!(
            verify(&t, &json!({"keys":"x"}), &expect()),
            Err(JwtError::UnknownKey)
        );
    }
}
