pub enum ThreadName {
    Watchexec,
    Reloader,
    Server,
}

impl From<ThreadName> for String {
    fn from(value: ThreadName) -> Self {
        match value {
            ThreadName::Watchexec => "wch".into(),
            ThreadName::Reloader => "rld".into(),
            ThreadName::Server => "srv".into(),
        }
    }
}

pub fn local_ip() -> cu::Result<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    Ok(socket.local_addr()?.ip())
}
