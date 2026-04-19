use std::pin::Pin;

use actix_web::body::{self, BoxBody, MessageBody};
use actix_web::dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready};
use actix_web::http::StatusCode;
use actix_web::HttpResponse;

use crate::server::handler;

// --- this bucket of trait soup below is boilerplate for a middleware
pub struct InjectHotReloadMiddleware {
    raw: bool,
}
impl InjectHotReloadMiddleware {
    pub fn new(raw: bool) -> Self {
        Self {
            raw,
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
            service,
        }))
    }
}

pub struct InjectService<S> {
    raw: bool,
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
                        handler::html_response(body),
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
                handler::html_response(body),
            ))
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
        .into()
}
