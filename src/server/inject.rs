use std::borrow::Cow;
use std::pin::Pin;
use std::sync::Arc;

use actix_web::body::{self, BoxBody, MessageBody};
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready};
use actix_web::http::StatusCode;
use actix_web::http::header::{CONTENT_TYPE, HeaderMap};
use actix_web::{HttpRequest, HttpResponse};
use dashmap::{DashMap, DashSet};

use crate::server::handler;

// --- this bucket of trait soup below is boilerplate for a middleware
pub struct InjectHotReloadMiddleware {
    raw: bool,
    path_is_webpage_cache: Arc<DashMap<String, bool>>,
}
impl InjectHotReloadMiddleware {
    pub fn new(raw: bool, cache: Arc<DashMap<String, bool>>) -> Self {
        Self {
            raw,
            path_is_webpage_cache: cache,
        }
    }
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
        std::future::ready(Ok(InjectService {
            raw: self.raw,
            wrapper_paths: Arc::clone(&self.path_is_webpage_cache),
            service,
        }))
    }
}

pub struct InjectService<S> {
    raw: bool,
    wrapper_paths: Arc<DashMap<String, bool>>,
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

    fn call(&self, sreq: ServiceRequest) -> Self::Future {
        let req = sreq.request();
        let pathname = req.path();

        macro_rules! handle {
            (passthrough) => {{
                handle!(|mut res| {
                    handler::set_cache(res.response_mut(), false);
                    Ok(res.map_into_boxed_body())
                })
            }};
            (|$res:ident| $code:stmt) => {{
                let fut = self.service.call(sreq);
                return Box::pin(async move {
                    let $res = fut.await?;
                    $code
                });
            }};
            (|mut $res:ident| $code:stmt) => {{
                let fut = self.service.call(sreq);
                return Box::pin(async move {
                    let mut $res = fut.await?;
                    $code
                });
            }};
            ($code:stmt) => {{
                return Box::pin(async move { $code });
            }};
        }

        if self.raw || !handler::is_browser(req) {
            cu::info!("(passthrough) {pathname}");
            handle!(passthrough);
        }

        // let is_raw = handler::is_raw(req);
        // if is_raw {
        //     handle!(|mut res| {
        //         let status = res.status();
        //         if !status.is_client_error() && !status.is_server_error() {
        //             // if not error (success, redirection...), return the inner response
        //             handler::set_cache(res.response_mut(), false);
        //             cu::info!("(raw) {} - {}", status, res.request().uri());
        //             return Ok(res.map_into_boxed_body())
        //         }
        //         // did not find the raw resource
        //         let req = res.request();
        //         let pathname = req.path();
        //         if handler::probably_webpage(pathname) {
        //             // if probably requesting HTML page, return error page
        //             let body = make_error_page(status);
        //             cu::error!("(raw,webpage) {} - {}", status, res.request().uri());
        //             let (req, _) = res.into_parts();
        //             return Ok(ServiceResponse::new(req, handler::html_response(body, false)));
        //         }
        //
        //         handler::set_cache(res.response_mut(), false);
        //         cu::error!("(raw) {} - {}", status, res.request().uri());
        //         Ok(res.map_into_boxed_body())
        //     });
        // }

        // let pathname = req.path();

        // // if we previously requested the page then we know if it is a webpage
        // // (this assumes paths don't suddenly change between assets and webpage, which is
        // // reasonable)
        // match self.wrapper_paths.get(pathname).map(|x| *x) {
        //     Some(true) => {
        //         let body = do_inject("".into(), pathname);
        //         let (req, _) = sreq.into_parts();
        //         cu::info!("(wrapper,from-cache) {}", req.uri());
        //         // bypass inner service
        //         handle! {
        //             Ok(ServiceResponse::new(req, handler::html_response(body, true /* cache */)))
        //         }
        //     }
        //     Some(false) => {
        //         cu::info!("(passthrough,from-cache) {}", req.uri());
        //         handle!(passthrough)
        //     }
        //     _ => {}
        // }

        // otherwise, we have to call the inner service to check the content-type
        // let cache = Arc::clone(&self.wrapper_paths);

        // call inner service to serve the file

        handle!(|mut res| {
            let status = res.status();
            let pathname = res.request().path();
            // never inject or cache 1XX and 3XX
            if status.is_informational() || status.is_redirection() {
                cu::debug!("{} - {}", status, res.request().uri());
                // cache.insert(pathname.to_owned(), false);
                handler::set_cache(res.response_mut(), true);
                return Ok(res.map_into_boxed_body());
            }
            if status.is_client_error() || status.is_server_error() {
                // error (likely resource not found)
                if handler::probably_webpage(pathname) {
                    cu::error!("{} - {} (injected)", status, res.request().uri());
                    let body = make_error_page(status);
                    let body = handler::inject_bootstrap(body.as_bytes());
                    let (req, _) = res.into_parts();
                    return Ok(ServiceResponse::new(
                        req,
                        handler::html_bytes_response(body),
                    ));
                }
                cu::error!("{} - {}", status, res.request().uri());
                handler::set_cache(res.response_mut(), false);
                return Ok(res.map_into_boxed_body());
            };
            // success - we can check the content type
            let is_webpage = handler::is_html(res.response());
            if !is_webpage {
                cu::debug!("{} - {}", status, res.request().uri());
                // cache - but browser will revalidate each time
                handler::set_cache(res.response_mut(), true);
                return Ok(res.map_into_boxed_body());
            }
            cu::info!("{} - {} (injected)", status, res.request().uri());
            let (req, res) = res.into_parts();
            let body_bytes = match body::to_bytes(res.into_body()).await {
                Ok(b) => b,
                Err(e) => {
                    cu::error!("failed to read response body for injection: {e}");
                    return Ok(ServiceResponse::new(
                        req,
                        HttpResponse::InternalServerError().finish(),
                    ));
                }
            };
            let body = handler::inject_bootstrap(&body_bytes);
            Ok(ServiceResponse::new(
                req,
                handler::html_bytes_response(body),
            ))
        });
    }
}

// static WRAPPER: &str = include_str!("../../dist/index.html");
static ERROR_PAGE: &str = include_str!("404.html");

fn make_error_page(status: StatusCode) -> String {
    if status == StatusCode::NOT_FOUND {
        return ERROR_PAGE.into();
    }
    ERROR_PAGE
        .replacen("404", &status.as_u16().to_string(), 2)
        .replacen("Not Found", status.canonical_reason().unwrap_or("Error"), 2)
        .into()
}

// fn do_inject(content_str: &str) -> String {
//     let mut output = String::new();
//     let mut rest_wrapper = WRAPPER;
//
//     // // Replace <html> in wrapper with the opening <html ...> tag from content
//     // // to preserve any attributes (e.g. lang="en")
//     // if let Some(html_tag) = extract_html_tag(&content_str) {
//     //     replace_placeholder(&mut output, &mut rest_wrapper, "<html>", html_tag);
//     // }
//     //
//     // // If content has a <link rel="icon">, add it to the wrapper's <head>
//     // if let Some(link_icon_tag) = extract_link_icon_tag(&content_str) {
//     //     replace_placeholder(
//     //         &mut output,
//     //         &mut rest_wrapper,
//     //         "<!-- PLACEHOLDER_LINK_ICON -->",
//     //         link_icon_tag,
//     //     );
//     // }
//
//     // if !path.is_empty() {
//     //     replace_placeholder(
//     //         &mut output,
//     //         &mut rest_wrapper,
//     //         "<!-- PLACEHOLDER_PRELOAD -->",
//     //         &format!(r#"<link rel="preload" href="{path}?x-shwoop-is-raw=1">"#),
//     //     );
//     //
//     // }
//     //
//     // replace_placeholder(
//     //     &mut output,
//     //     &mut rest_wrapper,
//     //     "<!-- PLACEHOLDER_SERVER_FRAME -->",
//     //     &format!(r##"<iframe class="content-frame" allow="accelerometer; autoplay; bluetooth; camera; clipboard-read; clipboard-write; display-capture; encrypted-media; fullscreen; gamepad; geolocation; gyroscope; hid; identity-credentials-get; idle-detection; local-fonts; magnetometer; microphone; midi; payment; picture-in-picture; publickey-credentials-get; screen-wake-lock; serial; storage-access; usb; web-share; xr-spatial-tracking" sandbox="allow-downloads allow-forms allow-modals allow-orientation-lock allow-pointer-lock allow-popups allow-popups-to-escape-sandbox allow-presentation allow-same-origin allow-scripts allow-top-navigation allow-top-navigation-by-user-activation allow-top-navigation-to-custom-protocols" src="{}?x-shwoop-is-raw=1" style="z-index:99" onload="console.log('rendered in '+Math.floor(performance.now())+'ms')"></iframe>"##, path)
//     // );
//
//     output.push_str(rest_wrapper);
//     output
// }

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
