use std::path::{Path, PathBuf};
use std::time::Duration;

use cu::pre::*;
use dashmap::DashMap;
use watchexec::Watchexec;
use watchexec_events::{FileType, Source, Tag, filekind::FileEventKind};
use watchexec_signals::Signal;

use crate::reloader::{ReloadEvent, ReloadEventSender};

pub fn start(
    path: PathBuf,
    source_paths: Vec<PathBuf>,
    has_build_step: bool,
    reload_sender: ReloadEventSender,
    stop_signal: oneshot::Receiver<()>,
) -> cu::co::Handle<cu::Result<()>> {
    cu::co::spawn(async move {
        Watcher {
            path,
            source_paths,
            has_build_step,
            reload_sender,
            stop_signal,
        }
        .start()
        .await
    })
}

pub struct Watcher {
    stop_signal: oneshot::Receiver<()>,
    path: PathBuf,
    source_paths: Vec<PathBuf>,
    has_build_step: bool,
    reload_sender: ReloadEventSender,
}

impl Watcher {
    pub async fn start(self) -> cu::Result<()> {
        cu::info!("watcher started");
        let wx = {
            let output_path = self.path.clone();
            let files_in_output_cache = DashMap::<String, bool>::default();
            let has_build_step = self.has_build_step;
            let reload_sender = self.reload_sender;
            Watchexec::new(move |mut action| {
                if action.signals().any(|sig| sig == Signal::Interrupt) {
                    action.quit();
                }

                let mut filtered_event_paths = action.events.iter().filter_map(|event| {
                    let mut has_file_event = false;
                    let mut the_path = None;
                    for tag in &event.tags {
                        match tag {
                            Tag::Path { path, file_type } => {
                                if file_type
                                    .is_none_or(|t| t == FileType::File || t == FileType::Symlink)
                                {
                                    the_path = Some(path);
                                }
                            }
                            // keep only relavant sources
                            Tag::Source(Source::Filesystem | Source::Os) => {}
                            Tag::Source(_) => {
                                cu::trace!("event filtered out [by source]: {event:?}");
                                return None;
                            }

                            Tag::FileEventKind(
                                FileEventKind::Create(_)
                                | FileEventKind::Modify(_)
                                | FileEventKind::Remove(_),
                            ) => {
                                has_file_event = true;
                            }

                            _ => {}
                        }
                    }

                    let Some(path) = the_path else {
                        cu::trace!("event filtered out [by path]: {event:?}");
                        return None;
                    };
                    if !has_file_event {
                        cu::trace!("event filtered out [by file event kind]: {event:?}");
                        return None;
                    }

                    cu::debug!("event: {event:?}");
                    Some(path)
                });

                let refresh_type = if has_build_step {
                    let mut refresh_type = RefreshType::None;
                    for path in filtered_event_paths {
                        let full_path = output_path.join(path);
                        let full_path_str = full_path.to_string_lossy();
                        let is_output = match files_in_output_cache.get(full_path_str.as_ref()) {
                            Some(x) => *x,
                            None => {
                                let Ok(is_output) = is_path_within(&output_path, &full_path) else {
                                    continue;
                                };
                                files_in_output_cache.insert(full_path_str.to_string(), is_output);
                                is_output
                            }
                        };
                        if is_output {
                            if refresh_type != RefreshType::Source {
                                refresh_type = RefreshType::Output;
                            }
                        } else {
                            refresh_type = RefreshType::Source;
                            break;
                        }
                    }
                    refresh_type
                } else {
                    if filtered_event_paths.next().is_some() {
                        RefreshType::Output
                    } else {
                        RefreshType::None
                    }
                };

                match refresh_type {
                    RefreshType::None => {}
                    RefreshType::Output => {
                        let _ = reload_sender.send(ReloadEvent::Reload);
                    }
                    RefreshType::Source => {
                        let _ = reload_sender.send(ReloadEvent::BuildAndReload);
                    }
                }

                cu::cli::reset_thread_name();
                action
            })?
        };

        wx.config
            .pathset(std::iter::once(self.path).chain(self.source_paths));
        wx.config.throttle(Duration::from_millis(100));
        cu::co::select! {
            result = wx.main() => {
                let result = cu::check!(result, "watchexec join error")?;
                cu::check!(result, "watchexec critical error")?;
            }
            _ = self.stop_signal => {
            }
        };
        cu::info!("watcher stopped");

        Ok(())
    }
}

#[derive(PartialEq)]
enum RefreshType {
    None,
    Output,
    Source,
}

fn is_path_within(target_path: &Path, event_path: &Path) -> cu::Result<bool> {
    let metadata = target_path.metadata()?;
    if metadata.is_file() {
        // check if event path is the same file
        let p = event_path.normalize()?;
        return Ok(target_path == p);
    }

    let event_rel_path = event_path.try_to_rel_from(target_path);
    if event_rel_path.is_absolute() {
        return Ok(false);
    }
    if event_rel_path.starts_with("..") {
        return Ok(false);
    }

    Ok(true)
}
