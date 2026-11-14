//! High-level, synchronous RTMP publisher for kstream.
//!
//! This wraps the sans-I/O [`crate::sessions::ClientSession`] plus a TCP
//! transport into a blocking API so a caller (typically the kstream engine
//! running on a worker thread) can publish with minimal boilerplate:
//!
//! ```no_run
//! use kstream_rtmp::publisher::{Publisher, PublisherConfig};
//! use std::time::Duration;
//!
//! let mut publisher = Publisher::connect(
//!     "192.168.1.10:1935",
//!     PublisherConfig {
//!         app: "live".into(),
//!         stream_key: "demo".into(),
//!         ..Default::default()
//!     },
//! )?;
//! publisher.send_video(0, true, &[0x17, 0x00])?; // FLV video tag body
//! publisher.finish()?;
//! # Ok::<(), kstream_rtmp::publisher::PublisherError>(())
//! ```
//!
//! The media payload passed to [`Publisher::send_video`] /
//! [`Publisher::send_audio`] is the **RTMP message body** (i.e. the FLV tag
//! body: frame-type/codec byte, composition time for AVC/HEVC, then codec
//! bytes). RTMPX deliberately does not package codecs; the caller owns that.
//! Timestamps are in milliseconds.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use thiserror::Error;

use crate::handshake::{Handshake, HandshakeProgress, HandshakeRole};
use crate::sessions::{
    ClientEvent, ClientOutput, ClientSession, ClientSessionConfig, PublishMode, StreamHandle,
    StreamMetadata,
};
use crate::time::RtmpTimestamp;
use crate::{DropPolicy, Packet, Segments};

/// Configuration for a publish session.
#[derive(Clone, Debug)]
pub struct PublisherConfig {
    /// Application name (e.g. `"live"`).
    pub app: String,
    /// Stream key.
    pub stream_key: String,
    /// Socket read/write timeout.
    pub timeout: Duration,
    /// Optional stream metadata sent after publish is accepted.
    pub metadata: Option<StreamMetadata>,
    /// Drop policy for media packets when the transport is congested.
    pub drop_policy: DropPolicy,
}

impl Default for PublisherConfig {
    fn default() -> Self {
        Self {
            app: "live".into(),
            stream_key: "stream".into(),
            timeout: Duration::from_secs(10),
            metadata: None,
            drop_policy: DropPolicy::Never,
        }
    }
}

#[derive(Debug, Error)]
pub enum PublisherError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("handshake: {0}")]
    Handshake(String),
    #[error("rtmp session: {0}")]
    Session(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("not publishing yet")]
    NotPublishing,
    #[error("peer closed the connection")]
    PeerClosed,
}

impl From<crate::sessions::ClientSessionError> for PublisherError {
    fn from(e: crate::sessions::ClientSessionError) -> Self {
        PublisherError::Session(e.to_string())
    }
}

/// Blocking RTMP publisher.
pub struct Publisher {
    stream: TcpStream,
    session: ClientSession,
    input: Bytes,
    buffer: BytesMut,
    handle: Option<StreamHandle>,
    publishing: bool,
    metadata: Option<StreamMetadata>,
    drop_policy: DropPolicy,
    read_chunk: usize,
}

impl Publisher {
    /// Connect, handshake, connect the application, and start publishing.
    pub fn connect<A: std::net::ToSocketAddrs>(
        address: A,
        config: PublisherConfig,
    ) -> Result<Self, PublisherError> {
        let mut stream = TcpStream::connect(address)?;
        stream.set_read_timeout(Some(config.timeout))?;
        stream.set_write_timeout(Some(config.timeout))?;

        // Handshake.
        let mut handshake = Handshake::new(HandshakeRole::Client);
        stream.write_all(&handshake.generate_outbound_p0_and_p1().map_err(|e| {
            PublisherError::Handshake(e.to_string())
        })?)?;
        stream.flush()?;
        let mut hs_buf = [0u8; 4096];
        #[allow(unused_assignments)]
        let mut remaining_bytes_out: Option<Bytes> = None;
        loop {
            let n = stream.read(&mut hs_buf)?;
            if n == 0 {
                return Err(PublisherError::PeerClosed);
            }
            match handshake
                .process_bytes(&hs_buf[..n])
                .map_err(|e| PublisherError::Handshake(e.to_string()))?
            {
                HandshakeProgress::InProgress { response_bytes } => {
                    stream.write_all(&response_bytes)?;
                    stream.flush()?;
                }
                HandshakeProgress::Completed {
                    response_bytes,
                    remaining_bytes,
                } => {
                    stream.write_all(&response_bytes)?;
                    stream.flush()?;
                    remaining_bytes_out = Some(Bytes::from(remaining_bytes));
                    break;
                }
            }
        }
        let remaining = remaining_bytes_out.unwrap_or_default();

        let mut session = ClientSession::new(ClientSessionConfig::default())?;
        session.connect(&config.app)?;

        let metadata = config.metadata.clone().or_else(|| {
            Some(StreamMetadata {
                encoder: Some("kstream".into()),
                ..Default::default()
            })
        });

        let mut publisher = Publisher {
            stream,
            session,
            input: remaining,
            buffer: BytesMut::with_capacity(16 * 1024),
            handle: None,
            publishing: false,
            metadata,
            drop_policy: config.drop_policy,
            read_chunk: 16 * 1024,
        };

        // Drive the control plane until publish is accepted.
        publisher.drive_until_publish(&config.stream_key)?;
        Ok(publisher)
    }

    fn read_more(&mut self) -> Result<bool, PublisherError> {
        self.buffer.reserve(self.read_chunk);
        let mut tmp = vec![0u8; self.read_chunk];
        match self.stream.read(&mut tmp) {
            Ok(0) => Ok(false),
            Ok(n) => {
                self.buffer.extend_from_slice(&tmp[..n]);
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut
                || e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                // No more data right now; that's fine.
                Ok(true)
            }
            Err(e) => Err(PublisherError::Io(e)),
        }
    }

    fn write_packet<P: Segments>(&mut self, mut packet: Packet<P>) -> Result<(), PublisherError> {
        while !packet.is_complete() {
            let mut slices = [std::io::IoSlice::new(&[]); 32];
            let count = packet.io_slices(&mut slices);
            let n = self.stream.write_vectored(&slices[..count])?;
            if n == 0 {
                return Err(PublisherError::PeerClosed);
            }
            packet.advance(n);
        }
        Ok(())
    }

    fn drain_outputs(&mut self) -> Result<(), PublisherError> {
        loop {
            match self.session.receive(&mut self.input) {
                Ok(Some(ClientOutput::Packet(packet))) => {
                    self.write_packet(packet)?;
                }
                Ok(Some(ClientOutput::Event(ClientEvent::ConnectionRequestAccepted { .. }))) => {
                    // handled by caller state
                }
                Ok(Some(ClientOutput::Event(ClientEvent::PublishRequestAccepted { .. }))) => {
                    self.publishing = true;
                }
                Ok(Some(ClientOutput::Event(ClientEvent::ConnectionRequestRejected {
                    description,
                    ..
                }))) => {
                    return Err(PublisherError::Protocol(description));
                }
                Ok(Some(ClientOutput::Event(ClientEvent::PublishRequestRejected {
                    status,
                    ..
                }))) => {
                    return Err(PublisherError::Protocol(
                        status.description().unwrap_or("publish rejected").to_string(),
                    ));
                }
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(e) => return Err(PublisherError::from(e)),
            }
        }
        // If input exhausted, pull more from socket.
        if self.input.is_empty() {
            if !self.read_more()? {
                // peer closed; keep going, caller will notice
            }
            // Move freshly read bytes into input.
            if !self.buffer.is_empty() {
                self.input = self.buffer.split().freeze();
            }
        }
        Ok(())
    }

    fn drive_until_publish(&mut self, stream_key: &str) -> Result<(), PublisherError> {
        // connect already queued a packet; publish after connection accepted.
        let mut published = false;
        let mut publish_handle: Option<StreamHandle> = None;
        // First, ensure we send the connect output.
        self.drain_outputs()?;
        // Loop until publish accepted.
        let deadline_iterations = 10_000usize;
        for _ in 0..deadline_iterations {
            if self.publishing {
                break;
            }
            // If we haven't published yet and connection accepted, publish now.
            // Detect connection accepted by attempting publish when handle is None.
            if publish_handle.is_none() {
                match self.session.publish(stream_key, PublishMode::Live) {
                    Ok(h) => {
                        publish_handle = Some(h);
                        self.handle = publish_handle;
                    }
                    Err(_) => {
                        // connection not accepted yet
                    }
                }
            }
            // Send metadata once publishing accepted.
            self.drain_outputs()?;
            if self.publishing && !published {
                published = true;
                #[allow(clippy::collapsible_if)]
                if let Some(h) = self.handle {
                    if let Some(meta) = self.metadata.take() {
                        let pkt = self.session.send_metadata(h, &meta)?;
                        self.write_packet(pkt)?;
                    }
                }
            }
            if self.publishing {
                break;
            }
            // Need more input.
            if self.input.is_empty() {
                if !self.read_more()? {
                    return Err(PublisherError::PeerClosed);
                }
                if !self.buffer.is_empty() {
                    self.input = self.buffer.split().freeze();
                }
            }
        }
        if !self.publishing {
            return Err(PublisherError::Protocol("publish not accepted".into()));
        }
        Ok(())
    }

    /// Send one video message. `data` is the RTMP video message body.
    pub fn send_video(
        &mut self,
        pts_ms: u32,
        _is_keyframe: bool,
        data: &[u8],
    ) -> Result<(), PublisherError> {
        if !self.publishing {
            return Err(PublisherError::NotPublishing);
        }
        let h = self.handle.ok_or(PublisherError::NotPublishing)?;
        let pkt = self.session.send_video(
            h,
            Bytes::copy_from_slice(data),
            RtmpTimestamp::new(pts_ms),
            self.drop_policy,
        )?;
        self.write_packet(pkt)?;
        // Drain any control outputs (acks/pings) opportunistically.
        let _ = self.drain_outputs();
        Ok(())
    }

    /// Send one audio message. `data` is the RTMP audio message body.
    pub fn send_audio(&mut self, pts_ms: u32, data: &[u8]) -> Result<(), PublisherError> {
        if !self.publishing {
            return Err(PublisherError::NotPublishing);
        }
        let h = self.handle.ok_or(PublisherError::NotPublishing)?;
        let pkt = self.session.send_audio(
            h,
            Bytes::copy_from_slice(data),
            RtmpTimestamp::new(pts_ms),
            self.drop_policy,
        )?;
        self.write_packet(pkt)?;
        let _ = self.drain_outputs();
        Ok(())
    }

    /// Finish publishing: delete the stream and drain.
    pub fn finish(&mut self) -> Result<(), PublisherError> {
        if let Some(h) = self.handle.take() {
            let _ = self.session.delete_stream(h);
        }
        // Drain remaining outputs.
        for _ in 0..1000 {
            match self.session.receive(&mut self.input) {
                Ok(Some(ClientOutput::Packet(packet))) => {
                    self.write_packet(packet)?;
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    if self.input.is_empty() {
                        if !self.read_more()? {
                            break;
                        }
                        if !self.buffer.is_empty() {
                            self.input = self.buffer.split().freeze();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
        Ok(())
    }
}
