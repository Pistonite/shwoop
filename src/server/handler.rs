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
        "max-age=0,stale-while-revalidate=3600"
    } else {
        "no-store"
    })
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
