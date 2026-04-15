use std::borrow::Cow;
use std::pin::Pin;

use actix_web::{HttpRequest, HttpResponse};
use actix_web::body::{self, BoxBody, MessageBody};
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready};
use actix_web::http::StatusCode;
use actix_web::http::header::{CONTENT_TYPE, HeaderMap};

// --- this bucket of trait soup below is boilerplate for a middleware
pub struct InjectHotReloadMiddleware {
    pub enabled: bool,
}
impl<S, B> Transform<S, ServiceRequest> for InjectHotReloadMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: MessageBody + 'static,
    B::Error: std::fmt::Debug + std::fmt::Display,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = actix_web::Error;
    type Transform = InjectService<S>;
    type InitError = ();
    type Future = std::future::Ready<Result<Self::Transform, ()>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(InjectService { enabled: self.enabled, service }))
    }
}

pub struct InjectService<S> {
    enabled: bool,
    service: S,
}

impl<S, B> Service<ServiceRequest> for InjectService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = actix_web::Error> + 'static,
    B: MessageBody + 'static,
    B::Error: std::fmt::Debug + std::fmt::Display,
{
    type Response = ServiceResponse<BoxBody>;
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let fut = self.service.call(req);
        let enabled = self.enabled;
        Box::pin(async move {
            let res = fut.await?;
            if !enabled {
                return Ok(res.map_into_boxed_body());
            }
            process_request(res).await
        })
    }
}
// --- above trait soup is boilerplate for a middleware

async fn process_request<B>(
    res: ServiceResponse<B>,
) -> Result<ServiceResponse<BoxBody>, actix_web::Error>
where
    B: MessageBody + 'static,
    B::Error: std::fmt::Debug + std::fmt::Display,
{
    if !req_is_browser(res.request()) {
        return Ok(res.map_into_boxed_body());
    }

    let status = res.status();
    if status.is_client_error() || status.is_server_error() {
        // handle error case
        if is_html_path(res.request().path()) {
            // if the error looks like a request to a page that might not exist yet
            // return an error page
            if res_is_raw(&res) {
                // fabricate an error page to return as a raw error page
                let url = res.request().path().to_string();
                let body = make_error_page(
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Error"),
                    &url,
                );
                let (http_req, http_res) = res.into_parts();
                let headers = http_res.headers().clone();
                return Ok(build_response(http_req, StatusCode::OK, headers, body));
            }
            // error page but not requesting raw, make a placeholder wrapper
            // that supports reloading the page when it comes live
            let body = do_inject("".into());
            let (http_req, http_res) = res.into_parts();
            let headers = http_res.headers().clone();
            return Ok(build_response(http_req, StatusCode::OK, headers, body));
        }
        // non-html error, just return failure response as is
        return Ok(res.map_into_boxed_body());
    }

    // successfully found the resource
    if !res_is_html(&res) || res_is_raw(&res) {
        return Ok(res.map_into_boxed_body());
    }

    let (http_req, http_res) = res.into_parts();
    let headers = http_res.headers().clone();
    let bytes = match body::to_bytes(http_res.into_body()).await {
        Err(e) => {
            cu::error!("internal error while reading raw html before injection: {e:?}");
            return Err(actix_web::error::ErrorInternalServerError("unexpected"));
        }
        Ok(x) => x,
    };
    let bytes: Vec<u8> = bytes.into();
    let body_injected = do_inject(String::from_utf8_lossy(&bytes));

    Ok(build_response(http_req, status, headers, body_injected))
}

fn build_response(
    http_req: HttpRequest,
    status: StatusCode,
    headers: HeaderMap,
    body: String,
) -> ServiceResponse {
    let mut builder = HttpResponse::build(status);
    for (name, value) in headers {
        builder.append_header((name, value));
    }
    let res = builder.body(body);
    ServiceResponse::new(http_req, res)
}

/// Check if request looks like it's coming from a browser
fn req_is_browser(req: &actix_web::HttpRequest) -> bool {
    req.headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ua| ua.contains("Mozilla") || ua.contains("Chrome") || ua.contains("Safari"))
}

/// Check if the request is requesting a raw file
fn res_is_raw<B>(res: &ServiceResponse<B>) -> bool {
    if res.headers()
        .get("x-shwoop-is-raw")
        .is_some_and(|x| x.as_bytes() == b"1") {
        return true;
    }
        res
            .request()
            .query_string()
            .split('&')
            .any(|p| p == "x-shwoop-is-raw=1")
}

/// Check if the request is a success HTML file
fn res_is_html<B>(res: &ServiceResponse<B>) -> bool {
    res.headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/html"))
}

/// Returns true if the path looks like a navigable HTML page (ends with `/`,
/// has a `.html` extension, or has no extension), so we only serve the error
/// page for those, not for assets like CSS or JS.
fn is_html_path(path: &str) -> bool {
    if path.ends_with('/') {
        return true;
    }
    let last_segment = path.rsplit('/').next().unwrap_or(path);
    match last_segment.rfind('.') {
        Some(dot) => {
            let ext = &last_segment[dot..];
            ext.eq_ignore_ascii_case(".html") || ext.eq_ignore_ascii_case(".htm")
        }
        None => true,
    }
}

static WRAPPER: &str = include_str!("../../dist/index.html");
static ERROR_PAGE: &str = include_str!("error.html");

fn make_error_page(code: u16, text: &str, url: &str) -> String {
    ERROR_PAGE
        .replacen("PLACEHOLDER_STATUS_CODE", &code.to_string(), 2)
        .replacen("PLACEHOLDER_STATUS_TEXT", text, 2)
        .replacen("PLACEHOLDER_URL", url, 1)
}

fn do_inject(content_str: Cow<'_, str>) -> String {
    let mut output = String::new();
    let mut rest_wrapper = WRAPPER;

    // Replace <html> in wrapper with the opening <html ...> tag from content
    // to preserve any attributes (e.g. lang="en")
    if let Some(html_tag) = extract_html_tag(&content_str) {
        replace_placeholder(&mut output, &mut rest_wrapper, "<html>", html_tag);
    }

    // If content has a <link rel="icon">, add it to the wrapper's <head>
    if let Some(link_icon_tag) = extract_link_icon_tag(&content_str) {
        replace_placeholder(
            &mut output,
            &mut rest_wrapper,
            "<!-- PLACEHOLDER_LINK_ICON -->",
            link_icon_tag,
        );
    }

    output.push_str(rest_wrapper);
    output
}

/// Advance the `rest` cursor past `placeholder`, emitting everything before it plus
/// `replacement` into `output`. Logs an error if the placeholder is not found.
fn replace_placeholder<'a>(
    output: &mut String,
    rest: &mut &'a str,
    placeholder: &str,
    replacement: &str,
) {
    if let Some(pos) = rest.find(placeholder) {
        output.push_str(&rest[..pos]);
        output.push_str(replacement);
        *rest = &rest[pos + placeholder.len()..];
    } else {
        cu::error!("unexpected: did not find {placeholder:?} in wrapper html");
    }
}

/// Extract the opening tag of an element, e.g. `<html lang="en">`.
fn extract_html_tag(html: &str) -> Option<&str> {
    let start = html.find("<html")?;
    let end = html[start..].find('>')? + 1;
    Some(&html[start..start + end])
}

/// Find the first `<link>` tag with `rel="icon"` or `rel='icon'`.
fn extract_link_icon_tag<'a>(html: &'a str) -> Option<&'a str> {
    let mut rest = html;
    let mut base = 0;
    loop {
        let link_pos = rest.find("<link")?;
        let tag_end = rest[link_pos..].find('>')? + 1;
        let tag = &rest[link_pos..link_pos + tag_end];
        if tag.contains("rel=\"icon\"") || tag.contains("rel='icon'") {
            return Some(&html[base + link_pos..base + link_pos + tag_end]);
        }
        base += link_pos + tag_end;
        rest = &rest[link_pos + tag_end..];
    }
}
