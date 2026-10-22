//! Why a request to another machine's mods did not get its answer: a kind the players' buses
//! decide by (ask again soon, give up, fall back to an older host), and the same words as
//! ever for the log and the player.
//!
//! The connection code the map download shares (`open_waiting`, `read_reply`) says what went
//! wrong in words. They are read here, once (`From<String>`: an anti-corruption layer between
//! that code and the players' buses): `connect` and `reply` hand our code the kind, and
//! nothing else looks into an error's text.

use std::fmt;
use std::io::BufRead;
use std::net::SocketAddr;
use std::time::Duration;

/// The words a host answers `ERR` with, which both sides write and read.
pub(crate) mod says {
    /// A bus not kept on the host (it may be on its way).
    pub(crate) const NO_SUCH_BUS: &str = "no such bus";
    /// A file the host does not keep (or one named wrongly).
    pub(crate) const NO_SUCH_FILE: &str = "no such file";
    /// Too many connections or too much held at once: come back.
    pub(crate) const BUSY: &str = "busy";
    /// A request the host does not know: a host of a version before it.
    pub(crate) const WHAT: &str = "what";
    /// A host that takes no buses from its players.
    pub(crate) const TAKES_NO_UPLOADS: &str = "this server takes no uploads";
}

/// What a request to the host came to, when it did not get its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Failure {
    /// No way to the host is known yet (its mods were not fetched).
    NoHost,
    /// The host could not be reached (the whole line, with the address and the system's words).
    Unreachable(String),
    /// The connection broke off or was closed (what happened).
    ConnectionLost(String),
    /// The host answered `ERR` with these words (see `says`).
    HostSays(String),
    /// Anything else: a bad answer, a check that failed here.
    Other(String),
}

const HOST_SAYS: &str = "the host says: ";
const NO_HOST: &str = "no host to ask yet";
const UNREACHABLE: &str = "cannot reach ";

impl Failure {
    /// Did the host answer with these words?
    pub(crate) fn host_says(&self, words: &str) -> bool {
        matches!(self, Failure::HostSays(w) if w == words)
    }

    /// A host of a version before the request (it answered `ERR what`).
    pub(crate) fn older_host(&self) -> bool {
        self.host_says(says::WHAT)
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::NoHost => f.write_str(NO_HOST),
            Failure::Unreachable(line) => f.write_str(line),
            Failure::ConnectionLost(what) => write!(f, "{}: {what}", super::CONNECTION_LOST),
            Failure::HostSays(words) => write!(f, "{HOST_SAYS}{words}"),
            Failure::Other(s) => f.write_str(s),
        }
    }
}

/// The words of the shared connection code, read as a kind (the one place that does).
impl From<String> for Failure {
    fn from(s: String) -> Failure {
        let lost = format!("{}: ", super::CONNECTION_LOST);
        if let Some(what) = s.strip_prefix(&lost) {
            Failure::ConnectionLost(what.to_string())
        } else if let Some(words) = s.strip_prefix(HOST_SAYS) {
            Failure::HostSays(words.to_string())
        } else if s.starts_with(UNREACHABLE) {
            Failure::Unreachable(s)
        } else if s == NO_HOST || s == "no host to ask" {
            Failure::NoHost
        } else {
            Failure::Other(s)
        }
    }
}

impl From<&str> for Failure {
    fn from(s: &str) -> Failure {
        Failure::from(s.to_string())
    }
}

/// A connection to the host's mods, greeted (`open_waiting`), its failure as a kind.
pub(super) fn connect(host: SocketAddr, session: u64, wait: Duration) -> Result<super::Connection, Failure> {
    super::open_waiting(host, session, wait).map_err(Failure::from)
}

/// The host's answer to a request (`read_reply`: `OK [n]`), its failure as a kind.
pub(super) fn reply(input: &mut impl BufRead) -> Result<Option<u64>, Failure> {
    super::read_reply(input).map_err(Failure::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_words_of_the_connection_code_become_kinds_and_read_the_same() {
        let lost = format!("{}: closed by the host", super::super::CONNECTION_LOST);
        let cases = [
            (lost.as_str(), Failure::ConnectionLost("closed by the host".into())),
            ("the host says: no such bus", Failure::HostSays(says::NO_SUCH_BUS.into())),
            ("the host says: busy", Failure::HostSays(says::BUSY.into())),
            ("cannot reach the host's mods on TCP 127.0.0.1:1: refused", Failure::Unreachable("cannot reach the host's mods on TCP 127.0.0.1:1: refused".into())),
            ("no host to ask yet", Failure::NoHost),
            ("bad list: expected value", Failure::Other("bad list: expected value".into())),
        ];
        for (words, kind) in cases {
            let f = Failure::from(words);
            assert_eq!(f, kind, "{words}");
            // (the log and the player read what they read before)
            assert_eq!(f.to_string(), words);
        }
    }
}
