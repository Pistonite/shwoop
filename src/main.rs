use actix_web::middleware::Logger;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use cu::pre::*;
use std::sync::Mutex;
use std::thread;

mod args;
mod server;

use server::websocket::Session;

struct AppState {
    sessions: Mutex<Vec<Session>>,
}

fn is_ws_upgrade(req: &HttpRequest) -> bool {
    req.headers()
        .get(actix_web::http::header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

async fn ws_route(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    if !is_ws_upgrade(&req) {
        let body = std::fs::read("test-site/index.html")
            .map_err(actix_web::error::ErrorInternalServerError)?;
        return Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(body));
    }
    let (session, response) = Session::start(req, stream)?;
    state.sessions.lock().unwrap().push(session);
    Ok(response)
}

async fn js_route() -> Result<HttpResponse, actix_web::Error> {
    let body = std::fs::read("test-site/client.js")
        .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok()
        .content_type("text/javascript; charset=utf-8")
        .body(body))
}

async fn reload_route(state: web::Data<AppState>) -> HttpResponse {
    let mut sessions = state.sessions.lock().unwrap();
    sessions.retain_mut(|s| {
        let s = s.stopped();
        cu::info!("stopped: {s}");
        !s
    });
    for session in sessions.iter_mut() {
        session.reload();
    }
    HttpResponse::Ok().finish()
}

#[cu::cli]
fn main(args: args::Args) -> cu::Result<()> {
    cu::debug!("args: {args:#?}");

    // cu::co::block(future)

    // thread::spawn(move || {
        actix_web::rt::System::new().block_on(async {
            let state = web::Data::new(AppState {
                sessions: Mutex::new(vec![]),
            });
            HttpServer::new(move || {
                App::new()
                    .app_data(state.clone())
                    .route("/", web::get().to(ws_route))
                    .route("/client.js", web::get().to(js_route))
                    .route("/reload", web::post().to(reload_route))
                .wrap(Logger::default())
            })
                .workers(1)
                .bind(("0.0.0.0", 8241))?
                .run()
            .await?;
            cu::Ok(())
        })?;
    // });





    Ok(())
}
