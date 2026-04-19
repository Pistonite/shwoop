use std::sync::Mutex;
use std::sync::atomic::AtomicU32;

use actix_web::{HttpRequest, HttpResponse};
use actix_web::web::Payload;
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

    pub async fn reload_all(&self) {
        let mut sessions = self.sessions.lock().unwrap();
        let mut i = 0;
        cu::info!("notifying clients to reload...");
        while i < sessions.len() {
            if !sessions[i].reload().await {
                sessions.swap_remove(i);
            } else {
                i+=1;
            }
        }
        if sessions.is_empty() {
            drop(sessions);
            cu::debug!("no active websocket sessions");
            return;
        }

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

        let session = Session {
            inner: session
        };

        Ok((session, response))
    }
    /// Send a message through the WebSocket session to reload the page
    /// Return true if sent successfully, false if the session has been closed
    pub async fn reload(&mut self) -> bool {
        self.inner.text("reload").await.is_ok()
    }
}
