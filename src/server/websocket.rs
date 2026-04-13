use actix::{Actor, Addr, Handler, Message, StreamHandler, ActorContext, AsyncContext};
use actix_web::{HttpRequest, HttpResponse, web};
use actix_web_actors::ws;

/// Connected Websocket sessions
pub struct Session {
    inner_recv: oneshot::Receiver<Addr<SessionInternal>>,
    inner: Option<Addr<SessionInternal>>
}
impl Session {
    /// Connect an upgrade websocket request
    pub fn start(req: HttpRequest, stream: web::Payload) -> Result<(Self, HttpResponse), actix_web::Error> {
        let (send, recv) = oneshot::channel();
        let inner = SessionInternal {
            send: Some(send)
        };
        let response = ws::start(inner, &req, stream)?;
        let session = Session {
            inner_recv: recv,
            inner: None
        };

        Ok((session, response))
    }
    /// Check if the session has already been stopped so it can be cleaned up
    pub fn stopped(&self) -> bool {
        match &self.inner {
            Some(inner) => !inner.connected(),
            // not connected yet
            None => self.inner_recv.is_closed()
        }
    }
    /// Send a message through the WebSocket session to reload the page
    pub fn reload(&mut self) {
        cu::info!("seinding reload");
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

struct SessionInternal {
    send: Option<oneshot::Sender<Addr<Self>>>
}
impl Actor for SessionInternal {
    type Context = ws::WebsocketContext<Self>;

     fn started(&mut self, ctx: &mut Self::Context) {
        cu::info!("websocket connection started");
        if let Some(send) = self.send.take() {
            let _ = send.send(ctx.address());
        }
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        cu::info!("websocket connection stopped");
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
                cu::info!("recicing stopped from client");
                ctx.close(reason);
                ctx.stop();
            }
            Err(e) => {
                cu::warn!("ws protocol error when receiving from client: {e}");
            }
            // ignore text/binary messages
            _ => {}
        }
    }
}
