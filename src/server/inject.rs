use std::path::PathBuf;
use std::pin::Pin;

use actix_web::HttpResponse;
use actix_web::body::{self, BoxBody, MessageBody};
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready};
use actix_web::http::StatusCode;
use cu::str::PathExtension;

use crate::server::handler;

// --- this bucket of trait soup below is boilerplate for a middleware
pub struct InjectHotReloadMiddleware {
    pub raw: bool,
    pub path: PathBuf,
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
            path: self.path.clone(),
            service,
        }))
    }
}

pub struct InjectService<S> {
    raw: bool,
    path: PathBuf,
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
            (|mut $res:ident| $code:stmt) => {{
                let fut = self.service.call(sreq);
                return Box::pin(async move {
                    let mut $res = fut.await?;
                    $code
                });
            }};
        }

        if self.raw || !handler::is_browser(req) {
            cu::info!("(passthrough) {pathname}");
            handle!(passthrough);
        }

        // call inner service to serve the file
        let path = self.path.clone();
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
            // check if the 404 path is a directory - show listing instead of error page
            if status == StatusCode::NOT_FOUND {
                if path.is_dir() {
                    let rel = res.request().path().trim_start_matches('/');
                    if let Ok(fs_path) = path.join(rel).normalize_exists() {
                        if fs_path.starts_with(&path) && fs_path.is_dir() {
                            cu::info!("200 OK - {} (directory listing)", res.request().uri());
                            let body = handler::directory_listing(&fs_path, res.request().path());
                            let body = handler::inject_bootstrap(body.as_bytes());
                            let (req, _) = res.into_parts();
                            return Ok(ServiceResponse::new(req, handler::html_response(body)));
                        }
                    }
                }
            }
            if status.is_client_error() || status.is_server_error() {
                // error (likely resource not found)
                if handler::probably_webpage(pathname) {
                    cu::error!("{} - {} (injected)", status, res.request().uri());
                    let body = make_error_page(status);
                    let body = handler::inject_bootstrap(body.as_bytes());
                    let (req, _) = res.into_parts();
                    return Ok(ServiceResponse::new(req, handler::html_response(body)));
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
            Ok(ServiceResponse::new(req, handler::html_response(body)))
        });
    }
}

static ERROR_PAGE: &str = include_str!("404.html");

fn make_error_page(status: StatusCode) -> String {
    if status == StatusCode::NOT_FOUND {
        return ERROR_PAGE.into();
    }
    ERROR_PAGE
        .replacen("404", &status.as_u16().to_string(), 2)
        .replacen("Not Found", status.canonical_reason().unwrap_or("Error"), 2)
}
