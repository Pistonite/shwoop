use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use actix_files::NamedFile;
use actix_web::middleware::Logger;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, guard, web};
use cu::str::PathExtension;

use crate::server::inject::InjectHotReloadMiddleware;
use crate::server::{self, Session, SessionMgr};
use crate::util::ThreadName;

struct ServerState {
    sessions: Arc<SessionMgr>,
    serve_path: PathBuf,
}

pub fn start(
    port: u16,
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
                App::new()
                    .app_data(state.clone())
                    .route(
                        "/",
                        web::get()
                            .guard(guard::fn_guard(server::is_websocket_upgrade))
                            .to(websocket_handler),
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
                    .wrap(InjectHotReloadMiddleware)
                    // .wrap(DefaultHeaders::new().add((
                    //     "Cache-Control", "max-age=0, stale-while-revalidate=300")))
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
