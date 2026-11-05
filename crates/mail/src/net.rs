//! Connections: plain TCP, implicit TLS, or STARTTLS upgrade.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::error::{MailError, MailResult};

/// How a connection is protected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsMode {
    /// TLS from the first byte (IMAPS 993, SMTPS 465).
    Implicit,
    /// Plain greeting, then `STARTTLS` (587, 143).
    StartTls,
    /// No TLS. Only for a local relay or a test server.
    None,
}

impl TlsMode {
    pub fn parse(value: &str) -> MailResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "implicit" | "tls" | "ssl" => Ok(TlsMode::Implicit),
            "starttls" => Ok(TlsMode::StartTls),
            "none" | "plain" | "" => Ok(TlsMode::None),
            other => Err(MailError::config(format!(
                "unknown TLS mode {other:?}; use implicit, starttls or none"
            ))),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TlsMode::Implicit => "implicit",
            TlsMode::StartTls => "starttls",
            TlsMode::None => "none",
        }
    }

    /// Whether a plain connection should begin with TLS.
    pub fn is_implicit(self) -> bool {
        self == TlsMode::Implicit
    }

    /// Whether the connection should be upgraded in-band.
    pub fn is_starttls(self) -> bool {
        self == TlsMode::StartTls
    }
}

/// A connection that may or may not be wrapped in TLS.
pub enum Conn {
    Plain(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Conn {
    /// Open a connection, using TLS immediately when `mode` says so.
    pub fn connect(host: &str, port: u16, mode: TlsMode) -> MailResult<Self> {
        if host.trim().is_empty() {
            return Err(MailError::config("no mail server host is configured"));
        }
        let stream = TcpStream::connect((host, port)).map_err(|error| {
            MailError::transient(format!("cannot reach {host}:{port}: {error}"))
        })?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))?;
        let _ = stream.set_nodelay(true);
        if mode.is_implicit() {
            return Self::tls(stream, host);
        }
        Ok(Conn::Plain(stream))
    }

    fn tls(stream: TcpStream, host: &str) -> MailResult<Self> {
        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = ServerName::try_from(host.to_string())
            .map_err(|_| MailError::config(format!("{host:?} is not a valid TLS server name")))?;
        let connection = ClientConnection::new(Arc::new(config), name)
            .map_err(|error| MailError::transient(format!("TLS setup failed: {error}")))?;
        Ok(Conn::Tls(Box::new(StreamOwned::new(connection, stream))))
    }

    /// Upgrade a plain connection after a successful STARTTLS command.
    pub fn upgrade(self, host: &str) -> MailResult<Self> {
        match self {
            Conn::Plain(stream) => Self::tls(stream, host),
            other => Ok(other),
        }
    }

    pub fn is_tls(&self) -> bool {
        matches!(self, Conn::Tls(_))
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(stream) => stream.read(buf),
            Conn::Tls(stream) => stream.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(stream) => stream.write(buf),
            Conn::Tls(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Conn::Plain(stream) => stream.flush(),
            Conn::Tls(stream) => stream.flush(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tls_modes() {
        assert_eq!(TlsMode::parse("implicit").unwrap(), TlsMode::Implicit);
        assert_eq!(TlsMode::parse("StartTLS").unwrap(), TlsMode::StartTls);
        assert_eq!(TlsMode::parse("none").unwrap(), TlsMode::None);
        assert!(TlsMode::parse("maybe").is_err());
    }

    #[test]
    fn refuses_an_empty_host() {
        assert!(Conn::connect("", 993, TlsMode::Implicit).is_err());
    }
}
