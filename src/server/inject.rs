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
            // don't inject to failure
            let status = res.status();
            if status.is_client_error() || status.is_server_error() {
                return Ok(res.map_into_boxed_body());
            }

            // only inject to html files
            let is_html = res
                .headers()
                .get(CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|ct| ct.starts_with("text/html"));

            if !is_html {
                return Ok(res.map_into_boxed_body());
            }

            let (http_req, http_res) = res.into_parts();
            let headers = http_res.headers().clone();

            let bytes = body::to_bytes(http_res.into_body())
                .await
                .map_err(actix_web::error::ErrorInternalServerError)?;

            let injected = do_inject(bytes.into());

            let mut builder = HttpResponse::build(status);
            for (name, value) in headers {
                builder.append_header((name, value));
            }
            let new_res = builder.body(injected);
            Ok(ServiceResponse::new(http_req, new_res))
        })
    }
}

fn do_inject(content: Vec<u8>) -> Vec<u8> {
    const SCRIPT: &[u8] = concat!(
        "<script type=\"module\">",
        include_str!("../../dist/index.js"),
        "</script>",
    ).as_bytes();

    let insert_pos = content
        .windows(b"</head>".len())
        .position(|w| w.eq_ignore_ascii_case(b"</head>"))
        .unwrap_or(content.len());

    let mut result = Vec::with_capacity(content.len() + SCRIPT.len());
    result.extend_from_slice(&content[..insert_pos]);
    result.extend_from_slice(SCRIPT);
    result.extend_from_slice(&content[insert_pos..]);
    result
}
