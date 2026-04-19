use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use actix_files::Files as FilesService;
use actix_web::middleware::Logger;
use actix_web::web::{self, Data as WebData};
use actix_web::{App, HttpServer};
use dashmap::DashMap;

use crate::server::SessionMgr;
use crate::server::handler::{self, ServerState};
use crate::server::inject::InjectHotReloadMiddleware;
use crate::util::ThreadName;

pub fn start(
    port: u16,
    raw: bool,
    local_only: bool,
    sessions: Arc<SessionMgr>,
    serve_path: PathBuf,
) -> JoinHandle<cu::Result<()>> {
    cu::debug!("starting http server on port {port}");
    thread::spawn(move || {
        cu::cli::set_thread_name(ThreadName::Server);
        actix_web::rt::System::new()
            .block_on(async move { run_server(port, raw, local_only, sessions, serve_path).await })
    })
}

async fn run_server(
    port: u16,
    raw: bool,
    local_only: bool,
    sessions: Arc<SessionMgr>,
    serve_path: PathBuf,
) -> cu::Result<()> {
    let is_file = serve_path.is_file();
    let num_cpus = thread::available_parallelism()
        .map(|x| usize::from(x))
        .unwrap_or(1);
    // don't need that many workers - use 25% of threads and at most 4
    // 4 threads -> 1 worker
    // 8 threads -> 2 workers
    // 16 threads -> 4 workers
    let num_workers = (num_cpus / 4 + 1).min(4);
    let state = WebData::new(ServerState {
        sessions,
        path: serve_path,
        path_is_webpage_cache: Arc::new(DashMap::default()),
    });

    // skip formatting so it's not too vertical
    #[rustfmt::skip]
    HttpServer::new(move || {
        cu::cli::set_thread_name(ThreadName::Server);
        // let serve_path = state.serve_path.clone();
        let state = state.clone();

        App::new().app_data(state.clone())

            // websocket route
            .route("/",
                web::get().guard(handler::websocket_guard()).to(handler::websocket))

            // hot-reload client JS
            .route(handler::js_source_path(), web::get().to(handler::js_source))
            .route(handler::js_sourcemap_path(), web::get().to(handler::js_sourcemap))

            // main file server
            .configure(move |cfg| {
                if is_file {
                    cfg.route("/",
                        web::get().to(handler::single_file)
                            .wrap(InjectHotReloadMiddleware::new(raw, state.path_is_webpage_cache.clone())),
                    );
                } else {
                    cfg.service(
                        web::scope("")
                            .service(
                                FilesService::new("/", &state.path)
                                    .show_files_listing()
                                    .index_file("index.html")
                                    .redirect_to_slash_directory()
                                    .prefer_utf8(true),
                            )
                            .wrap(InjectHotReloadMiddleware::new(raw, state.path_is_webpage_cache.clone()))
                    );
                }
            })
            .wrap(Logger::new("%s - %r").log_level(log::Level::Debug))
    })
    .workers(num_workers)
    .bind((if local_only { "127.0.0.1" } else { "0.0.0.0" }, port))?
    .run()
    .await?;
    cu::Ok(())
}
