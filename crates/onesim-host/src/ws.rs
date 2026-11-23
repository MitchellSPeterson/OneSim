//! WebSocket transport for the v2 API (docs/api.md): one JSON-RPC object per text frame.
//!
//! Per connection: the connection thread owns the `WebSocket` and blocks in `poll(2)` on the
//! socket and a wake pipe (no busy-polling); a worker thread runs requests in order and queues
//! replies, and a tiny bridge forwards outbound messages to the wake pipe.

use crate::rpc::Server;
#[cfg(unix)]
use std::collections::VecDeque;
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc};
#[cfg(unix)]
use std::sync::Mutex;
use tungstenite::{Error, Message};

/// Accept connections on `listener` forever, one thread per connection.
pub fn serve_ws(listener: TcpListener, server: Arc<Server>) -> std::io::Result<()> {
    for stream in listener.incoming() {
        let (stream, server) = (stream?, server.clone());
        std::thread::spawn(move || connection(stream, server));
    }
    Ok(())
}

#[cfg(unix)]
fn connection(stream: TcpStream, server: Arc<Server>) {
    use std::os::fd::AsRawFd;

    let _ = stream.set_nodelay(true);
    let Ok(mut ws) = tungstenite::accept(stream) else { return };
    if ws.get_ref().set_nonblocking(true).is_err() {
        return;
    }

    let mut pipe_fds = [0i32; 2];
    if unsafe { libc::pipe(pipe_fds.as_mut_ptr()) } != 0 {
        return;
    }
    let (wake_r, wake_w) = (pipe_fds[0], pipe_fds[1]);
    for fd in [wake_r, wake_w] {
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFL);
            if flags >= 0 {
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }
    }

    let (out_tx, out_rx) = mpsc::channel::<String>();
    let (req_tx, req_rx) = mpsc::channel::<String>();
    let out_queue = Arc::new(Mutex::new(VecDeque::<String>::new()));

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

    // Bridge mpsc `out_rx` into the wake pipe so `poll(2)` wakes immediately on any reply or event.
    let q = out_queue.clone();
    std::thread::spawn(move || {
        let byte = 1u8;
        for text in out_rx {
            q.lock().unwrap_or_else(|e| e.into_inner()).push_back(text);
            unsafe {
                libc::write(wake_w, &byte as *const u8 as *const libc::c_void, 1);
            }
        }
        unsafe {
            libc::close(wake_w);
        }
    });

    let sock_fd = ws.get_ref().as_raw_fd();
    let mut fds = [
        libc::pollfd { fd: sock_fd, events: libc::POLLIN, revents: 0 },
        libc::pollfd { fd: wake_r, events: libc::POLLIN, revents: 0 },
    ];

    'pump: loop {
        // Drain any queued outbound frames first.
        loop {
            let next = out_queue.lock().unwrap_or_else(|e| e.into_inner()).pop_front();
            let Some(text) = next else { break };
            // Temporarily ensure blocking write so partial nonblocking writes don't fail.
            let _ = ws.get_ref().set_nonblocking(false);
            let res = ws.send(Message::text(text));
            let _ = ws.get_ref().set_nonblocking(true);
            if res.is_err() {
                break 'pump;
            }
        }

        // Drain any already-buffered / readable incoming frames before sleeping in poll.
        loop {
            match ws.read() {
                Ok(Message::Text(t)) => {
                    if req_tx.send(t.as_str().to_owned()).is_err() {
                        break 'pump;
                    }
                }
                Ok(Message::Close(_)) => break 'pump,
                Ok(_) => {}
                Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break 'pump,
            }
        }

        fds[0].revents = 0;
        fds[1].revents = 0;
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if fds[1].revents & libc::POLLIN != 0 {
            let mut buf = [0u8; 64];
            while unsafe { libc::read(wake_r, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) } > 0 {}
        }
        if fds[0].revents & (libc::POLLHUP | libc::POLLERR) != 0 && fds[0].revents & libc::POLLIN == 0 {
            break;
        }
    }

    let _ = ws.get_ref().set_nonblocking(false);
    let _ = ws.close(None);
    let _ = ws.flush();
    unsafe {
        libc::close(wake_r);
    }
}

#[cfg(not(unix))]
fn connection(stream: TcpStream, server: Arc<Server>) {
    use std::time::Duration;
    const POLL: Duration = Duration::from_millis(5);
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
