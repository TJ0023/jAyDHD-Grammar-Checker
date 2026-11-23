//! Google Docs, read and edited through Google's own interface (the Docs API).
//!
//! Google Docs draws its pages itself, so no other program can read the words off the screen.
//! Google does allow a program you have signed in to read and edit your documents directly.
//! This module does that on a background thread:
//!
//!  * it watches which window you are in, and notices when that is a Google Docs tab;
//!  * it finds which document it is (from the browser's address bar, or from a link you paste);
//!  * it signs in once through your browser, then keeps a token on this computer;
//!  * it fetches the document every couple of seconds while you are in it, and sends the text;
//!  * it applies a fix as a real edit, only if nobody changed the document in the meantime.
//!
//! The background thread never touches this program's own windows (that once froze the app).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::capture;
use crate::doc_map::{self, DocText};
use crate::inputwatch::Input;

const SCOPE: &str = "https://www.googleapis.com/auth/documents";
const API: &str = "https://docs.googleapis.com/v1/documents";

/// What the panel needs to know about signing in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setup {
    /// `client_secret.json` is not in the settings folder yet.
    NoClientFile(PathBuf),
    NeedSignIn,
    SigningIn,
    Ready,
}

pub enum Event {
    Setup(Setup),
    /// The text of the open document, newest version.
    Doc {
        title: String,
        pid: u32,
        text: String,
    },
    /// You left the document (another program or another tab).
    Left,
    Note(String),
    /// The words selected in the document (read quietly after you select something).
    Selection(String),
}

pub enum Command {
    SignIn,
    SignOut,
    /// A pasted link, for when the address bar could not be read.
    UseLink(String),
    /// Switch the quiet reading of selected words on or off.
    AutoSelect(bool),
    /// Jump to this text in the document (with the document's own Find).
    Find(String),
    /// Replace `range` (which currently holds `old`) with `new`.
    Apply {
        range: Range<usize>,
        old: String,
        new: String,
    },
}

pub struct Docs {
    pub rx: Receiver<Event>,
    tx: Sender<Command>,
}

impl Docs {
    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }
}

pub fn start(repaint: impl Fn() + Send + 'static, input: Receiver<Input>) -> Docs {
    let (etx, rx) = channel();
    let (tx, crx) = channel();
    std::thread::spawn(move || {
        Worker {
            etx,
            crx,
            input,
            repaint: Box::new(repaint),
            setup: None,
            token: None,
            cur: None,
            manual: None,
            reader: win::Reader::default(),
            auto_select: true,
            select_at: None,
            last_note: String::new(),
            session_at: None,
        }
        .run()
    });
    Docs { rx, tx }
}

pub fn folder() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("GrammarDesk"))
}

fn client_file() -> Option<PathBuf> {
    Some(folder()?.join("client_secret.json"))
}

fn token_file() -> Option<PathBuf> {
    Some(folder()?.join("google_token.json"))
}

// -------------------------------------------------------------------------------------------
// The window you are in, and the document's address
// -------------------------------------------------------------------------------------------

struct Front {
    title: String,
    pid: u32,
    hwnd: isize,
}

#[cfg(windows)]
mod win {
    use super::Front;
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};
    use uiautomation::patterns::UIValuePattern;
    use uiautomation::types::{ControlType, Handle};
    use uiautomation::{UIAutomation, UIElement};

    unsafe extern "system" {
        fn GetForegroundWindow() -> isize;
        fn GetWindowTextW(hwnd: isize, text: *mut u16, max: i32) -> i32;
        fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    }

    pub fn foreground() -> Option<Front> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd == 0 {
                return None;
            }
            let mut buf = [0u16; 512];
            let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32).max(0) as usize;
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            Some(Front {
                title: String::from_utf16_lossy(&buf[..n]),
                pid,
                hwnd,
            })
        }
    }

    fn value_of(e: &UIElement) -> Option<String> {
        e.get_pattern::<UIValuePattern>().ok()?.get_value().ok()
    }

    /// Reads the browser's address bar through UI Automation. The bar is found once by walking
    /// the browser's toolbar (never the web page itself, which is huge), then remembered, so
    /// reading it again every half second is cheap.
    #[derive(Default)]
    pub struct Reader {
        automation: Option<UIAutomation>,
        cached: Option<(isize, UIElement)>,
        searched: Option<Instant>,
    }

    impl Reader {
        pub fn address(&mut self, hwnd: isize) -> Option<String> {
            if let Some((h, e)) = &self.cached {
                if *h == hwnd {
                    if let Some(v) = value_of(e) {
                        if v.contains("docs.google.com") {
                            return Some(v);
                        }
                    }
                }
            }
            self.cached = None;
            // Walking the toolbar takes a moment, so a failed search is not repeated at once.
            if self
                .searched
                .is_some_and(|t| t.elapsed() < Duration::from_secs(4))
            {
                return None;
            }
            self.searched = Some(Instant::now());
            if self.automation.is_none() {
                self.automation = UIAutomation::new().ok();
            }
            let a = self.automation.as_ref()?;
            let root = a.element_from_handle(Handle::from(hwnd)).ok()?;
            let walker = a.get_raw_view_walker().ok()?;
            let mut queue = VecDeque::from([root]);
            let mut seen = 0;
            let mut log = String::new();
            while let Some(e) = queue.pop_front() {
                seen += 1;
                if seen > 900 {
                    break;
                }
                let kind = e.get_control_type().ok();
                if matches!(kind, Some(ControlType::Edit | ControlType::ComboBox)) {
                    let value = value_of(&e);
                    if log.len() < 6000 {
                        log.push_str(&format!(
                            "{kind:?} | {} | {}\n",
                            e.get_name().unwrap_or_default(),
                            value
                                .as_deref()
                                .unwrap_or("(no value)")
                                .chars()
                                .take(90)
                                .collect::<String>()
                        ));
                    }
                    if let Some(v) = value {
                        if v.contains("docs.google.com/document") {
                            self.cached = Some((hwnd, e));
                            return Some(v);
                        }
                    }
                }
                if kind == Some(ControlType::Document) {
                    continue;
                }
                if let Ok(mut child) = walker.get_first_child(&e) {
                    loop {
                        queue.push_back(child.clone());
                        match walker.get_next_sibling(&child) {
                            Ok(next) => child = next,
                            Err(_) => break,
                        }
                    }
                }
            }
            if let Some(dir) = super::folder() {
                let _ = std::fs::write(
                    dir.join("docs_debug.txt"),
                    format!(
                        "Looked at {seen} items in the browser window; no address bar with a Google Docs address.\nEdit and ComboBox items seen:\n{log}"
                    ),
                );
            }
            None
        }
    }
}

#[cfg(not(windows))]
mod win {
    use super::Front;
    pub fn foreground() -> Option<Front> {
        None
    }
    #[derive(Default)]
    pub struct Reader;
    impl Reader {
        pub fn address(&mut self, _: isize) -> Option<String> {
            None
        }
    }
}

/// `My story - Google Docs - Mozilla Firefox` gives `My story`.
fn docs_title(window_title: &str) -> Option<String> {
    let i = window_title.find(" - Google Docs")?;
    Some(window_title[..i].to_string())
}

/// The page title Firefox keeps in its session file: the window title without the browser name.
fn page_title(window_title: &str) -> &str {
    for tail in [" \u{2014} Mozilla Firefox", " - Mozilla Firefox"] {
        if let Some(t) = window_title.strip_suffix(tail) {
            return t;
        }
    }
    window_title
}

/// Firefox saves its open tabs every ~15 seconds. When the address bar cannot be read, the tab
/// whose title matches this window gives the address (a little behind, but good enough).
fn session_address(window_title: &str) -> Option<String> {
    let title = page_title(window_title);
    let profiles = PathBuf::from(std::env::var_os("APPDATA")?).join("Mozilla/Firefox/Profiles");
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(profiles).ok()?.flatten() {
        let p = entry
            .path()
            .join("sessionstore-backups")
            .join("recovery.jsonlz4");
        if let Ok(m) = std::fs::metadata(&p).and_then(|m| m.modified()) {
            if newest.as_ref().is_none_or(|(t, _)| m > *t) {
                newest = Some((m, p));
            }
        }
    }
    let bytes = std::fs::read(newest?.1).ok()?;
    if bytes.len() < 12 || &bytes[..8] != b"mozLz40\0" {
        return None;
    }
    let size = u32::from_le_bytes(bytes[8..12].try_into().ok()?) as usize;
    let json = lz4_flex::block::decompress(&bytes[12..], size).ok()?;
    let v: Value = serde_json::from_slice(&json).ok()?;
    let mut best: Option<(u64, String)> = None;
    for window in v.get("windows")?.as_array()? {
        let Some(tabs) = window.get("tabs").and_then(Value::as_array) else {
            continue;
        };
        for tab in tabs {
            let Some(entries) = tab.get("entries").and_then(Value::as_array) else {
                continue;
            };
            let index = tab.get("index").and_then(Value::as_u64).unwrap_or(1) as usize;
            let Some(entry) = entries.get(index.saturating_sub(1)) else {
                continue;
            };
            let url = entry.get("url").and_then(Value::as_str).unwrap_or("");
            let entry_title = entry.get("title").and_then(Value::as_str).unwrap_or("");
            if url.contains("docs.google.com/document") && entry_title == title {
                let seen = tab.get("lastAccessed").and_then(Value::as_u64).unwrap_or(0);
                if best.as_ref().is_none_or(|(t, _)| seen >= *t) {
                    best = Some((seen, url.to_string()));
                }
            }
        }
    }
    best.map(|(_, url)| url)
}

// -------------------------------------------------------------------------------------------
// Signing in
// -------------------------------------------------------------------------------------------

struct Client {
    id: String,
    secret: String,
    token_uri: String,
    auth_uri: String,
}

struct Token {
    access: String,
    refresh: String,
    expires: u64,
}

/// An instant far enough back that "time since" checks pass at once.
fn long_ago() -> Instant {
    Instant::now()
        .checked_sub(Duration::from_secs(60))
        .unwrap_or_else(Instant::now)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_client() -> Option<Client> {
    let v: Value = serde_json::from_slice(&std::fs::read(client_file()?).ok()?).ok()?;
    let c = v.get("installed").or_else(|| v.get("web"))?;
    let s = |k: &str| c.get(k).and_then(Value::as_str).map(str::to_string);
    Some(Client {
        id: s("client_id")?,
        secret: s("client_secret")?,
        token_uri: s("token_uri").unwrap_or_else(|| "https://oauth2.googleapis.com/token".into()),
        auth_uri: s("auth_uri")
            .unwrap_or_else(|| "https://accounts.google.com/o/oauth2/v2/auth".into()),
    })
}

fn read_token() -> Option<Token> {
    let v: Value = serde_json::from_slice(&std::fs::read(token_file()?).ok()?).ok()?;
    Some(Token {
        access: v.get("access")?.as_str()?.to_string(),
        refresh: v.get("refresh")?.as_str()?.to_string(),
        expires: v.get("expires")?.as_u64()?,
    })
}

fn save_token(t: &Token) {
    if let Some(path) = token_file() {
        let _ = std::fs::write(
            path,
            json!({"access": t.access, "refresh": t.refresh, "expires": t.expires}).to_string(),
        );
    }
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
                out.push(u8::from_str_radix(hex, 16).unwrap_or(b'?'));
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn open_in_browser(url: &str) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

fn sign_in(client: &Client) -> Result<Token, String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Sign-in failed: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect = format!("http://127.0.0.1:{port}");
    let state = format!("{:x}{:x}", now(), std::process::id());
    let url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&state={}",
        client.auth_uri,
        encode(&client.id),
        encode(&redirect),
        encode(SCOPE),
        state
    );
    open_in_browser(&url);

    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let mut stream = loop {
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(_) if started.elapsed() < Duration::from_secs(180) => {
                std::thread::sleep(Duration::from_millis(250))
            }
            Err(_) => return Err("Sign-in timed out. Press the button to try again.".into()),
        }
    };
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut line = String::new();
    BufReader::new(&stream)
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;
    let query = line
        .split_whitespace()
        .nth(1)
        .and_then(|p| p.split('?').nth(1))
        .unwrap_or("");
    let get = |k: &str| {
        query.split('&').find_map(|kv| {
            let (key, v) = kv.split_once('=')?;
            (key == k).then(|| decode(v))
        })
    };
    let page = |msg: &str| {
        let body = format!(
            "<html><body style=\"font-family:sans-serif;text-align:center;margin-top:20vh\"><h2>{msg}</h2><p>You can close this tab.</p></body></html>"
        );
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    };
    if get("state").as_deref() != Some(state.as_str()) {
        let _ = stream.write_all(page("Sign-in did not match. Try again.").as_bytes());
        return Err("Sign-in did not match. Press the button to try again.".into());
    }
    let Some(code) = get("code") else {
        let _ = stream.write_all(page("Sign-in was cancelled.").as_bytes());
        return Err("Sign-in was cancelled.".into());
    };
    let reply: Result<Value, String> = ureq::post(&client.token_uri)
        .send_form(&[
            ("code", code.as_str()),
            ("client_id", client.id.as_str()),
            ("client_secret", client.secret.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
        ])
        .map_err(|e| format!("Google refused the sign-in: {e}"))
        .and_then(|r| r.into_json().map_err(|e| e.to_string()));
    let v = match reply {
        Ok(v) => v,
        Err(e) => {
            let _ = stream.write_all(page("Sign-in failed.").as_bytes());
            return Err(e);
        }
    };
    let _ = stream.write_all(page("Signed in. Back to your document!").as_bytes());
    let token = Token {
        access: v
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or("No access token")?
            .into(),
        refresh: v
            .get("refresh_token")
            .and_then(Value::as_str)
            .ok_or("No refresh token")?
            .into(),
        expires: now() + v.get("expires_in").and_then(Value::as_u64).unwrap_or(3000) - 60,
    };
    save_token(&token);
    Ok(token)
}

enum ApiError {
    /// The token no longer works: sign in again.
    SignedOut,
    Other(String),
}

fn refresh(client: &Client, t: &mut Token) -> Result<(), ApiError> {
    let r = ureq::post(&client.token_uri).send_form(&[
        ("client_id", client.id.as_str()),
        ("client_secret", client.secret.as_str()),
        ("refresh_token", t.refresh.as_str()),
        ("grant_type", "refresh_token"),
    ]);
    match r {
        Ok(r) => {
            let v: Value = r.into_json().map_err(|e| ApiError::Other(e.to_string()))?;
            t.access = v
                .get("access_token")
                .and_then(Value::as_str)
                .ok_or_else(|| ApiError::Other("No access token".into()))?
                .to_string();
            t.expires = now() + v.get("expires_in").and_then(Value::as_u64).unwrap_or(3000) - 60;
            save_token(t);
            Ok(())
        }
        // Google answers 400 invalid_grant once the weekly (test mode) token has expired.
        Err(ureq::Error::Status(400 | 401, _)) => Err(ApiError::SignedOut),
        Err(e) => Err(ApiError::Other(format!("Could not reach Google: {e}"))),
    }
}

// -------------------------------------------------------------------------------------------
// Reading and editing the document
// -------------------------------------------------------------------------------------------

struct Fetched {
    revision: String,
    /// The tab these words belong to (every document has at least one).
    tab_id: String,
    doc: DocText,
}

/// Collect `(index, text)` for every text run, including those inside tables.
fn collect_runs(content: &Value, out: &mut Vec<(usize, String)>) {
    let Some(items) = content.as_array() else {
        return;
    };
    for item in items {
        if let Some(elements) = item
            .pointer("/paragraph/elements")
            .and_then(Value::as_array)
        {
            for e in elements {
                if let (Some(start), Some(text)) = (
                    e.get("startIndex").and_then(Value::as_u64).or(Some(0)),
                    e.pointer("/textRun/content").and_then(Value::as_str),
                ) {
                    out.push((start as usize, text.to_string()));
                }
            }
        }
        if let Some(rows) = item.pointer("/table/tableRows").and_then(Value::as_array) {
            for row in rows {
                if let Some(cells) = row.get("tableCells").and_then(Value::as_array) {
                    for cell in cells {
                        if let Some(c) = cell.get("content") {
                            collect_runs(c, out);
                        }
                    }
                }
            }
        }
    }
}

/// Every tab of the document (child tabs too), in order, as `(tab id, text runs)`.
fn collect_tabs(tab: &Value, out: &mut Vec<(String, Vec<(usize, String)>)>) {
    let id = tab
        .pointer("/tabProperties/tabId")
        .and_then(Value::as_str)
        .unwrap_or("t.0")
        .to_string();
    let mut runs = Vec::new();
    if let Some(c) = tab.pointer("/documentTab/body/content") {
        collect_runs(c, &mut runs);
    }
    out.push((id, runs));
    if let Some(children) = tab.get("childTabs").and_then(Value::as_array) {
        for child in children {
            collect_tabs(child, out);
        }
    }
}

fn status_error(e: ureq::Error) -> ApiError {
    match e {
        ureq::Error::Status(401, _) => ApiError::SignedOut,
        ureq::Error::Status(403, _) => ApiError::Other(
            "Google says this account may not open that document (or the Docs API is off for the project).".into(),
        ),
        ureq::Error::Status(404, _) => ApiError::Other("Google could not find that document.".into()),
        ureq::Error::Status(429, _) => ApiError::Other("Google asked us to slow down; waiting.".into()),
        ureq::Error::Status(c, r) => {
            let body = r.into_string().unwrap_or_default();
            ApiError::Other(format!("Google error {c}: {}", body.chars().take(160).collect::<String>()))
        }
        e => ApiError::Other(format!("Could not reach Google: {e}")),
    }
}

/// Fetch the document and pick the tab you are in (the first when the address names none).
fn fetch(token: &Token, id: &str, tab: Option<&str>) -> Result<(String, Fetched), ApiError> {
    let url = format!("{API}/{id}?includeTabsContent=true");
    let v: Value = ureq::get(&url)
        .set("Authorization", &format!("Bearer {}", token.access))
        .call()
        .map_err(status_error)?
        .into_json()
        .map_err(|e| ApiError::Other(e.to_string()))?;
    let mut tabs = Vec::new();
    for t in v
        .get("tabs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect_tabs(t, &mut tabs);
    }
    let pick = tab
        .and_then(|want| tabs.iter().position(|(id, _)| id == want))
        .unwrap_or(0);
    let Some((tab_id, runs)) = tabs.into_iter().nth(pick) else {
        return Err(ApiError::Other(
            "That document came back with no text.".into(),
        ));
    };
    Ok((
        v.get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        Fetched {
            revision: v
                .get("revisionId")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            tab_id,
            doc: doc_map::build(&runs),
        },
    ))
}

/// The edit requests for one replacement: insert the new text at the end of the old, then delete
/// the old. Inserting first lets the new text take on the old text's style.
fn edit_body(revision: &str, tab_id: &str, range: Range<usize>, new: &str) -> Value {
    let mut requests = Vec::new();
    if !new.is_empty() {
        requests.push(
            json!({"insertText": {"location": {"index": range.end, "tabId": tab_id}, "text": new}}),
        );
    }
    requests.push(json!({"deleteContentRange": {"range": {"startIndex": range.start, "endIndex": range.end, "tabId": tab_id}}}));
    json!({"requests": requests, "writeControl": {"requiredRevisionId": revision}})
}

fn apply(
    token: &Token,
    id: &str,
    fresh: &Fetched,
    range: Range<usize>,
    new: &str,
) -> Result<(), ApiError> {
    ureq::post(&format!("{API}/{id}:batchUpdate"))
        .set("Authorization", &format!("Bearer {}", token.access))
        .send_json(edit_body(&fresh.revision, &fresh.tab_id, range, new))
        .map(|_| ())
        .map_err(status_error)
}

// -------------------------------------------------------------------------------------------
// The worker
// -------------------------------------------------------------------------------------------

struct Open {
    id: String,
    tab: Option<String>,
    window_title: String,
    pid: u32,
    hwnd: isize,
    fetched_at: Instant,
    /// Tab and revision last sent to the app, so an unchanged document is not sent again.
    sent_key: String,
}

struct Worker {
    etx: Sender<Event>,
    crx: Receiver<Command>,
    input: Receiver<Input>,
    repaint: Box<dyn Fn() + Send>,
    setup: Option<Setup>,
    token: Option<Token>,
    cur: Option<Open>,
    /// A document and tab from a pasted link; used while the window title stays the same.
    manual: Option<(String, Option<String>, String)>,
    reader: win::Reader,
    auto_select: bool,
    select_at: Option<Instant>,
    last_note: String,
    session_at: Option<(Instant, Option<String>)>,
}

impl Worker {
    fn emit(&self, e: Event) {
        let _ = self.etx.send(e);
        (self.repaint)();
    }

    /// A note, unless the same one was just shown.
    fn note(&mut self, m: String) {
        if self.last_note != m {
            self.last_note = m.clone();
            self.emit(Event::Note(m));
        }
    }

    fn set_setup(&mut self, s: Setup) {
        if self.setup.as_ref() != Some(&s) {
            self.setup = Some(s.clone());
            self.emit(Event::Setup(s));
        }
    }

    fn leave(&mut self) {
        if self.cur.take().is_some() {
            self.emit(Event::Left);
        }
    }

    fn run(mut self) {
        self.token = read_token();
        let own = std::process::id();
        loop {
            std::thread::sleep(Duration::from_millis(250));
            self.commands();
            while let Ok(i) = self.input.try_recv() {
                if i == Input::Select {
                    self.select_at = Some(Instant::now());
                }
            }
            let Some(front) = win::foreground() else {
                continue;
            };
            if front.pid == own {
                self.select_at = None;
                continue;
            }
            match docs_title(&front.title) {
                Some(title) => {
                    self.in_docs(front, title);
                    self.look_at_selection();
                }
                None => {
                    self.select_at = None;
                    self.leave();
                    self.manual = None;
                }
            }
        }
    }

    fn client_state(&mut self) -> Option<Client> {
        let Some(client) = read_client() else {
            if let Some(p) = client_file() {
                self.set_setup(Setup::NoClientFile(p));
            }
            return None;
        };
        if self.token.is_none() {
            self.set_setup(Setup::NeedSignIn);
        } else {
            self.set_setup(Setup::Ready);
        }
        Some(client)
    }

    /// A moment after you select something (mouse released, Shift+arrows), copy it quietly so the
    /// Thesaurus can show it. Skipped while typing, so it never gets in the way.
    fn look_at_selection(&mut self) {
        if !self.auto_select || self.cur.is_none() || self.token.is_none() {
            return;
        }
        let Some(at) = self.select_at else { return };
        if at.elapsed() < Duration::from_millis(350) {
            return;
        }
        self.select_at = None;
        if let Some(text) = capture::selection_now() {
            if text.chars().count() <= 2000 {
                self.emit(Event::Selection(text));
            }
        }
    }

    fn commands(&mut self) {
        while let Ok(c) = self.crx.try_recv() {
            match c {
                Command::SignIn => {
                    let Some(client) = read_client() else {
                        self.client_state();
                        continue;
                    };
                    self.set_setup(Setup::SigningIn);
                    match sign_in(&client) {
                        Ok(t) => {
                            self.token = Some(t);
                            self.set_setup(Setup::Ready);
                        }
                        Err(m) => {
                            self.set_setup(Setup::NeedSignIn);
                            self.emit(Event::Note(m));
                        }
                    }
                }
                Command::SignOut => {
                    self.token = None;
                    if let Some(p) = token_file() {
                        let _ = std::fs::remove_file(p);
                    }
                    self.set_setup(Setup::NeedSignIn);
                }
                Command::AutoSelect(on) => self.auto_select = on,
                Command::UseLink(link) => match doc_map::id_in_address(&link) {
                    Some(id) => {
                        let title = self
                            .cur
                            .as_ref()
                            .map(|o| o.window_title.clone())
                            .unwrap_or_default();
                        self.manual = Some((id, doc_map::tab_in_address(&link), title));
                        self.cur = None;
                    }
                    None => self.emit(Event::Note(
                        "That does not look like a Google Docs link.".into(),
                    )),
                },
                Command::Apply { range, old, new } => self.apply_edit(range, old, new),
                Command::Find(needle) => {
                    if let Some(o) = &self.cur {
                        capture::find_in_document(o.hwnd, &needle);
                        self.emit(Event::Note("Jumped to it in the document.".into()));
                    }
                }
            }
        }
    }

    fn apply_edit(&mut self, range: Range<usize>, old: String, new: String) {
        let (Some(client), Some(open)) = (read_client(), self.cur.as_ref()) else {
            return self.emit(Event::Note("Open the document first.".into()));
        };
        let Some(mut token) = self.token.take() else {
            return self.emit(Event::Note("Sign in first.".into()));
        };
        let (id, tab) = (open.id.clone(), open.tab.clone());
        let result = (|| {
            if token.expires <= now() {
                refresh(&client, &mut token)?;
            }
            // Fetch fresh, so the edit is checked against what is in the document right now.
            let (_, fresh) = fetch(&token, &id, tab.as_deref())?;
            if fresh.doc.text.get(range.clone()) != Some(old.as_str()) {
                return Err(ApiError::Other(
                    "The text changed since it was checked. It has been refreshed; try again."
                        .into(),
                ));
            }
            let Some(doc_range) = fresh.doc.to_doc_range(range.clone()) else {
                return Err(ApiError::Other(
                    "That spot cannot be edited safely from here. Fix it by hand.".into(),
                ));
            };
            apply(&token, &id, &fresh, doc_range, &new)
        })();
        if let Some(o) = &mut self.cur {
            o.fetched_at = long_ago();
        }
        match result {
            Ok(()) => {
                self.token = Some(token);
                self.emit(Event::Note("Fixed in the document.".into()));
            }
            Err(ApiError::SignedOut) => self.signed_out(),
            Err(ApiError::Other(m)) => {
                self.token = Some(token);
                self.emit(Event::Note(m));
            }
        }
    }

    fn signed_out(&mut self) {
        self.token = None;
        if let Some(p) = token_file() {
            let _ = std::fs::remove_file(p);
        }
        self.set_setup(Setup::NeedSignIn);
        self.emit(Event::Note("Google signed you out (this happens about weekly while the app is in test mode). Sign in again.".into()));
    }

    /// The document and tab in the window you are in: the live address bar first, then a pasted
    /// link, then Firefox's saved session.
    fn locate(&mut self, front: &Front) -> Option<(String, Option<String>)> {
        if let Some(addr) = self.reader.address(front.hwnd) {
            if let Some(id) = doc_map::id_in_address(&addr) {
                return Some((id, doc_map::tab_in_address(&addr)));
            }
        }
        if let Some((id, tab, title)) = &self.manual {
            if title.is_empty() || *title == front.title {
                return Some((id.clone(), tab.clone()));
            }
        }
        let stale = self
            .session_at
            .as_ref()
            .is_none_or(|(t, _)| t.elapsed() > Duration::from_secs(3));
        if stale {
            self.session_at = Some((Instant::now(), session_address(&front.title)));
        }
        let addr = self.session_at.as_ref().and_then(|(_, a)| a.clone())?;
        Some((
            doc_map::id_in_address(&addr)?,
            doc_map::tab_in_address(&addr),
        ))
    }

    fn in_docs(&mut self, front: Front, title: String) {
        if self.client_state().is_none() || self.token.is_none() {
            return;
        }
        let Some((id, tab)) = self.locate(&front) else {
            self.leave();
            self.note(format!(
                "In \u{201C}{title}\u{201D}, but could not tell which document it is. Paste its link below (details are in docs_debug.txt)."
            ));
            return;
        };
        let changed = self
            .cur
            .as_ref()
            .is_none_or(|o| o.id != id || o.tab != tab || o.hwnd != front.hwnd);
        if changed {
            self.last_note.clear();
            self.cur = Some(Open {
                id,
                tab,
                window_title: front.title.clone(),
                pid: front.pid,
                hwnd: front.hwnd,
                fetched_at: long_ago(),
                sent_key: String::new(),
            });
        }
        let Some(client) = read_client() else { return };
        let Some(open) = &mut self.cur else { return };
        open.window_title = front.title.clone();
        // Right away after a tab or document change, otherwise every couple of seconds.
        if open.fetched_at.elapsed() < Duration::from_millis(1200) {
            return;
        }
        let (id, tab, pid) = (open.id.clone(), open.tab.clone(), open.pid);
        let mut token = self.token.take().unwrap();
        let result = (|| {
            if token.expires <= now() {
                refresh(&client, &mut token)?;
            }
            fetch(&token, &id, tab.as_deref())
        })();
        match result {
            Ok((doc_title, fetched)) => {
                self.token = Some(token);
                let open = self.cur.as_mut().unwrap();
                open.fetched_at = Instant::now();
                let key = format!("{}:{}", fetched.tab_id, fetched.revision);
                if open.sent_key != key {
                    open.sent_key = key;
                    self.emit(Event::Doc {
                        title: doc_title,
                        pid,
                        text: fetched.doc.text,
                    });
                }
            }
            Err(ApiError::SignedOut) => self.signed_out(),
            Err(ApiError::Other(m)) => {
                self.token = Some(token);
                if let Some(o) = &mut self.cur {
                    o.fetched_at = Instant::now() + Duration::from_secs(8);
                }
                self.note(m);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_of_a_docs_window() {
        assert_eq!(
            docs_title("My story - Google Docs \u{2014} Mozilla Firefox").as_deref(),
            Some("My story")
        );
        assert_eq!(docs_title("Budget - Google Sheets - Mozilla Firefox"), None);
    }

    #[test]
    fn url_encoding_round_trips() {
        let s = "http://127.0.0.1:5000/a b&c";
        assert_eq!(decode(&encode(s)), s);
        assert_eq!(decode("4%2FabC-d"), "4/abC-d");
    }

    #[test]
    fn edit_inserts_then_deletes() {
        let b = edit_body("rev1", "t.0", 7..12, "world");
        assert_eq!(b["requests"][0]["insertText"]["location"]["index"], 12);
        assert_eq!(
            b["requests"][1]["deleteContentRange"]["range"]["startIndex"],
            7
        );
        assert_eq!(b["writeControl"]["requiredRevisionId"], "rev1");
        assert_eq!(b["requests"][0]["insertText"]["location"]["tabId"], "t.0");
        assert_eq!(
            edit_body("r", "t.0", 1..3, "")["requests"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn runs_inside_tables_are_found() {
        let v = json!([
            {"paragraph": {"elements": [{"startIndex": 1, "textRun": {"content": "Hi\n"}}]}},
            {"table": {"tableRows": [{"tableCells": [{"content": [
                {"paragraph": {"elements": [{"startIndex": 9, "textRun": {"content": "cell\n"}}]}}
            ]}]}]}}
        ]);
        let mut out = Vec::new();
        collect_runs(&v, &mut out);
        assert_eq!(
            out,
            vec![(1, "Hi\n".to_string()), (9, "cell\n".to_string())]
        );
    }
}
