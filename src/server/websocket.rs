use std::sync::Mutex;
use std::sync::atomic::AtomicU32;

use actix_web::web::Payload;
use actix_web::{HttpRequest, HttpResponse};
use actix_ws::Message;

static NEXT_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Default)]
pub struct SessionMgr {
    sessions: Mutex<Vec<Session>>,
}

impl SessionMgr {
    pub fn add(&self, session: Session) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.push(session);
    }

    pub async fn send_to_all(&self, msg: Msg) {
        // to avoid holding the lock across awaits, we will
        // take the existing sessions out, then after reloading
        // we will put them back
        let mut sessions: Vec<Session> = {
            let mut sessions = self.sessions.lock().unwrap();
            sessions.drain(..).collect()
        };
        let mut i = 0;
        while i < sessions.len() {
            if !sessions[i].send_msg(msg).await {
                sessions.swap_remove(i);
            } else {
                i += 1;
            }
        }
        if let Msg::Reload = msg {
            match sessions.len() {
                0 => cu::debug!("no active websocket sessions"),
                1 => cu::info!("notified 1 client to reload "),
                x => cu::info!("notified {x} clients to reload"),
            }
        }
        let mut self_sessions = self.sessions.lock().unwrap();
        self_sessions.extend(sessions);
    }
}

/// Connected Websocket sessions
pub struct Session {
    inner: actix_ws::Session,
}
impl Session {
    /// Connect an upgrade websocket request
    pub fn start(
        req: HttpRequest,
        body: Payload,
    ) -> Result<(Self, HttpResponse), actix_web::Error> {
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let (response, session, mut recv) = actix_ws::handle(&req, body)?;

        {
            let mut send = session.clone();
            actix_web::rt::spawn(async move {
                while let Some(msg) = recv.recv().await {
                    match msg {
                        Ok(Message::Ping(msg)) => {
                            if send.pong(&msg).await.is_err() {
                                break;
                            }
                        }
                        Ok(Message::Close(reason)) => {
                            let _ = send.close(reason).await;
                            break;
                        }
                        Err(e) => {
                            cu::debug!("ws connection [{}] error: {e}", id);
                            let _ = send.close(None).await;
                            break;
                        }
                        _ => {}
                    };
                }
                cu::debug!("ws connection [{}] closed", id);
            });
        }

        let session = Session { inner: session };

        Ok((session, response))
    }
    /// Send a message through the WebSocket session to reload the page
    /// Return true if sent successfully, false if the session has been closed
    pub async fn send_msg(&mut self, msg: Msg) -> bool {
        self.inner.text(msg.to_str()).await.is_ok()
    }
}

#[derive(Clone, Copy)]
pub enum Msg {
    /// Tell the Client to reload if build is succeeding
    Reload,
    /// Tell the Client build has started
    BuildStarted,
    /// Tell the Client build has failed
    BuildFailed,
    /// Tell the Client build has succeeded
    BuildSucceeded,
}

impl Msg {
    fn to_str(self) -> &'static str {
        match self {
            Msg::Reload => "reload",
            Msg::BuildStarted => "build-started",
            Msg::BuildFailed => "build-failed",
            Msg::BuildSucceeded => "build-succeeded",
        }
    }
}
