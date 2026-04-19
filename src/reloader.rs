use std::ops::ControlFlow;
use std::path::Path;
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cu::pre::*;

use crate::server::{Msg, SessionMgr};

pub fn start(
    sessions: Arc<SessionMgr>,
    build_command: Vec<String>,
) -> (ReloadEventSender, JoinHandle<cu::Result<()>>) {
    let (send, recv) = mpsc::channel();

    let handle = thread::spawn(move || {
        let runtime = cu::check!(
            tokio::runtime::LocalRuntime::new(),
            "failed to create runtime for reloader"
        )?;
        runtime.block_on(async move {
            cu::debug!("started reloader");
            let mut state = State::Idle;
            let mut should_build = false;
            let mut was_build_failed = false;
            loop {
                let result = poll_next_event_with_state(
                    &mut state,
                    &mut should_build,
                    &mut was_build_failed,
                    &recv,
                    &sessions,
                    &build_command,
                )
                .await;
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
            cu::debug!("stopped reloader");
            cu::Ok(())
        })
    });
    (send, handle)
}

async fn poll_next_event_with_state(
    state: &mut State,
    should_build: &mut bool,
    was_build_failed: &mut bool,
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
                                *should_build = false;
                                sessions.send_to_all(Msg::BuildStarted).await;
                                if let Err(e) = run_build(build_command, *was_build_failed).await {
                                    let is_first_failure = !*was_build_failed;
                                    *was_build_failed = true;
                                    // don't reload if build failed
                                    cu::error!("{e:?}");
                                    if is_first_failure {
                                        cu::hint!("the output from the build command will be printed for debugging on the next run");
                                    }
                                    sessions.send_to_all(Msg::BuildFailed).await;
                                    *state = State::Idle;
                                } else {
                                    // build success, about to reload
                                    *was_build_failed = false;
                                    sessions.send_to_all(Msg::BuildSucceeded).await;
                                    *state = State::ReloadScheduled;
                                }
                            } else {
                                // do notify the client
                                sessions.send_to_all(Msg::Reload).await;
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

pub async fn run_build(build_command: &[String], print: bool) -> cu::Result<()> {
    let Some(build_command_bin) = build_command.iter().next() else {
        return Ok(());
    };
    let spinner = cu::pio::spinner("building");
    let spinner = match (cu::lv::D.enabled(), print) {
        (true, _) => {
            // always print the message
            spinner.debug()
        },
        (false, true) => {
            spinner.print()
        }
        (false, false) => {
            spinner
        }
    };
    let command = Path::new(build_command_bin)
        .command()
        .args(build_command.iter().skip(1))
        .stdoe(spinner)
        .stdin_null();
    let (child, progress, _) = cu::check!(command.co_spawn().await, "failed to spawn build command")?;
    if let Err(e) = child.co_wait_nz().await {
        if cu::lv::D.enabled() {
            cu::rethrow!(e, "build command failed");
        } else {
            cu::bail!("build command failed: {e}");
        }
    }
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
