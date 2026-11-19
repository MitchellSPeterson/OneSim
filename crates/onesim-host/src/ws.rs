//! WebSocket transport for the v2 API (docs/api.md): one JSON-RPC object per text frame.
//!
//! Per connection: the connection thread is the only one touching the socket (reads with a short
//! timeout, writes whatever is queued in between, so frames never interleave); a worker thread runs
//! requests in order and queues replies. Event notifications are queued by the session's `EventLog`.
// ponytail: the pump polls every `POLL` instead of blocking in epoll/select, so replies and events lag
// by up to 1 ms and an idle connection wakes 1000x/s (a 10 ms poll put a floor of ~11 ms on every
// call, too slow for a live 60 fps viewer). Upgrade: a nonblocking socket + `mio`/tokio.

use crate::rpc::Server;
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc};
use std::time::Duration;
use tungstenite::{Error, Message};

const POLL: Duration = Duration::from_millis(1);

/// Accept connections on `listener` forever, one thread per connection.
pub fn serve_ws(listener: TcpListener, server: Arc<Server>) -> std::io::Result<()> {
    for stream in listener.incoming() {
        let (stream, server) = (stream?, server.clone());
        std::thread::spawn(move || connection(stream, server));
    }
    Ok(())
}

fn connection(stream: TcpStream, server: Arc<Server>) {
    let _ = stream.set_nodelay(true);
    let Ok(mut ws) = tungstenite::accept(stream) else { return };
    if ws.get_ref().set_read_timeout(Some(POLL)).is_err() {
        return;
    }
    let (out_tx, out_rx) = mpsc::channel::<String>();
    let (req_tx, req_rx) = mpsc::channel::<String>();
    let mut conn = server.connect(out_tx.clone());
    std::thread::spawn(move || {
        for text in req_rx {
            if let Some(reply) = conn.handle(&text) {
                if out_tx.send(reply).is_err() {
                    break;
                }
            }
        }
        conn.close();
    });
    'pump: loop {
        while let Ok(text) = out_rx.try_recv() {
            if ws.send(Message::text(text)).is_err() {
                break 'pump;
            }
        }
        match ws.read() {
            Ok(Message::Text(t)) => {
                if req_tx.send(t.as_str().to_owned()).is_err() {
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => break,
        }
    }
    let _ = ws.close(None);
    let _ = ws.flush();
}
