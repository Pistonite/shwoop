use std::path::{Path, PathBuf};
use std::sync::Arc;

use cu::pre::*;

use crate::server::SessionMgr;

#[cu::cli(log_config = |_| args::LogConfig)]
fn main(args: args::Args) -> cu::Result<()> {
    cu::debug!("args: {args:#?}");

    let path = Path::new(&args.path).normalize()?;
    if !args.command.is_empty() {
        let runtime = cu::check!(
            tokio::runtime::LocalRuntime::new(),
            "failed to create runtime for initial build"
        )?;
        let command = args.command.clone();
        let result = runtime.block_on(async move { reloader::run_build(&command, true).await });
        if let Err(e) = result {
            cu::error!("initial build failed: {e:?}");
            cu::info!("trying to start server anyway...");
        }
    }
    match path.metadata() {
        Ok(m) if m.is_file() => {
            cu::warn!(
                "input path is a file - only this file will be served and not referenced assets."
            );
        }
        Err(_) => {
            cu::bail!("the path to serve does not exist: '{}'", path.display());
        }
        _ => {}
    }
    let watch_for_build = !args.command.is_empty() && !args.watch.is_empty();

    let sessions = Arc::new(SessionMgr::default());

    let port = args.port;
    let host_address = if args.host {
        match server::local_ip() {
            Ok(ip) => format!("http://{ip}:{port}"),
            Err(e) => {
                cu::warn!("failed to get local ip address: {e:?}");
                format!("http://localhost:{port}")
            }
        }
    } else {
        format!("http://localhost:{port}")
    };

    let server_thread = server::start(
        args.port,
        args.raw,
        !args.host,
        Arc::clone(&sessions),
        path.clone(),
    );
    cu::hint!("webpage hosted at {host_address}");
    if !args.host {
        cu::hint!("use --host to expose the server to local network");
    }
    if args.raw {
        cu::hint!("acting as plain server because of --raw; not starting watcher and reloader");
        // actix will handle the interrupt signal if started successfully
        let server_result = match server_thread.join() {
            Ok(x) => x,
            Err(_) => Err(cu::fmterr!("server thread panicked")),
        };
        cu::check!(server_result, "failed to run http server")?;
    } else {
        let (reload_sender, reloader_thread) = reloader::start(sessions, args.command);
        let (stop_send, stop_recv) = oneshot::channel();

        let source_paths = args.watch.into_iter().map(PathBuf::from).collect();
        let watcher = watcher::start(
            path,
            source_paths,
            watch_for_build,
            reload_sender,
            stop_recv,
        );

        // actix will handle the interrupt signal if started successfully
        let server_result = match server_thread.join() {
            Ok(x) => x,
            Err(_) => Err(cu::fmterr!("server thread panicked")),
        };
        // stop and join the watcher
        let _ = stop_send.send(());
        let watcher_result = watcher.join();
        // stop and join the reloader
        let _ = reloader_thread.join();

        // prioritize showing error related to the server
        cu::check!(server_result, "failed to run http server")?;

        // propagate any watchexec errors
        watcher_result??;
    }
    // no need to print time for successful exits
    cu::lv::disable_print_time();

    cu::info!("shutdown complete");
    Ok(())
}

mod args;
mod reloader;
mod server;
mod watcher;
