use std::net::{IpAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::Arc;

use actix_web::guard::{self, Guard};
use actix_web::http::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderValue, USER_AGENT};
use actix_web::web::{Data as WebData, Payload};
use actix_web::{HttpRequest, HttpResponse};
use lol_html::{HtmlRewriter, Settings, element, html_content::ContentType};

use crate::server::{Session, SessionMgr};

pub struct ServerState {
    /// Websocket sessions
    ///
    /// This doesn't need to be an Arc technically because WebData is an Arc
    /// internally. However, this lets us decouple the reloader/watcher
    /// with the server state which is easier to reason.
    pub sessions: Arc<SessionMgr>,
    /// Input path (normalized)
    pub path: PathBuf,
}

pub fn local_ip() -> cu::Result<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    Ok(socket.local_addr()?.ip())
}

/// Check the request is a websocket upgrade request
pub fn websocket_guard() -> impl Guard {
    guard::fn_guard(|ctx| {
        ctx.head()
            .headers()
            .get("upgrade")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
    })
}

/// Handle incoming websocket upgrade requests
pub async fn websocket(
    req: HttpRequest,
    stream: Payload,
    state: WebData<ServerState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (session, response) = Session::start(req, stream)?;
    state.sessions.add(session);
    Ok(response)
}

// see comment in vite.config.ts
#[path = "../../dist/metadata.rs"]
mod js_metadata;
pub fn js_source_path() -> &'static str {
    js_metadata::JS_SOURCEMAP_PATH.strip_suffix(".map").unwrap()
}
pub fn js_sourcemap_path() -> &'static str {
    js_metadata::JS_SOURCEMAP_PATH
}
pub async fn js_source() -> HttpResponse {
    HttpResponse::Ok()
        .append_header(("Content-Type", "text/javascript"))
        // JS path is hashed - cache to make loading faster
        .append_header(("Cache-Control", "max-age=86400, immutable"))
        .body(js_metadata::JS_SOURCE)
}
pub async fn js_sourcemap() -> HttpResponse {
    HttpResponse::Ok()
        .append_header(("Content-Type", "text/javascript"))
        // JS path is hashed - cache to make loading faster
        .append_header(("Cache-Control", "max-age=86400, immutable"))
        .body(js_metadata::JS_SOURCEMAP)
}

pub fn inject_bootstrap(html: &[u8]) -> Vec<u8> {
    let script = js_metadata::JS_BOOTSTRAP;
    let injected = std::cell::Cell::new(false);
    let mut output = Vec::with_capacity(html.len() + script.len());
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: vec![element!("body", |el| {
                injected.set(true);
                el.append(script, ContentType::Html);
                Ok(())
            })],
            ..Settings::default()
        },
        |c: &[u8]| output.extend_from_slice(c),
    );
    if let Err(e) = rewriter.write(html) {
        cu::warn!("failed to inject bootstrap: {e}");
        return html.to_vec();
    }
    if let Err(e) = rewriter.end() {
        cu::warn!("failed to finalize injected html: {e}");
        return html.to_vec();
    }
    if !injected.get() {
        cu::warn!("no <body> tag found, not injecting");
    }
    output
}

pub async fn single_file(state: WebData<ServerState>) -> Result<HttpResponse, actix_web::Error> {
    let body = match std::fs::read(&state.path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(HttpResponse::NotFound().finish());
        }
        Err(e) => {
            cu::error!("(single-file mode) failed to read file: {e}");
            return Ok(HttpResponse::InternalServerError().finish());
        }
    };
    Ok(html_response(body))
}

/// Check if request looks like it's coming from a browser
pub fn is_browser(req: &HttpRequest) -> bool {
    req.headers()
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ua| ua.contains("Mozilla") || ua.contains("Chrome") || ua.contains("Safari"))
}

/// Returns true if the path looks like a navigable HTML page (ends with `/`,
/// has a `.html` extension, or has no extension ), so we only serve the error
/// page for those, not for assets like CSS or JS.
pub fn probably_webpage(path: &str) -> bool {
    let bytes = path.as_bytes();
    // mostly likely paths first
    if bytes.ends_with(b"/") || bytes.ends_with(b".html") {
        return true;
    }
    // case-insensitive check
    let last_segment = path.rsplit('/').next().unwrap_or(path);
    match last_segment.rfind('.') {
        Some(dot) => {
            let ext = &last_segment[dot..];
            ext.eq_ignore_ascii_case(".html") || ext.eq_ignore_ascii_case(".htm")
        }
        // paths like '/something/something'
        None => true,
    }
}

/// Check if the request is a success HTML file
pub fn is_html<B>(res: &HttpResponse<B>) -> bool {
    res.headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/html"))
}

pub fn set_cache<B>(res: &mut HttpResponse<B>, cache: bool) {
    res.headers_mut()
        .insert(CACHE_CONTROL, cache_control_value(cache));
}

pub fn html_response(body: Vec<u8>) -> HttpResponse {
    HttpResponse::Ok()
        .append_header((CONTENT_TYPE, "text/html"))
        .append_header((CACHE_CONTROL, "no-store"))
        .body(body)
}

pub fn cache_control_value(cache: bool) -> HeaderValue {
    HeaderValue::from_static(if cache {
        // this ensures files that are unlikely to be changed, such as
        // assets/css, are cached for faster loads (which is very important
        // to reduce flickering while reloading)
        // If they do change,
        // the browser will always revalidate to make sure it is correct
        // on the next call.
        "max-age=0,stale-while-revalidate=3600"
    } else {
        "no-store"
    })
}

static DIR_LISTING_HTML: &str = include_str!("dir.html");

pub fn directory_listing(fs_path: &std::path::Path, url_path: &str) -> String {
    let mut entries: Vec<_> = match std::fs::read_dir(fs_path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => vec![],
    };
    entries.sort_by(|a, b| {
        let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
        b_dir.cmp(&a_dir).then_with(|| a.file_name().cmp(&b.file_name()))
    });

    let url_path = if url_path.ends_with('/') {
        url_path.to_owned()
    } else {
        format!("{url_path}/")
    };

    let mut rows = String::new();
    if url_path != "/" {
        rows.push_str("<tr><td><a href=\"../\" class=\"up\">../</a></td><td class=\"col-size\"></td><td class=\"col-date\"></td></tr>\n");
    }
    for entry in &entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let meta = entry.metadata().ok();
        let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);

        rows.push_str("<tr><td><a href=\"");
        rows.push_str(&dir_url_encode(&name));
        if is_dir {
            rows.push('/');
        }
        rows.push_str("\" class=\"");
        rows.push_str(if is_dir { "d" } else { "f" });
        rows.push_str("\">");
        dir_html_escape(&name, &mut rows);
        if is_dir {
            rows.push('/');
        }
        rows.push_str("</a></td><td class=\"col-size\">");
        if let Some(ref m) = meta {
            if !is_dir {
                rows.push_str(&dir_format_size(m.len()));
            }
        }
        rows.push_str("</td><td class=\"col-date\">");
        if let Some(ref m) = meta {
            if let Ok(t) = m.modified() {
                rows.push_str(&dir_format_date(t));
            }
        }
        rows.push_str("</td></tr>\n");
    }

    let mut path_escaped = String::new();
    dir_html_escape(&url_path, &mut path_escaped);

    DIR_LISTING_HTML
        .replace("PLACEHOLDER_PATH", &path_escaped)
        .replacen("PLACEHOLDER_ROWS", &rows, 1)
}

fn dir_html_escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

fn dir_url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn dir_format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn dir_format_date(t: std::time::SystemTime) -> String {
    let Ok(dur) = t.duration_since(std::time::UNIX_EPOCH) else {
        return String::new();
    };
    let secs = dur.as_secs();
    let min = (secs / 60) % 60;
    let hour = (secs / 3600) % 24;
    let (y, m, d) = dir_days_to_ymd(secs / 86400);
    format!("{y:04}-{m:02}-{d:02} {hour:02}:{min:02}")
}

fn dir_days_to_ymd(days: u64) -> (u32, u32, u32) {
    // Hinnant's algorithm: http://howardhinnant.github.io/date_algorithms.html
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe as i64 + era * 400 + if m <= 2 { 1 } else { 0 };
    (y as u32, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(html: &str) -> String {
        let out = inject_bootstrap(html.as_bytes());
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn injects_before_body_close() {
        let script = js_metadata::JS_BOOTSTRAP;
        let result = run("<html><body><p>hello</p></body></html>");
        assert_eq!(
            result,
            format!("<html><body><p>hello</p>{script}</body></html>")
        );
    }
}
