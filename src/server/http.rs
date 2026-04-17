use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use actix_files::NamedFile;
use actix_web::middleware::{DefaultHeaders, Logger};
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, guard, web};
use cu::str::PathExtension;

use crate::server::inject::InjectHotReloadMiddleware;
use crate::server::{self, Session, SessionMgr};
use crate::util::ThreadName;

struct ServerState {
    sessions: Arc<SessionMgr>,
    serve_path: PathBuf,
}

// note on chunk name:
// Ideally, the JS should be inlined into the HTML, so the server only
// needs to route HTML pages through the wrapper page for hot-reload to work.
// The wrapper HTML would be self-contained, and the server doesn't need
// additional routes to handle hot-reload related files (since they
// could collide with the actual files to serve).
// However, there is just one issue - when inlining the script directly
// into a script tag and inlining the sourcemap as a data URL into the script,
// the source label stops working. All lines got mapped to the last line of main.ts
// for some reason.
// I don't have time to fix it right now, so here's a workaround to make
// a path that's unlikely to collide, and have the server serve the JS
// and the source map
static JS_CHUNK_NAME: &str = "bf52606e-9f65-4f5e-8a2f-016ad7cfa92a_please_dont_collide";

pub fn start(
    port: u16,
    raw: bool,
    local_only: bool,
    sessions: Arc<SessionMgr>,
    serve_path: PathBuf,
) -> JoinHandle<cu::Result<()>> {
    let is_file = serve_path.is_file();
    cu::debug!("starting http server on port {port}");
    let num_cpus = thread::available_parallelism()
        .map(|x| usize::from(x))
        .unwrap_or(1);
    // don't need that many workers - use 25% of threads and at most 4
    // 4 threads -> 1 worker
    // 8 threads -> 2 workers
    // 16 threads -> 4 workers
    let num_workers = (num_cpus / 4 + 1).min(4);
    thread::spawn(move || {
        cu::cli::set_thread_name(ThreadName::Server);
        actix_web::rt::System::new().block_on(async {
            let state = web::Data::new(ServerState {
                sessions,
                serve_path,
            });
            HttpServer::new(move || {
                cu::cli::set_thread_name(ThreadName::Server);
                let serve_path = state.serve_path.clone();
                let js_chunk_path = format!("/{JS_CHUNK_NAME}.js");
                let js_sourcemap_path = format!("/{JS_CHUNK_NAME}.js.map");
                App::new()
                    .app_data(state.clone())
                    .route(
                        "/",
                        web::get()
                            .guard(guard::fn_guard(server::is_websocket_upgrade))
                            .to(websocket_handler),
                    )
                    .route(
                        &js_chunk_path,
                        web::get().to(js_chunk_handler)
                    )
                    .route(
                        &js_sourcemap_path,
                        web::get().to(js_sourcemap_handler)
                    )
                    .configure(move |cfg| {
                        if is_file {
                            cfg.route("/", web::get().to(single_file_handler));
                            if let Ok(parent) = serve_path.parent_abs() {
                                cfg.service(
                                    actix_files::Files::new("/", &parent).prefer_utf8(true),
                                );
                            }
                        } else {
                            cfg.service(
                                actix_files::Files::new("/", &serve_path)
                                    .show_files_listing()
                                    .index_file("index.html")
                                    .redirect_to_slash_directory()
                                    .prefer_utf8(true),
                            );
                        }
                    })
                    .wrap(DefaultHeaders::new().add(("Cache-Control", "no-store")))
                    .wrap(InjectHotReloadMiddleware::new(!raw))
                    .wrap(Logger::new("%s - %r"))
            })
            .workers(num_workers)
            .bind((if local_only { "127.0.0.1" } else { "0.0.0.0" }, port))?
            .run()
            .await?;
            cu::Ok(())
        })?;
        cu::Ok(())
    })
}

/// Handle incoming websocket open requests
async fn websocket_handler(
    req: HttpRequest,
    stream: web::Payload,
    state: web::Data<ServerState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (session, response) = Session::start(req, stream)?;
    state.sessions.add(session);
    Ok(response)
}

async fn single_file_handler(
    req: HttpRequest,
    state: web::Data<ServerState>,
) -> Result<HttpResponse, actix_web::Error> {
    Ok(NamedFile::open(&state.serve_path)?.into_response(&req))
}

async fn js_chunk_handler() -> HttpResponse {
    HttpResponse::Ok().append_header(("Content-Type", "text/javascript")).body(include_str!("../../dist/bf52606e-9f65-4f5e-8a2f-016ad7cfa92a_please_dont_collide.js"))
}
async fn js_sourcemap_handler() -> HttpResponse {
    HttpResponse::Ok().body(include_str!("../../dist/bf52606e-9f65-4f5e-8a2f-016ad7cfa92a_please_dont_collide.js.map"))
}
