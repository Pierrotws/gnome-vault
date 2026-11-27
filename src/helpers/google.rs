//! Google Drive for the desktop app: Google sign-in (OAuth for installed
//! apps: the browser, a loopback redirect and PKCE), with the
//! `drive.file` scope only (Passel sees the files it made, nothing
//! else), and Drive's REST calls ([`HttpDrive`]). The sync itself is
//! [`crate::app::drive`].
//!
//! The sign-in is kept in the system keyring (a refresh token), so it is
//! asked once per computer. The client id is this build's
//! (`PASSEL_GOOGLE_CLIENT_ID` and `PASSEL_GOOGLE_CLIENT_SECRET` when it
//! was compiled, or in the environment): without one, Drive is not
//! offered.

// ureq's error type is large by design (it carries the response); the
// request closures here return it as ureq does.
#![allow(clippy::result_large_err)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use data_encoding::BASE64URL_NOPAD;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::app::drive::{DriveApi, DriveError, DriveFile, SUFFIX};
use crate::i18n::gettext;

const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const API: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3";
const FIELDS: &str = "id,name,version,modifiedTime";
const MIME_TYPE: &str = "application/octet-stream";
/// Tags of Passel's files (`appProperties`), to find them again.
const TAG_KEY: &str = "passel";
const TAG_VAULT: &str = "vault";
const TAG_FOLDER: &str = "folder";

const KEYRING_SERVICE: &str = "io.pierrotws.Passel";
const KEYRING_USER: &str = "google-drive";

/// The most a vault file may weigh when downloaded.
const MAX_DOWNLOAD_BYTES: u64 = 1 << 30;
/// How long the browser may take to come back from Google.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// This build's Google client: `None` when it has none.
fn client() -> Option<(String, Option<String>)> {
    let id = std::env::var("PASSEL_GOOGLE_CLIENT_ID")
        .ok()
        .or_else(|| option_env!("PASSEL_GOOGLE_CLIENT_ID").map(str::to_string))
        .filter(|id| !id.trim().is_empty())?;
    let secret = std::env::var("PASSEL_GOOGLE_CLIENT_SECRET")
        .ok()
        .or_else(|| option_env!("PASSEL_GOOGLE_CLIENT_SECRET").map(str::to_string))
        .filter(|secret| !secret.trim().is_empty());
    Some((id, secret))
}

/// Whether this build can use Google Drive at all.
pub fn is_available() -> bool {
    client().is_some()
}

/// This session's access token, and the refresh token once read.
#[derive(Default)]
struct Session {
    access: Option<(String, Instant)>,
    refresh: Option<String>,
    /// The keyring was asked once already (it may hold nothing, or not
    /// answer at all): not asked again this session.
    keyring_read: bool,
}

static SESSION: Mutex<Session> = Mutex::new(Session {
    access: None,
    refresh: None,
    keyring_read: false,
});

/// How long the keyring may take: a Secret Service that does not answer
/// must not freeze the sign-in.
const KEYRING_TIMEOUT: Duration = Duration::from_secs(5);

fn session() -> std::sync::MutexGuard<'static, Session> {
    SESSION
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A Drive client, signed in without asking the user: with this
/// session's token or the sign-in kept in the keyring.
/// [`DriveError::SignIn`] when there is none (or it was withdrawn): then
/// [`begin_sign_in`].
pub fn connect() -> Result<HttpDrive, DriveError> {
    access_token()?;
    Ok(HttpDrive { agent: agent()? })
}

/// Forgets the sign-in, here and in the keyring.
pub fn sign_out() {
    *session() = Session {
        keyring_read: true,
        ..Session::default()
    };
    keyring(|entry| entry.delete_credential());
}

fn access_token() -> Result<String, DriveError> {
    let mut session = session();
    if let Some((token, expires)) = &session.access {
        if Instant::now() + Duration::from_secs(60) < *expires {
            return Ok(token.clone());
        }
    }
    if session.refresh.is_none() && !session.keyring_read {
        session.keyring_read = true;
        session.refresh = keyring(|entry| entry.get_password());
    }
    let refresh = session.refresh.clone().ok_or(DriveError::SignIn)?;
    let (id, secret) = client().ok_or(DriveError::NotConfigured)?;
    let mut form = vec![
        ("client_id", id.as_str()),
        ("refresh_token", refresh.as_str()),
        ("grant_type", "refresh_token"),
    ];
    if let Some(secret) = &secret {
        form.push(("client_secret", secret));
    }
    match agent()?.post(TOKEN_URL).send_form(&form) {
        Ok(response) => {
            let token = read_token(response)?;
            session.access = Some(token.clone());
            Ok(token.0)
        }
        // Revoked or expired: a new sign-in is needed.
        Err(ureq::Error::Status(400 | 401, _)) => {
            drop(session);
            sign_out();
            Err(DriveError::SignIn)
        }
        Err(err) => Err(http_error(err)),
    }
}

/// A sign-in under way: open [`SignIn::url`] in the browser, then
/// [`SignIn::finish`] (it blocks) waits for Google to send the browser
/// back.
pub struct SignIn {
    pub url: String,
    listener: TcpListener,
    redirect_uri: String,
    verifier: String,
    state: String,
    cancelled: Arc<AtomicBool>,
}

/// Starts a sign-in: a one-time address on this computer for Google to
/// send the browser back to.
pub fn begin_sign_in() -> Result<SignIn, DriveError> {
    let (id, _) = client().ok_or(DriveError::NotConfigured)?;
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| DriveError::Unreachable)?;
    let port = listener
        .local_addr()
        .map_err(|_| DriveError::Unreachable)?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}");
    let verifier = random_token();
    let state = random_token();
    let challenge = BASE64URL_NOPAD.encode(&Sha256::digest(verifier.as_bytes()));
    let url = url::Url::parse_with_params(
        AUTH_URL,
        &[
            ("client_id", id.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", state.as_str()),
            // A refresh token, so the sign-in lasts.
            ("access_type", "offline"),
            ("prompt", "consent"),
        ],
    )
    .map_err(|_| DriveError::NotConfigured)?
    .to_string();
    Ok(SignIn {
        url,
        listener,
        redirect_uri,
        verifier,
        state,
        cancelled: Arc::new(AtomicBool::new(false)),
    })
}

impl SignIn {
    /// Set to stop [`SignIn::finish`] waiting.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }

    /// Waits for the browser to come back, then keeps the sign-in.
    pub fn finish(self) -> Result<(), DriveError> {
        let code = self.wait_for_code()?;
        let (id, secret) = client().ok_or(DriveError::NotConfigured)?;
        let mut form = vec![
            ("client_id", id.as_str()),
            ("code", code.as_str()),
            ("code_verifier", self.verifier.as_str()),
            ("grant_type", "authorization_code"),
            ("redirect_uri", self.redirect_uri.as_str()),
        ];
        if let Some(secret) = &secret {
            form.push(("client_secret", secret));
        }
        let response = agent()?
            .post(TOKEN_URL)
            .send_form(&form)
            .map_err(http_error)?;
        let json: Value = response.into_json().map_err(|_| DriveError::Unreachable)?;
        let access = token_from(&json)?;
        let refresh = json["refresh_token"].as_str().map(str::to_string);
        if let Some(refresh) = refresh.clone() {
            // Without a keyring: signed in for this session only.
            if keyring(move |entry| entry.set_password(&refresh)).is_none() {
                log::warn!("could not keep the Google sign-in in the keyring");
            }
        }
        let mut session = session();
        session.access = Some(access);
        session.refresh = refresh;
        Ok(())
    }

    fn wait_for_code(&self) -> Result<String, DriveError> {
        self.listener
            .set_nonblocking(true)
            .map_err(|_| DriveError::Unreachable)?;
        let deadline = Instant::now() + SIGN_IN_TIMEOUT;
        loop {
            if self.cancelled.load(Ordering::Relaxed) || Instant::now() > deadline {
                return Err(DriveError::SignIn);
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(result) = self.answer(stream) {
                        return result;
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(150));
                }
                Err(_) => return Err(DriveError::Unreachable),
            }
        }
    }

    /// Reads the browser's request: the code, an error, or something
    /// else (`None`: a favicon request, keep waiting).
    fn answer(&self, mut stream: TcpStream) -> Option<Result<String, DriveError>> {
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut line = String::new();
        BufReader::new((&stream).take(8192))
            .read_line(&mut line)
            .ok()?;
        let target = line.split_whitespace().nth(1)?;
        let query = url::Url::parse(&format!("http://localhost{target}")).ok()?;
        let param = |name: &str| {
            query
                .query_pairs()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.into_owned())
        };
        let result = match (param("state"), param("code"), param("error")) {
            (Some(state), Some(code), _) if state == self.state => Ok(code),
            (Some(state), _, Some(_)) if state == self.state => Err(DriveError::Refused),
            _ => return None,
        };
        let message = if result.is_ok() {
            gettext("Passel is connected to Google Drive. You can close this tab.")
        } else {
            gettext("Passel was not given access to Google Drive. You can close this tab.")
        };
        let body = format!(
            "<!doctype html><meta charset=utf-8><title>Passel</title><p style=\"font:16px sans-serif;margin:3em\">{}</p>",
            html_escape(&message)
        );
        let _ = write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        Some(result)
    }
}

/// Drive's REST API (v3), signed in.
pub struct HttpDrive {
    agent: ureq::Agent,
}

impl HttpDrive {
    /// Sends a request with the access token; an expired token is
    /// renewed once. The outer error: no token; the inner one: the
    /// request's.
    fn send_raw(
        &self,
        request: impl Fn(&ureq::Agent, &str) -> Result<ureq::Response, ureq::Error>,
    ) -> Result<Result<ureq::Response, ureq::Error>, DriveError> {
        let token = access_token()?;
        match request(&self.agent, &format!("Bearer {token}")) {
            Err(ureq::Error::Status(401, _)) => {
                session().access = None;
                let token = access_token()?;
                Ok(request(&self.agent, &format!("Bearer {token}")))
            }
            other => Ok(other),
        }
    }

    fn send(
        &self,
        request: impl Fn(&ureq::Agent, &str) -> Result<ureq::Response, ureq::Error>,
    ) -> Result<ureq::Response, DriveError> {
        self.send_raw(request)?.map_err(http_error)
    }

    fn json(
        &self,
        request: impl Fn(&ureq::Agent, &str) -> Result<ureq::Response, ureq::Error>,
    ) -> Result<Value, DriveError> {
        self.send(request)?
            .into_json()
            .map_err(|_| DriveError::Unreachable)
    }

    /// The "Passel" folder at the top of the Drive, made if missing.
    fn folder(&self) -> Result<String, DriveError> {
        let query = format!(
            "appProperties has {{ key='{TAG_KEY}' and value='{TAG_FOLDER}' }} and trashed=false"
        );
        let found = self.json(|agent, auth| {
            agent
                .get(&format!("{API}/files"))
                .set("Authorization", auth)
                .query("q", &query)
                .query("spaces", "drive")
                .query("fields", "files(id)")
                .call()
        })?;
        if let Some(id) = found["files"][0]["id"].as_str() {
            return Ok(id.to_string());
        }
        let metadata = json!({
            "name": "Passel",
            "mimeType": "application/vnd.google-apps.folder",
            "appProperties": { TAG_KEY: TAG_FOLDER },
        });
        let made = self.json(|agent, auth| {
            agent
                .post(&format!("{API}/files"))
                .set("Authorization", auth)
                .query("fields", "id")
                .send_json(metadata.clone())
        })?;
        made["id"]
            .as_str()
            .map(str::to_string)
            .ok_or(DriveError::Unreachable)
    }
}

impl DriveApi for HttpDrive {
    fn vaults(&self) -> Result<Vec<DriveFile>, DriveError> {
        let query = format!(
            "appProperties has {{ key='{TAG_KEY}' and value='{TAG_VAULT}' }} and trashed=false"
        );
        let json = self.json(|agent, auth| {
            agent
                .get(&format!("{API}/files"))
                .set("Authorization", auth)
                .query("q", &query)
                .query("spaces", "drive")
                .query("orderBy", "modifiedTime desc")
                .query("pageSize", "100")
                .query("fields", &format!("files({FIELDS})"))
                .call()
        })?;
        Ok(json["files"]
            .as_array()
            .map(|files| files.iter().filter_map(drive_file).collect())
            .unwrap_or_default())
    }

    fn file(&self, id: &str) -> Result<Option<DriveFile>, DriveError> {
        let url = format!("{API}/files/{}", encode(id));
        let response = match self.send_raw(|agent, auth| {
            agent
                .get(&url)
                .set("Authorization", auth)
                .query("fields", &format!("{FIELDS},trashed"))
                .call()
        })? {
            Ok(response) => response,
            // Deleted.
            Err(ureq::Error::Status(404, _)) => return Ok(None),
            Err(err) => return Err(http_error(err)),
        };
        let json: Value = response.into_json().map_err(|_| DriveError::Unreachable)?;
        if json["trashed"].as_bool() == Some(true) {
            return Ok(None);
        }
        Ok(drive_file(&json))
    }

    fn download(&self, id: &str, to: &Path) -> Result<(), DriveError> {
        let url = format!("{API}/files/{}", encode(id));
        let response = self.send(|agent, auth| {
            agent
                .get(&url)
                .set("Authorization", auth)
                .query("alt", "media")
                .call()
        })?;
        let mut file = std::fs::File::create(to).map_err(store_io)?;
        std::io::copy(
            &mut response.into_reader().take(MAX_DOWNLOAD_BYTES),
            &mut file,
        )
        .map_err(|_| DriveError::Unreachable)?;
        Ok(())
    }

    fn create(&self, vault_name: &str, from: &Path) -> Result<DriveFile, DriveError> {
        let folder = self.folder()?;
        let metadata = json!({
            "name": format!("{vault_name}{SUFFIX}"),
            "mimeType": MIME_TYPE,
            "parents": [folder],
            "appProperties": { TAG_KEY: TAG_VAULT },
        });
        let contents = std::fs::read(from).map_err(store_io)?;
        let boundary = format!("passel-{}", random_token());
        let mut body = format!(
            "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{metadata}\r\n--{boundary}\r\nContent-Type: {MIME_TYPE}\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(&contents);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let json = self.json(|agent, auth| {
            agent
                .post(&format!("{UPLOAD}/files"))
                .set("Authorization", auth)
                .set(
                    "Content-Type",
                    &format!("multipart/related; boundary={boundary}"),
                )
                .query("uploadType", "multipart")
                .query("fields", FIELDS)
                .send_bytes(&body)
        })?;
        drive_file(&json).ok_or(DriveError::Unreachable)
    }

    fn update(&self, id: &str, from: &Path) -> Result<DriveFile, DriveError> {
        let contents = std::fs::read(from).map_err(store_io)?;
        let url = format!("{UPLOAD}/files/{}", encode(id));
        let json = self.json(|agent, auth| {
            agent
                .request("PATCH", &url)
                .set("Authorization", auth)
                .set("Content-Type", MIME_TYPE)
                .query("uploadType", "media")
                .query("fields", FIELDS)
                .send_bytes(&contents)
        })?;
        drive_file(&json).ok_or(DriveError::Unreachable)
    }
}

fn http_error(err: ureq::Error) -> DriveError {
    match err {
        ureq::Error::Status(code, response) => {
            // The body (no secrets in it) helps when Google refuses.
            let detail = response.into_string().unwrap_or_default();
            log::info!(
                "Google: HTTP {code} {}",
                detail.chars().take(300).collect::<String>()
            );
            match code {
                401 => DriveError::SignIn,
                403 if detail.contains("storageQuotaExceeded") => DriveError::Full,
                403 => DriveError::Refused,
                507 => DriveError::Full,
                _ => DriveError::Unreachable,
            }
        }
        ureq::Error::Transport(transport) => {
            log::info!("Google: {}", transport.kind());
            DriveError::Unreachable
        }
    }
}

fn agent() -> Result<ureq::Agent, DriveError> {
    let tls = native_tls::TlsConnector::new().map_err(|_| DriveError::Unreachable)?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .build())
}

fn read_token(response: ureq::Response) -> Result<(String, Instant), DriveError> {
    let json: Value = response.into_json().map_err(|_| DriveError::Unreachable)?;
    token_from(&json)
}

fn token_from(json: &Value) -> Result<(String, Instant), DriveError> {
    let token = json["access_token"]
        .as_str()
        .ok_or(DriveError::Refused)?
        .to_string();
    let lifetime = json["expires_in"].as_u64().unwrap_or(3600);
    Ok((token, Instant::now() + Duration::from_secs(lifetime)))
}

fn drive_file(json: &Value) -> Option<DriveFile> {
    Some(DriveFile {
        id: json["id"].as_str()?.to_string(),
        name: json["name"].as_str().unwrap_or_default().to_string(),
        version: json["version"].as_str().unwrap_or_default().to_string(),
        modified_time: json["modifiedTime"].as_str().map(str::to_string),
    })
}

/// Runs `action` on Passel's keyring entry, giving up after
/// [`KEYRING_TIMEOUT`]; `None` when it failed or took too long.
fn keyring<T: Send + 'static>(
    action: impl FnOnce(&keyring::Entry) -> keyring::Result<T> + Send + 'static,
) -> Option<T> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result =
            keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).and_then(|entry| action(&entry));
        let _ = sender.send(result);
    });
    match receiver.recv_timeout(KEYRING_TIMEOUT) {
        Ok(Ok(value)) => Some(value),
        Ok(Err(keyring::Error::NoEntry)) => None,
        Ok(Err(err)) => {
            log::info!("keyring: {err}");
            None
        }
        Err(_) => {
            log::info!("keyring: no answer");
            None
        }
    }
}

fn store_io(err: std::io::Error) -> DriveError {
    DriveError::Store(crate::pass::store::StoreError::Io(err))
}

/// 32 random bytes, URL-safe.
fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the system's random source");
    BASE64URL_NOPAD.encode(&bytes)
}

fn encode(segment: &str) -> String {
    url::form_urlencoded::byte_serialize(segment.as_bytes()).collect()
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign_in_for_test() -> SignIn {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        SignIn {
            url: String::new(),
            listener,
            redirect_uri: format!("http://127.0.0.1:{port}"),
            verifier: random_token(),
            state: "the-state".to_string(),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    fn browser(sign_in: &SignIn, path: &str) -> String {
        let mut stream = TcpStream::connect(sign_in.listener.local_addr().unwrap()).unwrap();
        write!(stream, "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        reply
    }

    #[test]
    fn the_redirect_gives_the_code_only_with_the_right_state() {
        let sign_in = sign_in_for_test();
        let waiting = std::thread::scope(|scope| {
            let waiter = scope.spawn(|| sign_in.wait_for_code());
            // Something else first (a favicon): ignored.
            browser(&sign_in, "/favicon.ico");
            browser(&sign_in, "/?state=forged&code=evil");
            let reply = browser(&sign_in, "/?state=the-state&code=4%2Fabc&scope=x");
            assert!(reply.starts_with("HTTP/1.1 200 OK"));
            waiter.join().unwrap()
        });
        assert_eq!(waiting.unwrap(), "4/abc");
    }

    #[test]
    fn a_refused_consent_and_a_cancel_end_the_wait() {
        let sign_in = sign_in_for_test();
        let refused = std::thread::scope(|scope| {
            let waiter = scope.spawn(|| sign_in.wait_for_code());
            browser(&sign_in, "/?state=the-state&error=access_denied");
            waiter.join().unwrap()
        });
        assert!(matches!(refused, Err(DriveError::Refused)));

        let sign_in = sign_in_for_test();
        sign_in.cancel_flag().store(true, Ordering::Relaxed);
        assert!(matches!(sign_in.wait_for_code(), Err(DriveError::SignIn)));
    }

    #[test]
    fn pkce_tokens_are_url_safe_and_long_enough() {
        let token = random_token();
        assert!(token.len() >= 43);
        assert!(token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    }
}
