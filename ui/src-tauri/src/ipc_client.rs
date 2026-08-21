//! Thin IPC client: connects to the agent's pipe and performs one
//! request/response exchange per command.
//!
//! The agent is the source of truth; this crate never touches the database or
//! any platform backend. Every command is: connect, send one frame, read one
//! response, close.

use st_ipc::{transport, Request, Response};

const PIPE_NAME: &str = "screentime";

/// Performs one request/response round trip against the agent.
///
/// Returns `None` when the agent is not running, so callers can present a
/// graceful "disconnected" state instead of failing hard.
pub fn request(request: Request) -> Result<Response, st_ipc::transport::TransportError> {
    let mut stream = transport::client_connect(PIPE_NAME)?;
    st_ipc::write_message(&mut stream, &request)
        .map_err(|e| st_ipc::transport::TransportError::Io(std::io::Error::other(e)))?;
    let response = st_ipc::read_message::<_, Response>(&mut stream)
        .map_err(|e| st_ipc::transport::TransportError::Io(std::io::Error::other(e)))?;
    Ok(response)
}
