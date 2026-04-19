use std::ops::ControlFlow;
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cu::pre::*;

use crate::server::SessionMgr;

pub fn start(
    sessions: Arc<SessionMgr>,
    build_command: Vec<String>,
) -> (ReloadEventSender, JoinHandle<cu::Result<()>>) {
    let (send, recv) = mpsc::channel();

    let handle = thread::spawn(move || {
        let runtime = cu::check!(tokio::runtime::LocalRuntime::new(), "failed to create runtime for reloader")?;
        runtime.block_on(async move {
            cu::debug!("reloader started");
            let mut state = State::Idle;
            let mut should_build = false;
            loop {
                let result = poll_next_event_with_state(
                    &mut state,
                    &mut should_build,
                    &recv,
                    &sessions,
                    &build_command,
                ).await;
                let event = match result {
                    ControlFlow::Continue(false) => break,
                    ControlFlow::Continue(true) => continue,
                    ControlFlow::Break(event) => event,
                };
                match event {
                    ReloadEvent::Reload => {
                        state = State::ReloadScheduled;
                    }
                    ReloadEvent::BuildAndReload => {
                        should_build = true;
                        state = State::ReloadScheduled;
                    }
                }
            }
            cu::debug!("reloader stopped");
            cu::Ok(())
        })
    });
    (send, handle)
}

async fn poll_next_event_with_state(
    state: &mut State,
    should_build: &mut bool,
    recv: &ReloadEventReceiver,
    sessions: &SessionMgr,
    build_command: &[String],
) -> ControlFlow<ReloadEvent, bool> {
    match state {
        State::Idle => {
            // when idling, block until notified by next event
            match recv.recv() {
                Ok(event) => ControlFlow::Break(event),
                Err(_) => {
                    // sender has closed
                    ControlFlow::Continue(false)
                }
            }
        }
        _ => {
            match recv.try_recv() {
                Ok(event) => ControlFlow::Break(event),
                Err(mpsc::TryRecvError::Disconnected) => {
                    // sender has closed
                    ControlFlow::Continue(false)
                }
                Err(mpsc::TryRecvError::Empty) => {
                    match state {
                        State::ReloadScheduled => {
                            // wait for 200ms before last event to notify
                            // client. this also gives time for any file system
                            // updates to finish (such as files written to disk)
                            thread::sleep(Duration::from_millis(200));
                            *state = State::ReloadScheduleReached;
                        }
                        State::ReloadScheduleReached => {
                            if *should_build {
                                cu::info!("running build command");
                                *should_build = false;
                                if let Err(e) = run_build(build_command) {
                                    // don't reload if build failed
                                    cu::error!("{e:?}");
                                    *state = State::Idle;
                                } else {
                                    // build success, about to reload
                                    *state = State::ReloadScheduled;
                                }
                            } else {
                                // do notify the client
                                sessions.reload_all().await;
                                *state = State::Idle;
                            }
                        }
                        _ => {
                            *state = State::Idle;
                        }
                    };
                    ControlFlow::Continue(true)
                }
            }
        }
    }
}

pub fn run_build(build_command: &[String]) -> cu::Result<()> {
    let Some(build_command_bin) = build_command.iter().next() else {
        return Ok(());
    };
    let command = Path::new(build_command_bin)
        .command()
        .args(build_command.iter().skip(1))
        .stdoe(cu::pio::spinner("building").debug())
        .stdin_null();
    let (child, progress, _) = cu::check!(command.spawn(), "failed to spawn build command")?;
    cu::check!(child.wait_nz(), "build command failed")?;
    progress.done();
    Ok(())
}

pub enum ReloadEvent {
    Reload,
    BuildAndReload,
}

pub type ReloadEventSender = mpsc::Sender<ReloadEvent>;
pub type ReloadEventReceiver = mpsc::Receiver<ReloadEvent>;

enum State {
    /// Waiting for event
    Idle,
    /// Will to do a reload if no more events are coming after some time
    ReloadScheduled,
    /// Will do a reload now if there are no pending events
    ReloadScheduleReached,
}
