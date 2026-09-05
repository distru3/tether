//! The local DNS responder thread.
//!
//! Binds a UDP socket on `127.0.0.1:53` and answers every query with one of two
//! outcomes, both decided by [`crate::resolve::decision`] against the *current*
//! shared rule set:
//!
//! * **Block** → a hand-built `NXDOMAIN` reply echoing the question (RCODE 3).
//!   The query is answered for the queried *name* regardless of type, so an A,
//!   AAAA, MX or TXT lookup for a blocked host all come back `NXDOMAIN`.
//! * **Forward** → the original query bytes are re-sent to the upstream resolver
//!   (the machine's real DNS, captured before this filter overrode it) and the
//!   raw response is relayed back, with only the transaction id rewritten so a
//!   client can never confound a reply with one meant for a different query. No
//!   answer section is ever re-encoded — pass-through is the whole job.
//!
//! A query we cannot parse is forwarded verbatim (fail-open): breaking DNS for a
//! malformed-but-otherwise-fine question is worse than letting one odd shape
//! through, and a blocker must never detach the machine from the network.
//!
//! The thread is deliberately cancellable: it drains on a short read timeout so
//! `stop()` can rendezvous within a few hundred milliseconds, and `clear()` can
//! restore the system DNS without waiting for a slow upstream.
//!
//! # Why a separate thread
//!
//! The agent's main loop is a 1 Hz tick; blocking it on a DNS wait or an
//! answer would make enforcement feel laggy and, worse, let a slow upstream
//! stall the tick. A dedicated thread keeps the resolver hot and the enforcer
//! responsive, and a fresh read timeout per forward bounds stalls.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use st_core::platform::BlockRule;

use crate::resolve::Decision;
use crate::wire;

/// How long to wait on an upstream resolver before declaring it unanswered.
const UPSTREAM_TIMEOUT: Duration = Duration::from_secs(4);
/// How often the receive loop wakes to re-check `stop`, so `clear()` is prompt.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Max usable query/response datagram. Enough for EDNS-sized answers without
/// unbounded allocation per packet.
const MAX_DATAGRAM: usize = 4096;

/// A live resolver: the bound socket, the shared rules it reads, and the thread
/// running the receive loop. Presence of the struct means the socket bound OK.
pub struct Resolver {
    upstream: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Resolver {
    /// Bind `127.0.0.1:53` and start the worker thread.
    ///
    /// Returns the error wrapped up if the bind fails, so the caller
    /// ([`DnsProxyFilter`](crate::filter::DnsProxyFilter)) can degrade instead
    /// of crashing — the agent must keep running even when it cannot grab port 53.
    pub fn start(
        bind: &str,
        upstream: SocketAddr,
        rules: Arc<RwLock<Vec<BlockRule>>>,
    ) -> io::Result<Self> {
        let socket = UdpSocket::bind(bind)?;
        socket.set_read_timeout(Some(POLL_INTERVAL))?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = stop.clone();
            let rules = Arc::clone(&rules);
            std::thread::Builder::new()
                .name("dns-resolver".into())
                .spawn(move || receive_loop(socket, rules, upstream, stop))?
        };
        Ok(Self {
            upstream,
            stop,
            thread: Some(thread),
        })
    }

    /// The upstream resolver currently used for forwarding.
    pub fn upstream(&self) -> SocketAddr {
        self.upstream
    }

    /// Signal the thread to stop and wait for it to exit.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            // The worker drains within POLL_INTERVAL, so a bounded join is
            // acceptable; a hang would mean the socket never unblocks.
            let _ = thread.join();
        }
    }
}

/// The single receive/respond loop for a socket.
fn receive_loop(
    socket: UdpSocket,
    rules: Arc<RwLock<Vec<BlockRule>>>,
    upstream: SocketAddr,
    stop: Arc<AtomicBool>,
) {
    let mut buf = vec![0u8; MAX_DATAGRAM];
    while !stop.load(Ordering::Relaxed) {
        match socket.recv_from(&mut buf) {
            Ok((n, peer)) => {
                let query = &buf[..n];
                match evaluate(query, &rules) {
                    LocalResponse::NxDomain(bytes) => {
                        if let Err(e) = socket.send_to(&bytes, peer) {
                            tracing::warn!(error = %e, "dns resolver failed to send NXDOMAIN");
                        }
                    }
                    LocalResponse::Forward => {
                        let query_vec = query.to_vec();
                        let socket_clone = socket.try_clone().expect("clone socket");
                        std::thread::spawn(move || {
                            match forward(&query_vec, upstream) {
                                Some(mut resp) => {
                                    // The client sent the id; make sure it gets that same
                                    // id back, not whatever the upstream echoed.
                                    if let Ok(id) = wire::message_id(&query_vec) {
                                        wire::set_message_id(&mut resp, id);
                                    }
                                    if let Err(e) = socket_clone.send_to(&resp, peer) {
                                        tracing::warn!(error = %e, "dns resolver failed to relay response");
                                    }
                                }
                                None => {
                                    tracing::warn!(%peer, "dns resolver forward timed out or upstream unreachable");
                                }
                            }
                        });
                    }
                }
            }
            Err(e)
                if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => {
                tracing::warn!(error = %e, "dns resolver receive error");
                // Do not spin on a persistent error; let the polling timeout
                // govern the pace and re-check stop.
                std::thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

/// The outcome of evaluating a query against the rule set, before any I/O.
#[derive(Debug, PartialEq, Eq)]
enum LocalResponse {
    /// The name is blocked: reply with a hand-built NXDOMAIN.
    NxDomain(Vec<u8>),
    /// Not blocked (or unparsable): relay the query to the upstream resolver.
    Forward,
}

/// Decide what to do with `query` under `rules`. Pure apart from the socket-less
/// parsing, so the block/forward split is unit-testable without a listener.
fn evaluate(query: &[u8], rules: &RwLock<Vec<BlockRule>>) -> LocalResponse {
    let Ok((header, question)) = wire::parse_question(query) else {
        // Unparsable: the safe answer is to forward, never to answer NXDOMAIN
        // for a name we could not even read. Fail-open.
        return LocalResponse::Forward;
    };
    if crate::resolve::decision(
        &question.name,
        &rules.read().unwrap_or_else(|p| p.into_inner()),
    ) == Decision::Block
    {
        LocalResponse::NxDomain(wire::build_nxdomain(header.id, &question))
    } else {
        LocalResponse::Forward
    }
}

/// Relay `query` to `upstream` and read back the raw response. Returns `None` on
/// send/recv timeout. The transaction id is NOT rewritten here — the caller does
/// it so it can also consume the original query's id.
fn forward(query: &[u8], upstream: SocketAddr) -> Option<Vec<u8>> {
    let client = UdpSocket::bind("0.0.0.0:0").ok()?;
    client.set_read_timeout(Some(UPSTREAM_TIMEOUT)).ok()?;
    client.send_to(query, upstream).ok()?;
    let mut resp = vec![0u8; MAX_DATAGRAM];
    let (n, _from) = client.recv_from(&mut resp).ok()?;
    resp.truncate(n);
    Some(resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(domain: &str, include_subdomains: bool) -> BlockRule {
        BlockRule {
            domain: domain.into(),
            include_subdomains,
        }
    }

    fn ruleset(rules: Vec<BlockRule>) -> RwLock<Vec<BlockRule>> {
        RwLock::new(rules)
    }

    /// A tiny A query for `name` with id 0xbeef.
    fn a_query(name: &str) -> Vec<u8> {
        let qname = wire::encode_name(name).expect("encode");
        let mut out = Vec::new();
        out.extend_from_slice(&0xbeefu16.to_be_bytes());
        out.extend_from_slice(&0x0100u16.to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&qname);
        out.extend_from_slice(&1u16.to_be_bytes()); // A
        out.extend_from_slice(&1u16.to_be_bytes()); // IN
        out
    }

    #[test]
    fn a_blocked_name_yields_nxdomain() {
        let rules = ruleset(vec![rule("example.com", true)]);
        let outcome = evaluate(&a_query("a.example.com"), &rules);
        let LocalResponse::NxDomain(bytes) = outcome else {
            panic!("expected NXDOMAIN");
        };
        let header = wire::parse_header(&bytes).expect("header");
        assert_eq!(header.id, 0xbeef);
        assert_eq!(header.flags, wire::NXDOMAIN_FLAGS);
    }

    #[test]
    fn a_passed_name_is_forwarded() {
        let rules = ruleset(vec![rule("example.com", true)]);
        assert_eq!(
            evaluate(&a_query("google.com"), &rules),
            LocalResponse::Forward
        );
    }

    #[test]
    fn an_uncovered_exact_rule_does_not_block_subdomains() {
        let rules = ruleset(vec![rule("example.com", false)]);
        assert_eq!(
            evaluate(&a_query("a.example.com"), &rules),
            LocalResponse::Forward
        );
    }

    #[test]
    fn an_unparsable_query_is_forwarded_not_failed() {
        let rules = ruleset(vec![rule("example.com", true)]);
        // Too short to even be a header: must forward, never answer.
        assert_eq!(evaluate(&[0u8; 8], &rules), LocalResponse::Forward);
    }
}
