//! A client-side connection to a TFTP server. Implementors can use this
//! to build a more fully-featured client application.

use std::io::{self, Read, Result, Write};
use std::iter::Iterator;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

use rand::Rng;

use crate::bytes::{FromBytes, IntoBytes};
use crate::connection::Connection;
use crate::connection::MIN_PORT_NUMBER;
use crate::packet::*;
use crate::RetransmissionConfig;

/// The initial state for building a `Client`.
pub struct New {
    socket: UdpSocket,
    retransmission_config: Option<RetransmissionConfig>,
}

/// An intermediate state for building a `Client`.Builder
///
/// At this point, the `Builder` has all the information
/// it needs to construct a client.
pub struct ConnectTo {
    server: Vec<SocketAddr>,
    socket: UdpSocket,
    retransmission_config: Option<RetransmissionConfig>,
}

/// Builds a `Client`.
pub struct Builder<T> {
    data: T,
}

/// Represents a single connection with a TFTP server.
pub struct Client {
    server: Vec<SocketAddr>,
    socket: UdpSocket,
    retransmission_config: Option<RetransmissionConfig>,
}

impl Builder<New> {
    /// Generates a Transfer ID (a bind address & port) and opens a `UdpSocket`
    /// for this connection.
    pub fn new(retransmission_config: Option<RetransmissionConfig>) -> Result<Self> {
        let mut rng = rand::thread_rng();
        let port: u16 = rng.gen_range(MIN_PORT_NUMBER, u16::MAX);
        let bind_to = format!("0.0.0.0:{}", port);
        let socket = UdpSocket::bind(bind_to)?;
        socket.set_read_timeout(retransmission_config.map(|conf| conf.timeout))?;

        let data = New {
            socket,
            retransmission_config,
        };

        Ok(Builder { data })
    }

    /// Stores the Transfer ID (address + port) of the server to connect to.
    pub fn connect_to<A: ToSocketAddrs>(self, server: A) -> Result<Builder<ConnectTo>> {
        let resolved = server.to_socket_addrs()?.collect();
        let data = ConnectTo {
            server: resolved,
            socket: self.data.socket,
            retransmission_config: self.data.retransmission_config,
        };

        Ok(Builder { data })
    }
}

impl Builder<ConnectTo> {
    /// Constructs the client.
    pub fn build(self) -> Client {
        Client {
            server: self.data.server,
            socket: self.data.socket,
            retransmission_config: self.data.retransmission_config,
        }
    }

    /// Creates an instance with a different socket from the origninal instance.
    pub fn try_clone(&self) -> Result<Self> {
        let new_sock_builder = Builder::new(self.data.retransmission_config)?;
        let data = ConnectTo {
            server: self.data.server.clone(),
            socket: new_sock_builder.data.socket,
            retransmission_config: self.data.retransmission_config,
        };
        Ok(Builder { data })
    }
}

impl Client {
    /// Retrieves a file from the remote server.
    pub fn get<S: AsRef<str>, W: Write>(self, file: S, mode: Mode, writer: W) -> Result<W> {
        let rrq = Packet::rrq(file, mode);
        let _ = self
            .socket
            .send_to(&rrq.into_bytes()[..], &self.server[..])?;

        let mut buf = [0; MAX_PACKET_SIZE];
        let (_, server) = self.socket.peek_from(&mut buf)?;
        self.socket.connect(server)?;

        let conn = Connection::new(
            self.socket,
            self.retransmission_config
                .and_then(|conf| conf.max_retransmissions),
        );
        conn.get(writer)
    }

    /// Stores a file on the remote server.
    pub fn put<S: AsRef<str>, R: Read>(self, file: S, mode: Mode, reader: R) -> Result<()> {
        let wrq = Packet::wrq(file, mode);
        let _ = self
            .socket
            .send_to(&wrq.into_bytes()[..], &self.server[..])?;

        let mut buf = [0; MAX_PACKET_SIZE];
        let (nbytes, server) = self.socket.recv_from(&mut buf)?;
        self.socket.connect(server)?;

        let _ = match Packet::<Ack>::from_bytes(&buf[..nbytes]) {
            Ok(a) => a,
            Err(e) => {
                let error: Packet<Error> = e.into();
                return Err(io::Error::from(error));
            }
        };

        let conn = Connection::new(
            self.socket,
            self.retransmission_config
                .and_then(|conf| conf.max_retransmissions),
        );
        conn.put(reader)
    }
}
