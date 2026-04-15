use std::sync::{Mutex, atomic::AtomicU32};

use actix::{Actor, ActorContext, Addr, AsyncContext, Handler, Message, StreamHandler};
use actix_web::guard::GuardContext;
use actix_web::{HttpRequest, HttpResponse, web};
use actix_web_actors::ws;

pub fn is_websocket_upgrade(ctx: &GuardContext) -> bool {
    ctx.head()
        .headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

#[derive(Default)]
pub struct SessionMgr {
    sessions: Mutex<Vec<Session>>,
}

impl SessionMgr {
    pub fn add(&self, session: Session) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.push(session);
    }

    pub fn reload_all(&self) {
        let mut sessions = self.sessions.lock().unwrap();
        sessions.retain_mut(|s| !s.stopped());
        if sessions.is_empty() {
            drop(sessions);
            cu::debug!("no active websocket sessions");
            return;
        }
        cu::info!("notifying clients to reload...");
        for session in sessions.iter_mut() {
            session.reload();
        }
    }
}

/// Connected Websocket sessions
pub struct Session {
    inner_recv: oneshot::Receiver<Addr<SessionInternal>>,
    inner: Option<Addr<SessionInternal>>,
}
impl Session {
    /// Connect an upgrade websocket request
    pub fn start(
        req: HttpRequest,
        stream: web::Payload,
    ) -> Result<(Self, HttpResponse), actix_web::Error> {
        let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let (send, recv) = oneshot::channel();
        let inner = SessionInternal {
            id,
            send: Some(send),
        };
        let response = ws::start(inner, &req, stream)?;
        let session = Session {
            inner_recv: recv,
            inner: None,
        };

        Ok((session, response))
    }
    /// Check if the session has already been stopped so it can be cleaned up
    pub fn stopped(&self) -> bool {
        match &self.inner {
            Some(inner) => !inner.connected(),
            // not connected yet
            None => self.inner_recv.is_closed(),
        }
    }
    /// Send a message through the WebSocket session to reload the page
    pub fn reload(&mut self) {
        if self.inner.is_none() {
            if let Ok(inner) = self.inner_recv.try_recv() {
                self.inner = Some(inner);
            }
        }
        let Some(inner) = self.inner.as_mut() else {
            return;
        };
        inner.do_send(ReloadMessage);
    }
}

static NEXT_ID: AtomicU32 = AtomicU32::new(1);
struct SessionInternal {
    id: u32,
    send: Option<oneshot::Sender<Addr<Self>>>,
}
impl Actor for SessionInternal {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        cu::debug!("websocket connection [{}] started", self.id);
        if let Some(send) = self.send.take() {
            let _ = send.send(ctx.address());
        }
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        cu::debug!("websocket connection [{}] stopped", self.id);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct ReloadMessage;
impl Handler<ReloadMessage> for SessionInternal {
    type Result = ();

    fn handle(&mut self, _: ReloadMessage, ctx: &mut Self::Context) -> Self::Result {
        ctx.text("reload");
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for SessionInternal {
    fn handle(&mut self, item: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match item {
            Ok(ws::Message::Ping(msg)) => ctx.pong(&msg),
            Ok(ws::Message::Close(reason)) => {
                ctx.close(reason);
                ctx.stop();
            }
            Err(e) => {
                cu::debug!("ws connection [{}] error: {e}", self.id);
                ctx.close(None);
                ctx.stop();
            }
            // ignore text/binary messages
            _ => {}
        }
    }
}
