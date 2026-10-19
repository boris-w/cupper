//! [`AcpTransport`]: the byte pipes between the host and one adapter instance
//! (design 5.2.16). Production reads and writes the adapter's stdio pipes
//! ([`StdioTransport`]); tests and in-process replay use an in-memory duplex
//! (`MemoryTransport`). Either way the lines pass through the wire tap.

use std::io;
use std::os::fd::OwnedFd;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::unix::pipe;

/// A bidirectional byte stream to an adapter instance.
pub trait AcpTransport: Send + 'static {
    /// What the adapter writes: its stdout.
    type Reader: AsyncRead + Send + Unpin + 'static;
    /// What the adapter reads: its stdin. Shutting it down, or dropping it,
    /// closes the adapter's stdin.
    type Writer: AsyncWrite + Send + Unpin + 'static;

    fn into_split(self) -> (Self::Reader, Self::Writer);
}

/// The stdin and stdout of an adapter that `cupper_sys::spawn` started with
/// both set to a pipe.
#[derive(Debug)]
pub struct StdioTransport {
    stdin: pipe::Sender,
    stdout: pipe::Receiver,
}

impl StdioTransport {
    /// Takes our ends of the child's stdin and stdout pipes (`Child::stdin`
    /// and `Child::stdout`) and puts them in non-blocking mode. Must run
    /// inside the tokio runtime.
    pub fn new(stdin: OwnedFd, stdout: OwnedFd) -> io::Result<Self> {
        Ok(Self {
            stdin: pipe::Sender::from_owned_fd(stdin)?,
            stdout: pipe::Receiver::from_owned_fd(stdout)?,
        })
    }
}

impl AcpTransport for StdioTransport {
    type Reader = pipe::Receiver;
    type Writer = pipe::Sender;

    fn into_split(self) -> (Self::Reader, Self::Writer) {
        (self.stdout, self.stdin)
    }
}

#[cfg(any(test, feature = "test-harness"))]
pub use memory::MemoryTransport;

#[cfg(any(test, feature = "test-harness"))]
mod memory {
    use tokio::io::{DuplexStream, ReadHalf, WriteHalf};

    use super::AcpTransport;

    /// The host's end of an in-memory duplex; the other end plays the agent.
    #[derive(Debug)]
    pub struct MemoryTransport(DuplexStream);

    impl MemoryTransport {
        /// A transport and the agent's end of it. Each direction buffers up
        /// to `capacity` bytes before the writer waits for the reader.
        pub fn pair(capacity: usize) -> (Self, DuplexStream) {
            let (host, agent) = tokio::io::duplex(capacity);
            (Self(host), agent)
        }
    }

    impl AcpTransport for MemoryTransport {
        type Reader = ReadHalf<DuplexStream>;
        type Writer = WriteHalf<DuplexStream>;

        fn into_split(self) -> (Self::Reader, Self::Writer) {
            tokio::io::split(self.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn unit_transport_memory_carries_bytes_both_ways() {
        let (transport, mut agent) = MemoryTransport::pair(64);
        let (mut from_agent, mut to_agent) = transport.into_split();

        to_agent.write_all(b"to the agent\n").await.unwrap();
        let mut buf = [0; 13];
        agent.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"to the agent\n");

        agent.write_all(b"to the host\n").await.unwrap();
        let mut buf = [0; 12];
        from_agent.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"to the host\n");
    }

    #[tokio::test]
    async fn unit_transport_memory_close_reaches_the_other_end() {
        let (transport, mut agent) = MemoryTransport::pair(64);
        let (mut from_agent, mut to_agent) = transport.into_split();

        // The host closing the agent's stdin: the agent reads what was sent, then EOF.
        to_agent.write_all(b"last").await.unwrap();
        to_agent.shutdown().await.unwrap();
        let mut got = Vec::new();
        agent.read_to_end(&mut got).await.unwrap();
        assert_eq!(got, b"last");

        // The agent going away: the host reads EOF, and writing fails.
        drop(agent);
        assert_eq!(from_agent.read(&mut [0; 8]).await.unwrap(), 0);
        let err = to_agent.write_all(b"late").await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }

    #[tokio::test]
    async fn stdio_transport_reads_and_writes_pipes() {
        let (child_stdin, our_stdin) = io::pipe().unwrap();
        let (our_stdout, child_stdout) = io::pipe().unwrap();
        let transport = StdioTransport::new(our_stdin.into(), our_stdout.into()).unwrap();
        let (mut from_child, mut to_child) = transport.into_split();

        to_child.write_all(b"ping").await.unwrap();
        drop(to_child);
        let mut got = String::new();
        std::io::Read::read_to_string(&mut &child_stdin, &mut got).unwrap();
        assert_eq!(got, "ping");

        std::io::Write::write_all(&mut &child_stdout, b"pong").unwrap();
        drop(child_stdout);
        let mut got = String::new();
        from_child.read_to_string(&mut got).await.unwrap();
        assert_eq!(got, "pong");
    }
}
