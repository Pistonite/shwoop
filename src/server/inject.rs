use actix_web::HttpResponse;
use actix_web::body::{self, BoxBody, MessageBody};
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready};
use actix_web::http::header::CONTENT_TYPE;

pub struct InjectHotReloadMiddleware;
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
        std::future::ready(Ok(InjectService { service }))
    }
}

pub struct InjectService<S> {
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
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>>>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let fut = self.service.call(req);
        Box::pin(async move {
            let res = fut.await?;
            process_request(res).await
        })
    }
}

async fn process_request<B>(
    res: ServiceResponse<B>,
) -> Result<ServiceResponse<BoxBody>, actix_web::Error> where 
    B: MessageBody + 'static,
    B::Error: std::fmt::Debug + std::fmt::Display,
{

    let is_raw = res.headers().get("x-shwoop-is-raw")
        .is_some_and(|x| x.as_bytes() == b"1")
        || res.request().query_string().split('&')
        .any(|p| p == "x-shwoop-is-raw=1");
    if is_raw {
        // requesting raw content, do not inject
        return Ok(res.map_into_boxed_body());
    }

    let is_html = res
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/html"));

    if !is_html {
        // only inject to html files
        return Ok(res.map_into_boxed_body());
    }

    let is_browser = res.request().headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ua| ua.contains("Mozilla") || ua.contains("Chrome") || ua.contains("Safari"));
    if !is_browser {
        return Ok(res.map_into_boxed_body());
    }

    // if it's a failure - return the error page
    let status = res.status();
    if status.is_client_error() || status.is_server_error() {
        let url = res.request().uri().to_string();
        let body = error_page(status.as_u16(), status.canonical_reason().unwrap_or("Error"), &url);
        let new_res = HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(body);
        return Ok(ServiceResponse::new(res.into_parts().0, new_res));
    }

    let (http_req, http_res) = res.into_parts();
    let headers = http_res.headers().clone();

    let bytes = match body::to_bytes(http_res.into_body()).await {
        Err(e) => {
            cu::error!("internal error while reading raw html before injection: {e:?}");
            return Err(actix_web::error::ErrorInternalServerError("unexpected"));
        }
        Ok(x) => x
    };

    let injected = do_inject(bytes.into());

    let mut builder = HttpResponse::build(status);
    for (name, value) in headers {
        builder.append_header((name, value));
    }
    let new_res = builder.body(injected);
    Ok(ServiceResponse::new(http_req, new_res))
}

static WRAPPER: &str = include_str!("../../dist/index.html");
static ERROR_PAGE: &str = include_str!("error.html");

fn error_page(code: u16, text: &str, url: &str) -> String {
    ERROR_PAGE
        .replacen("PLACEHOLDER_STATUS_CODE", &code.to_string(), 2)
        .replacen("PLACEHOLDER_STATUS_TEXT", text, 2)
        .replacen("PLACEHOLDER_URL", url, 1)
}

fn do_inject(content: Vec<u8>) -> Vec<u8> {
    let content_str = String::from_utf8_lossy(&content);
    let mut output = String::new();
    let mut rest_wrapper = WRAPPER;

    // Replace <html> in wrapper with the opening <html ...> tag from content
    // to preserve any attributes (e.g. lang="en")
    if let Some(html_tag) = extract_html_tag(&content_str) {
        replace_placeholder(&mut output, &mut rest_wrapper, "<html>", html_tag);
    }

    // If content has a <link rel="icon">, add it to the wrapper's <head>
    if let Some(link_icon_tag) = extract_link_icon_tag(&content_str) {
        replace_placeholder(&mut output, &mut rest_wrapper, "<!-- PLACEHOLDER_LINK_ICON -->", link_icon_tag);
    }

    output.push_str(rest_wrapper);
    output.into_bytes()
}

/// Advance the `rest` cursor past `placeholder`, emitting everything before it plus
/// `replacement` into `output`. Logs an error if the placeholder is not found.
fn replace_placeholder<'a>(output: &mut String, rest: &mut &'a str, placeholder: &str, replacement: &str) {
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
