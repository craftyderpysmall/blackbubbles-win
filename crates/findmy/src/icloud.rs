//! iCloud web sign-in and the Find My device list (the same API icloud.com uses).
//!
//! `interactive_login` runs once (it needs the 2FA code). After that the session token
//! and trust token are reused; `Session::devices` re-validates automatically:
//! 1. `validate` with the iCloud web cookies (they rotate; we save them after every call),
//! 2. `accountLogin` with the Apple ID session token,
//! 3. if the owner chose to remember the password, a fresh SRP sign-in with the trust
//!    token, which Apple accepts without a new 2FA code while this "browser" is trusted.
//!    At most once per [`SIGNIN_EVERY`], and a rejected password is forgotten at once, so a
//!    changed password can't lock the account.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use reqwest::blocking::{Client, RequestBuilder, Response};
use reqwest_cookie_store::CookieStoreMutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::srp;

const AUTH: &str = "https://idmsa.apple.com/appleauth/auth";
const SETUP: &str = "https://setup.icloud.com/setup/ws/1";
const WIDGET_KEY: &str = "d39ba9916b7251055b22c7f910e2ea796ee65e98b2ddecea8f5dde8d9d1a815d";
const BUILD: &str = "clientBuildNumber=2420Project47&clientMasteringNumber=2420B16";
/// Minimum gap between automatic password sign-ins.
pub const SIGNIN_EVERY: std::time::Duration = std::time::Duration::from_secs(10 * 60);
static LAST_SIGNIN: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// The Apple ID and password, kept (mode 600) only if the owner said yes in `findmy-login`.
#[derive(Serialize, Deserialize)]
struct Credentials {
    apple_id: String,
    password: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
struct State {
    client_id: String,
    session_token: Option<String>,
    trust_token: Option<String>,
    session_id: Option<String>,
    scnt: Option<String>,
    account_country: Option<String>,
    dsid: Option<String>,
    findme_url: Option<String>,
}

pub struct Session {
    dir: PathBuf,
    http: Client,
    cookies: Arc<CookieStoreMutex>,
    state: State,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub model: String,
    pub battery: Option<f32>,
    pub location: Option<(f64, f64, f32, i64)>,
}

impl Session {
    pub fn open(dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(dir)?;
        let state: State = std::fs::read(dir.join("session.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| State { client_id: format!("auth-{}", uuid::Uuid::new_v4()), ..State::default() });
        #[allow(deprecated)]
        let store = std::fs::File::open(dir.join("cookies.json"))
            .ok()
            .and_then(|f| reqwest_cookie_store::CookieStore::load_json(std::io::BufReader::new(f)).ok())
            .unwrap_or_default();
        let cookies = Arc::new(CookieStoreMutex::new(store));
        let http = Client::builder()
            .cookie_provider(cookies.clone())
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15")
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self { dir: dir.to_path_buf(), http, cookies, state })
    }

    pub fn is_signed_in(&self) -> bool {
        self.state.session_token.is_some()
    }

    fn save(&self) -> Result<()> {
        crate::write_private(&self.dir.join("session.json"), &serde_json::to_vec_pretty(&self.state)?)?;
        let mut buf = Vec::new();
        #[allow(deprecated)]
        self.cookies.lock().unwrap().save_json(&mut buf).map_err(|e| anyhow::anyhow!("{e}"))?;
        crate::write_private(&self.dir.join("cookies.json"), &buf)
    }

    fn auth_request(&self, builder: RequestBuilder) -> RequestBuilder {
        let mut b = builder
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("Origin", "https://www.icloud.com")
            .header("Referer", "https://www.icloud.com/")
            .header("X-Apple-OAuth-Client-Id", WIDGET_KEY)
            .header("X-Apple-OAuth-Client-Type", "firstPartyAuth")
            .header("X-Apple-OAuth-Redirect-URI", "https://www.icloud.com")
            .header("X-Apple-OAuth-Require-Grant-Code", "true")
            .header("X-Apple-OAuth-Response-Mode", "web_message")
            .header("X-Apple-OAuth-Response-Type", "code")
            .header("X-Apple-OAuth-State", &self.state.client_id)
            .header("X-Apple-Widget-Key", WIDGET_KEY);
        if let Some(scnt) = &self.state.scnt {
            b = b.header("scnt", scnt);
        }
        if let Some(id) = &self.state.session_id {
            b = b.header("X-Apple-ID-Session-Id", id);
        }
        b
    }

    fn absorb_headers(&mut self, response: &Response) {
        let get = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
        if let Some(v) = get("X-Apple-ID-Account-Country") {
            self.state.account_country = Some(v);
        }
        if let Some(v) = get("X-Apple-ID-Session-Id") {
            self.state.session_id = Some(v);
        }
        if let Some(v) = get("X-Apple-Session-Token") {
            self.state.session_token = Some(v);
        }
        if let Some(v) = get("X-Apple-TwoSV-Trust-Token") {
            self.state.trust_token = Some(v);
        }
        if let Some(v) = get("scnt") {
            self.state.scnt = Some(v);
        }
    }

    /// Returns true when Apple wants a 2FA code.
    fn sign_in(&mut self, apple_id: &str, password: &str) -> Result<bool> {
        let client = srp::Client::new(apple_id);
        let reply = self
            .auth_request(self.http.post(format!("{AUTH}/signin/init")))
            .json(&json!({ "a": B64.encode(client.public_a()), "accountName": apple_id, "protocols": ["s2k", "s2k_fo"] }))
            .send()?;
        self.absorb_headers(&reply);
        let init: Value = reply.error_for_status()?.json()?;
        let field = |k: &str| init.get(k).and_then(Value::as_str).with_context(|| format!("signin/init lacks {k}"));
        let salt = B64.decode(field("salt")?)?;
        let b = B64.decode(field("b")?)?;
        let iterations = init.get("iteration").and_then(Value::as_u64).context("signin/init lacks iteration")? as u32;
        let key = srp::derive_password(password, &salt, iterations, srp::Protocol::parse(field("protocol")?)?);
        let proof = client.process_challenge(&salt, &b, &key)?;

        let trust: Vec<String> = self.state.trust_token.iter().cloned().collect();
        let reply = self
            .auth_request(self.http.post(format!("{AUTH}/signin/complete?isRememberMeEnabled=true")))
            .json(&json!({
                "accountName": apple_id,
                "rememberMe": true,
                "trustTokens": trust,
                "m1": B64.encode(proof.m1),
                "c": field("c")?,
                "m2": B64.encode(proof.m2),
            }))
            .send()?;
        self.absorb_headers(&reply);
        match reply.status().as_u16() {
            200 => Ok(false),
            409 => Ok(true),
            401 | 403 => bail!("Apple rejected the Apple ID or password"),
            code => bail!("sign-in failed with HTTP {code}: {}", reply.text().unwrap_or_default()),
        }
    }

    /// Ask Apple for the 2FA state. This is what makes it push a code to trusted devices;
    /// the reply also lists the trusted phone numbers for the SMS fallback.
    /// Also says whether the account has no device that can show a code (then only SMS works).
    fn request_push(&mut self) -> Result<(Vec<TrustedPhone>, bool)> {
        let reply = self.auth_request(self.http.get(AUTH)).send().context("asking Apple to send the code")?;
        self.absorb_headers(&reply);
        let status = reply.status();
        let body: Value = reply.json().unwrap_or(Value::Null);
        if !status.is_success() {
            bail!("Apple didn't send a code (HTTP {status}): {body}");
        }
        let no_devices = body.get("noTrustedDevices").and_then(Value::as_bool).unwrap_or(false);
        let phones = trusted_phones(&body);
        if phones.is_empty() {
            let keys: Vec<&str> = body.as_object().map(|o| o.keys().map(String::as_str).collect()).unwrap_or_default();
            eprintln!("note: Apple's reply listed no phone numbers (fields: {keys:?}); `sms` will use your first trusted number.");
        }
        Ok((phones, no_devices))
    }

    fn send_sms(&mut self, phone: &TrustedPhone) -> Result<()> {
        let reply = self
            .auth_request(self.http.put(format!("{AUTH}/verify/phone")))
            .json(&json!({ "phoneNumber": { "id": phone.id }, "mode": "sms" }))
            .send()?;
        self.absorb_headers(&reply);
        if !reply.status().is_success() {
            bail!("Apple couldn't send the text ({})", reply.status());
        }
        Ok(())
    }

    fn verify_code(&mut self, code: &str, sms: Option<&TrustedPhone>) -> Result<()> {
        let request = match sms {
            None => self
                .auth_request(self.http.post(format!("{AUTH}/verify/trusteddevice/securitycode")))
                .json(&json!({ "securityCode": { "code": code.trim() } })),
            Some(phone) => self
                .auth_request(self.http.post(format!("{AUTH}/verify/phone/securitycode")))
                .json(&json!({ "securityCode": { "code": code.trim() }, "phoneNumber": { "id": phone.id }, "mode": "sms" })),
        };
        let reply = request.send()?;
        self.absorb_headers(&reply);
        if !reply.status().is_success() {
            bail!("2FA code was rejected ({})", reply.status());
        }
        let reply = self.auth_request(self.http.get(format!("{AUTH}/2sv/trust"))).send()?;
        self.absorb_headers(&reply);
        reply.error_for_status()?;
        Ok(())
    }

    /// Exchange the session token for iCloud web service URLs.
    fn account_login(&mut self) -> Result<()> {
        let token = self.state.session_token.clone().context("not signed in; run `bbd findmy-login`")?;
        let reply = self
            .http
            .post(format!("{SETUP}/accountLogin?{BUILD}&clientId={}", self.state.client_id))
            .header("Origin", "https://www.icloud.com")
            .header("Referer", "https://www.icloud.com/")
            .json(&json!({
                "accountCountryCode": self.state.account_country,
                "dsWebAuthToken": token,
                "extended_login": true,
                "trustToken": self.state.trust_token.clone().unwrap_or_default(),
            }))
            .send()?;
        if !reply.status().is_success() {
            bail!("iCloud session expired ({}); run `bbd findmy-login` on the Mac again", reply.status());
        }
        self.absorb_account(&reply.json()?)
    }

    /// Re-check the iCloud web cookies (what icloud.com does on every page load).
    fn validate(&mut self) -> Result<()> {
        let reply = self
            .http
            .post(format!("{SETUP}/validate?{BUILD}&clientId={}", self.state.client_id))
            .header("Origin", "https://www.icloud.com")
            .header("Referer", "https://www.icloud.com/")
            .body("null")
            .send()?;
        if !reply.status().is_success() {
            bail!("validate: {}", reply.status());
        }
        self.absorb_account(&reply.json()?)
    }

    fn absorb_account(&mut self, data: &Value) -> Result<()> {
        self.state.dsid = data.pointer("/dsInfo/dsid").and_then(|v| v.as_str().map(str::to_owned).or_else(|| v.as_u64().map(|n| n.to_string())));
        self.state.findme_url = data.pointer("/webservices/findme/url").and_then(Value::as_str).map(str::to_owned);
        if self.state.findme_url.is_none() {
            bail!("this account has no Find My web service");
        }
        self.save()
    }

    fn credentials(&self) -> Option<Credentials> {
        std::fs::read(self.dir.join("credentials.json")).ok().and_then(|b| serde_json::from_slice(&b).ok())
    }

    /// Get a working iCloud web session back without the owner (see the module docs).
    fn reauthenticate(&mut self) -> Result<()> {
        if self.validate().is_ok() {
            tracing::info!("findmy: iCloud session revalidated");
            return Ok(());
        }
        let expired = match self.account_login() {
            Ok(()) => {
                tracing::info!("findmy: iCloud session renewed with the session token");
                return Ok(());
            }
            Err(err) => err,
        };
        let Some(creds) = self.credentials() else { return Err(expired) };
        {
            let mut last = LAST_SIGNIN.lock().unwrap();
            if last.is_some_and(|t| t.elapsed() < SIGNIN_EVERY) {
                return Err(expired.context("signing in again shortly"));
            }
            *last = Some(std::time::Instant::now());
        }
        match self.sign_in(&creds.apple_id, &creds.password) {
            Ok(false) => {}
            Ok(true) => bail!("iCloud wants a new 2FA code (Apple stopped trusting this Mac's sign-in); run `bbd findmy-login` on the Mac again"),
            Err(err) => {
                if format!("{err}").contains("rejected the Apple ID or password") {
                    // Never retry a wrong password: Apple locks the account after a few.
                    let _ = std::fs::remove_file(self.dir.join("credentials.json"));
                    bail!("Apple rejected the saved password (changed?), so it was forgotten; run `bbd findmy-login` on the Mac again");
                }
                return Err(err);
            }
        }
        self.account_login()?;
        tracing::info!("findmy: signed in to iCloud again with the saved password");
        Ok(())
    }

    /// `fmly` is iCloud's switch for including Family Sharing members' devices.
    fn refresh_client(&self, include_family: bool) -> Result<Response> {
        let url = self.state.findme_url.as_deref().context("no Find My URL")?;
        let dsid = self.state.dsid.as_deref().unwrap_or_default();
        Ok(self
            .http
            .post(format!("{url}/fmipservice/client/web/refreshClient?{BUILD}&clientId={}&dsid={dsid}", self.state.client_id))
            .header("Origin", "https://www.icloud.com")
            .json(&json!({ "clientContext": { "fmly": include_family, "shouldLocate": true, "selectedDevice": "all", "deviceListVersion": 1 } }))
            .send()?)
    }

    /// Devices on the account, plus Family Sharing members' when `include_family`.
    pub fn devices(&mut self, include_family: bool) -> Result<Vec<Device>> {
        if self.state.findme_url.is_none() {
            self.reauthenticate()?;
        }
        let mut reply = self.refresh_client(include_family)?;
        if matches!(reply.status().as_u16(), 401 | 421 | 450) {
            self.reauthenticate()?;
            reply = self.refresh_client(include_family)?;
        }
        let data: Value = reply.error_for_status()?.json()?;
        // iCloud rotates its web cookies; keep the newest so a bbd restart can revalidate.
        self.save()?;
        Ok(parse_devices(&data))
    }
}

pub(crate) fn parse_devices(data: &Value) -> Vec<Device> {
    data.get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|d| {
            let s = |k: &str| d.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            let location = d.get("location").filter(|l| !l.is_null()).and_then(|l| {
                Some((
                    l.get("latitude")?.as_f64()?,
                    l.get("longitude")?.as_f64()?,
                    l.get("horizontalAccuracy").and_then(Value::as_f64).unwrap_or(0.0) as f32,
                    l.get("timeStamp").and_then(Value::as_i64).unwrap_or(0),
                ))
            });
            Device {
                id: s("id"),
                name: s("name"),
                model: s("deviceDisplayName"),
                battery: d.get("batteryLevel").and_then(Value::as_f64).filter(|b| *b > 0.0).map(|b| b as f32),
                location,
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrustedPhone {
    pub id: i64,
    /// Masked by Apple, e.g. "+1 (•••) •••-••12".
    pub number: String,
}

fn trusted_phones(auth: &Value) -> Vec<TrustedPhone> {
    auth.get("trustedPhoneNumbers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| {
            Some(TrustedPhone {
                id: p.get("id")?.as_i64()?,
                number: p.get("numberWithDialCode").or_else(|| p.get("obfuscatedNumber")).and_then(Value::as_str).unwrap_or("your phone").to_string(),
            })
        })
        .collect()
}

/// Sign in, asking `ask` for the 2FA code if needed, and save the session.
///
/// `ask(prompt)` reads one line. Answering `sms` texts the code to the first trusted phone
/// number instead of waiting for the device pop-up. `remember` keeps the password (mode 600)
/// so bbd can sign itself back in when iCloud's session expires; otherwise it's forgotten.
pub fn interactive_login(dir: &Path, apple_id: &str, password: &str, remember: bool, mut ask: impl FnMut(&str) -> Result<String>) -> Result<()> {
    let mut session = Session::open(dir)?;
    let saved = dir.join("credentials.json");
    let _ = std::fs::remove_file(&saved);
    if session.sign_in(apple_id, password)? {
        let (phones, no_devices) = match session.request_push() {
            Ok(found) => found,
            Err(err) => {
                eprintln!("warning: {err:#}");
                (vec![], false)
            }
        };
        let mut sms: Option<TrustedPhone> = None;
        if no_devices {
            let phone = phones.first().cloned().unwrap_or(TrustedPhone { id: 1, number: "your trusted phone number".into() });
            eprintln!("Apple says no device can show a code for this account, so it's being texted instead.");
            session.send_sms(&phone)?;
            sms = Some(phone);
        }
        loop {
            let hint = if phones.is_empty() || sms.is_some() { "" } else { ", or type sms to get it by text" };
            let prompt = match &sms {
                Some(p) => format!("Enter the 6-digit code texted to {}: ", p.number),
                None => format!("Enter the 6-digit code from your Apple device{hint}: "),
            };
            let answer = ask(&prompt)?;
            if answer.trim().eq_ignore_ascii_case("sms") && sms.is_none() {
                // Apple numbers trusted phones from 1; use the first if the list didn't parse.
                let phone = phones.first().cloned().unwrap_or(TrustedPhone { id: 1, number: "your trusted phone number".into() });
                session.send_sms(&phone)?;
                sms = Some(phone);
                continue;
            }
            match session.verify_code(&answer, sms.as_ref()) {
                Ok(()) => break,
                Err(err) => eprintln!("{err:#}; try again."),
            }
        }
    }
    session.account_login()?;
    if remember {
        crate::write_private(&saved, &serde_json::to_vec(&Credentials { apple_id: apple_id.into(), password: password.into() })?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trusted_phones() {
        let auth = json!({ "trustedPhoneNumbers": [
            { "id": 1, "numberWithDialCode": "+1 (•••) •••-••12", "pushMode": "sms" },
            { "id": 2, "obfuscatedNumber": "(•••) •••-••34" },
            { "numberWithDialCode": "no id" }
        ]});
        let phones = trusted_phones(&auth);
        assert_eq!(phones, vec![
            TrustedPhone { id: 1, number: "+1 (•••) •••-••12".into() },
            TrustedPhone { id: 2, number: "(•••) •••-••34".into() },
        ]);
        assert!(trusted_phones(&json!({})).is_empty());
    }

    #[test]
    fn parses_refresh_client_reply() {
        let data = json!({ "content": [
            { "id": "abc", "name": "Phone", "deviceDisplayName": "iPhone 16", "batteryLevel": 0.5,
              "location": { "latitude": 1.5, "longitude": -2.5, "horizontalAccuracy": 10.0, "timeStamp": 1700000000000i64 } },
            { "id": "def", "name": "Old iPad", "deviceDisplayName": "iPad", "batteryLevel": 0, "location": null }
        ]});
        let devices = parse_devices(&data);
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].location, Some((1.5, -2.5, 10.0, 1_700_000_000_000)));
        assert_eq!(devices[1].battery, None);
        assert_eq!(devices[1].location, None);
    }
}
