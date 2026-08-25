//! Frame exchange over the agent connection.
//!
//! The wire codec ([`st_ipc::write_message`] / [`st_ipc::read_message`]) is
//! stream-agnostic and supports unlimited request/response pairs per
//! connection; this module is the session helper's thin wrapper around that
//! fact. It is generic over `Read + Write` so the pairing discipline (write
//! one request, read exactly one response, no drift) is unit-testable over
//! in-memory streams, with no pipe involved.
//!
//! # Why a persistent connection at all
//!
//! The sampling front talks to the agent every second, forever; a
//! connect-per-frame helper would multiply pipe churn and lose the
//! "ordering guaranteed per connection" property the batching contract leans
//! on. The cost is that the client must handle the server closing the stream
//! whenever it pleases: any error on any frame marks the whole link dead and
//! the caller reconnects with backoff (see [`crate::backoff`]). That policy is
//! correct whether the agent serves one exchange per connection or many.

use std::io::{Read, Write};

use st_ipc::{Request, Response};

/// Send one request and wait for its response on `stream`.
///
/// Any codec or transport error is fatal to the link: framing state after a
/// failed read/write is unknowable, so callers must drop the stream rather
/// than reuse it.
pub fn round_trip<S: Read + Write>(stream: &mut S, request: &Request) -> st_ipc::Result<Response> {
    st_ipc::write_message(stream, request)?;
    st_ipc::read_message(stream)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    /// One end of an in-memory byte pipe. Bytes written here become readable
    /// at the peer end and vice versa, mimicking the named pipe's duplex
    /// behaviour closely enough to exercise multi-frame sequencing. Writes
    /// never block and reads of an empty buffer return EOF (`Ok(0)`), which is
    /// exactly how a dead pipe reads through the codec.
    #[derive(Clone)]
    struct End {
        inbound: Arc<Mutex<VecDeque<u8>>>,
        peer_inbound: Arc<Mutex<VecDeque<u8>>>,
    }

    fn duplex() -> (End, End) {
        let a = Arc::new(Mutex::new(VecDeque::new()));
        let b = Arc::new(Mutex::new(VecDeque::new()));
        (
            End {
                inbound: a.clone(),
                peer_inbound: b.clone(),
            },
            End {
                inbound: b,
                peer_inbound: a,
            },
        )
    }

    impl Read for End {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let mut q = self.inbound.lock().expect("inbound lock");
            let n = buf.len().min(q.len());
            for slot in buf.iter_mut().take(n) {
                *slot = q.pop_front().expect("n bounded by queue length");
            }
            Ok(n)
        }
    }

    impl Write for End {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.peer_inbound
                .lock()
                .expect("outbound lock")
                .extend(buf.iter().copied());
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Nothing on the wire reads as a closed peer, so a dead agent surfaces as
    /// [`st_ipc::IpcError::Closed`] rather than hanging or panicking.
    #[test]
    fn a_silent_peer_is_reported_as_a_closed_link() {
        let (mut client, _server) = duplex();
        assert!(matches!(
            round_trip(&mut client, &Request::Ping),
            Err(st_ipc::IpcError::Closed)
        ));
    }

    /// The property the persistent client depends on: several exchanges ride
    /// one stream in order, every request matched by exactly one response,
    /// with neither side drifting a frame.
    #[test]
    fn many_request_response_pairs_travel_over_one_connection() {
        let (mut client, mut server) = duplex();

        // The "agent" queues two replies up front; if round_trip mis-paired,
        // the second call would see the wrong response or hit EOF early.
        st_ipc::write_message(&mut server, &Response::Pong).expect("queue reply 1");
        st_ipc::write_message(
            &mut server,
            &Response::BlockedApps(st_ipc::BlockedAppsDto { blocked: vec![] }),
        )
        .expect("queue reply 2");

        assert!(matches!(
            round_trip(&mut client, &Request::Ping).expect("exchange 1"),
            Response::Pong
        ));
        assert!(matches!(
            round_trip(&mut client, &Request::BlockedApps).expect("exchange 2"),
            Response::BlockedApps(_)
        ));

        // Both requests landed on the same stream, in order, nothing lost.
        assert!(matches!(
            st_ipc::read_message::<_, Request>(&mut server).expect("request 1"),
            Request::Ping
        ));
        assert!(matches!(
            st_ipc::read_message::<_, Request>(&mut server).expect("request 2"),
            Request::BlockedApps
        ));
    }
}
