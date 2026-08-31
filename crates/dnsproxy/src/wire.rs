//! Minimal DNS wire-format helpers for the local responder.
//!
//! Kept I/O-free so the byte layout is provable in a unit test without binding a
//! socket. The responder only needs three things it can write by hand:
//!
//! 1. parse a query's header (12 bytes) and QNAME (label sequence),
//! 2. build an `NXDOMAIN` response that echoes the question back, and
//! 3. rewrite the transaction id on a forwarded response so a client can never
//!    confuse it with a reply to a different query.
//!
//! Forwarding itself needs no re-encoding: the responder re-sends the client's
//! original query bytes to the upstream resolver and relays the raw response
//! (after fixing the id), so we never build an answer or authority section.

use std::fmt;

/// Errors a client query can trip in the parser. The responder treats any parse
/// failure as "cannot decide, forward verbatim" — fail-open, because a blocker
/// that drops valid queries it does not understand is worse than one that lets a
/// suspicious (rare) shape through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireError {
    TooShort,
    IdNotSupported,
    BadLabel,
    BadName,
}

impl fmt::Display for WireError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            WireError::TooShort => "message is shorter than a DNS header",
            WireError::IdNotSupported => "message has no transaction id",
            WireError::BadLabel => "label length byte is malformed",
            WireError::BadName => "name is too long or empty",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for WireError {}

/// The fixed 12-byte DNS header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DnsHeader {
    pub id: u16,
    pub flags: u16,
    pub qdcount: u16,
    pub ancount: u16,
    pub nscount: u16,
    pub arcount: u16,
}

/// The single question section of a query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsQuestion {
    /// Normalised (lower-cased, trailing dot stripped) name.
    pub name: String,
    pub qtype: u16,
    pub qclass: u16,
}

/// Header flags for a response we fabricate: QR (1), RD (1), RA (1), RCODE=3
/// (NXDOMAIN). 0x8180 is the standard "response, recursion desired + available"
/// base; OR with 3 sets the response code.
pub const NXDOMAIN_FLAGS: u16 = 0x8183;

/// Length of the fixed header, and of the transaction id field.
pub const HEADER_LEN: usize = 12;
const ID_LEN: usize = 2;

/// Read the transaction id (first two bytes) of any DNS message that has one.
pub fn message_id(buf: &[u8]) -> Result<u16, WireError> {
    if buf.len() < ID_LEN {
        return Err(WireError::IdNotSupported);
    }
    Ok(u16::from_be_bytes([buf[0], buf[1]]))
}

/// Parse the fixed header.
pub fn parse_header(buf: &[u8]) -> Result<DnsHeader, WireError> {
    if buf.len() < HEADER_LEN {
        return Err(WireError::TooShort);
    }
    Ok(DnsHeader {
        id: u16::from_be_bytes([buf[0], buf[1]]),
        flags: u16::from_be_bytes([buf[2], buf[3]]),
        qdcount: u16::from_be_bytes([buf[4], buf[5]]),
        ancount: u16::from_be_bytes([buf[6], buf[7]]),
        nscount: u16::from_be_bytes([buf[8], buf[9]]),
        arcount: u16::from_be_bytes([buf[10], buf[11]]),
    })
}

/// Parse the header plus the first question. Returns the question and the offset
/// just past its QNAME (the top of QTYPE/QCLASS) so callers can echo the
/// original question bytes if they want; the echo is rebuilt by hand here.
pub fn parse_question(buf: &[u8]) -> Result<(DnsHeader, DnsQuestion), WireError> {
    let header = parse_header(buf)?;
    if header.qdcount == 0 {
        // Nothing to answer; a name cannot be extracted, so treat as unparsable
        // and let the responder forward the whole message.
        return Err(WireError::BadName);
    }
    // Per RFC 1035 QNAME starts at offset 12 (HEADER_LEN). We do not need the
    // offset past it — the NXDOMAIN reply is rebuilt by hand from the decoded
    // name + qtype + qclass — but skipping QTYPE/QCLASS here validates the
    // message is well-formed enough to answer.
    let (name, qname_end) = parse_qname(buf, HEADER_LEN)?;
    if qname_end + 4 > buf.len() {
        return Err(WireError::TooShort);
    }
    let qtype = u16::from_be_bytes([buf[qname_end], buf[qname_end + 1]]);
    let qclass = u16::from_be_bytes([buf[qname_end + 2], buf[qname_end + 3]]);
    Ok((
        header,
        DnsQuestion {
            name,
            qtype,
            qclass,
        },
    ))
}

/// Parse the QNAME beginning at `offset`. Returns the decoded name and the byte
/// offset just after the terminating zero, so the caller can skip to QTYPE.
///
/// Only the uncompressed encoding is handled. Queries almost never use
/// compression; a pointer label returns [`WireError::BadLabel`], which the
/// responder maps to "forward verbatim".
fn parse_qname(buf: &[u8], offset: usize) -> Result<(String, usize), WireError> {
    let mut labels: Vec<&str> = Vec::new();
    let mut pos = offset;
    let mut len_so_far = 0usize;
    loop {
        if pos >= buf.len() {
            return Err(WireError::TooShort);
        }
        let len = buf[pos] as usize;
        if len == 0 {
            return Ok((join_labels(&labels)?, pos + 1));
        }
        if len & 0xC0 != 0 {
            return Err(WireError::BadLabel);
        }
        if len > 63 {
            return Err(WireError::BadLabel);
        }
        if pos + 1 + len > buf.len() {
            return Err(WireError::TooShort);
        }
        let label = &buf[pos + 1..pos + 1 + len];
        // Labels are arbitrary octets; keep printable ones, otherwise a binary
        // name cannot be meaningfully matched against text rules. Non-ASCII
        // labels scan as "forward", which is the safe answer.
        let s = std::str::from_utf8(label).map_err(|_| WireError::BadLabel)?;
        labels.push(s);
        len_so_far += 1 + len;
        if len_so_far > 255 {
            return Err(WireError::BadName);
        }
        pos += 1 + len;
    }
}

fn join_labels(labels: &[&str]) -> Result<String, WireError> {
    let joined = labels.join(".");
    if joined.is_empty() {
        return Err(WireError::BadName);
    }
    Ok(joined.to_ascii_lowercase())
}

/// Encode a name into its label-sequence form, terminated by a zero byte.
/// Lower-cases and strips a trailing dot. Returns `Err` if any label exceeds 63
/// bytes or the whole name exceeds 255.
pub fn encode_name(name: &str) -> Result<Vec<u8>, WireError> {
    let mut name = name.to_ascii_lowercase();
    while name.ends_with('.') {
        name.pop();
    }
    if name.is_empty() {
        return Err(WireError::BadName);
    }
    let mut out = Vec::with_capacity(name.len() + 2);
    for label in name.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(WireError::BadName);
        }
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
    Ok(out)
}

/// Build an `NXDOMAIN` response echoing the question.
pub fn build_nxdomain(id: u16, question: &DnsQuestion) -> Vec<u8> {
    let qname = encode_name(&question.name).unwrap_or_default();
    let mut out = Vec::with_capacity(HEADER_LEN + qname.len() + 4);
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(&NXDOMAIN_FLAGS.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // qdcount: 1
    out.extend_from_slice(&0u16.to_be_bytes()); // ancount: 0
    out.extend_from_slice(&0u16.to_be_bytes()); // nscount: 0
    out.extend_from_slice(&0u16.to_be_bytes()); // arcount: 0
    out.extend_from_slice(&qname);
    out.extend_from_slice(&question.qtype.to_be_bytes());
    out.extend_from_slice(&question.qclass.to_be_bytes());
    out
}

/// Rewrite the transaction id of an upstream response to match `id`, so the
/// relaying client can never confound it with a reply to a different query.
/// Returns the original bytes unchanged if there is no room for the id.
pub fn set_message_id(buf: &mut [u8], id: u16) {
    if buf.len() >= ID_LEN {
        buf[0] = (id >> 8) as u8;
        buf[1] = (id & 0xff) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal query for `a.example.com` (type A, class IN), built from a
    /// dotted name so the wire bytes are exactly what a real client sends.
    fn query(name: &str, id: u16) -> Vec<u8> {
        let qname = encode_name(name).expect("encode name");
        let mut out = Vec::new();
        out.extend_from_slice(&id.to_be_bytes());
        out.extend_from_slice(&0x0100u16.to_be_bytes()); // flags: RD
        out.extend_from_slice(&1u16.to_be_bytes()); // qdcount
        out.extend_from_slice(&0u16.to_be_bytes()); // ancount
        out.extend_from_slice(&0u16.to_be_bytes()); // nscount
        out.extend_from_slice(&0u16.to_be_bytes()); // arcount
        out.extend_from_slice(&qname);
        out.extend_from_slice(&1u16.to_be_bytes()); // qtype A
        out.extend_from_slice(&1u16.to_be_bytes()); // qclass IN
        out
    }

    #[test]
    fn message_id_is_the_first_two_bytes() {
        let q = query("a.example.com", 0x1234);
        assert_eq!(message_id(&q).expect("id"), 0x1234);
    }

    #[test]
    fn parse_header_decodes_all_sections() {
        let q = query("example.com", 0x0102);
        let h = parse_header(&q).expect("header");
        assert_eq!(h.id, 0x0102);
        assert_eq!(h.qdcount, 1);
        assert_eq!(h.ancount, 0);
    }

    #[test]
    fn parse_question_extracts_name_type_and_class() {
        let q = query("a.example.com", 0x0005);
        let (_h, question) = parse_question(&q).expect("question");
        assert_eq!(question.name, "a.example.com");
        assert_eq!(question.qtype, 1);
        assert_eq!(question.qclass, 1);
    }

    #[test]
    fn parse_question_lowercases_the_name() {
        let q = query("A.EXAMPLE.COM", 0x0005);
        let (_h, question) = parse_question(&q).expect("question");
        assert_eq!(question.name, "a.example.com");
    }

    #[test]
    fn parse_rejects_a_truncated_message() {
        let q = query("a.example.com", 0x0005);
        assert_eq!(parse_question(&q[..10]), Err(WireError::TooShort));
    }

    #[test]
    fn parse_rejects_missing_question() {
        let mut q = query("example.com", 0x0005);
        q[4] = 0;
        q[5] = 0; // qdcount = 0 (bytes 4-5 of the header)
        assert!(matches!(parse_question(&q), Err(WireError::BadName)));
    }

    #[test]
    fn nxdomain_echoes_the_question_with_rcode_three() {
        let q = query("a.example.com", 0x00ff);
        let (_h, question) = parse_question(&q).expect("question");
        let resp = build_nxdomain(0x00ff, &question);

        let rh = parse_header(&resp).expect("resp header");
        assert_eq!(rh.id, 0x00ff);
        assert_eq!(rh.flags, NXDOMAIN_FLAGS, "QR + RCODE=3 (NXDOMAIN)");
        assert_eq!(rh.qdcount, 1);
        assert_eq!(rh.ancount, 0);

        let (rq, rquestion) = parse_question(&resp).expect("resp question");
        assert_eq!(rq.id, 0x00ff);
        assert_eq!(rquestion.name, "a.example.com");
        assert_eq!(rquestion.qtype, 1);
    }

    #[test]
    fn encode_name_round_trips_and_strips_trailing_dot() {
        let enc = encode_name("A.example.com.").expect("encode");
        assert_eq!(&enc[..], b"\x01a\x07example\x03com\x00");
    }

    #[test]
    fn encode_name_rejects_overlong_labels() {
        let long = "a".repeat(64);
        assert_eq!(encode_name(&long), Err(WireError::BadName));
    }

    #[test]
    fn set_message_id_patches_the_response() {
        let q = query("example.com", 0x1111);
        // The upstream might reply with the same id; force a rewrite anyway.
        let mut resp = q.clone();
        set_message_id(&mut resp, 0xaaaa);
        assert_eq!(message_id(&resp).expect("id"), 0xaaaa);
    }

    #[test]
    fn set_message_id_is_a_noop_on_a_tiny_buffer() {
        let mut short = vec![0u8; 1];
        set_message_id(&mut short, 0xabcd);
        assert_eq!(short, vec![0u8; 1]);
    }
}
