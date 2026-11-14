//! UniFFI bindings exposing kstream-rtmp's [`Publisher`] to Kotlin (and Swift).

use std::sync::{Arc, Mutex};

use kstream_rtmp::publisher::{Publisher, PublisherConfig, PublisherError};

uniffi::setup_scaffolding!();

#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum RtmpError {
    Io { message: String },
    Handshake { message: String },
    Session { message: String },
    Protocol { message: String },
    NotPublishing,
    PeerClosed,
}

impl std::fmt::Display for RtmpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RtmpError::Io { message } => write!(f, "io: {message}"),
            RtmpError::Handshake { message } => write!(f, "handshake: {message}"),
            RtmpError::Session { message } => write!(f, "session: {message}"),
            RtmpError::Protocol { message } => write!(f, "protocol: {message}"),
            RtmpError::NotPublishing => write!(f, "not publishing"),
            RtmpError::PeerClosed => write!(f, "peer closed"),
        }
    }
}

impl From<PublisherError> for RtmpError {
    fn from(e: PublisherError) -> Self {
        match e {
            PublisherError::Io(x) => RtmpError::Io { message: x.to_string() },
            PublisherError::Handshake(x) => RtmpError::Handshake { message: x },
            PublisherError::Session(x) => RtmpError::Session { message: x },
            PublisherError::Protocol(x) => RtmpError::Protocol { message: x },
            PublisherError::NotPublishing => RtmpError::NotPublishing,
            PublisherError::PeerClosed => RtmpError::PeerClosed,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct RtmpPublisherConfig {
    pub app: String,
    pub stream_key: String,
    pub address: String,
    pub timeout_ms: u64,
}

impl Default for RtmpPublisherConfig {
    fn default() -> Self {
        Self {
            app: "live".into(),
            stream_key: "stream".into(),
            address: "127.0.0.1:1935".into(),
            timeout_ms: 10_000,
        }
    }
}

#[derive(uniffi::Object)]
pub struct RtmpPublisher {
    inner: Mutex<Option<Publisher>>,
}

#[uniffi::export]
impl RtmpPublisher {
    #[uniffi::constructor]
    pub fn new(config: RtmpPublisherConfig) -> Result<Arc<Self>, RtmpError> {
        let mut pcfg = PublisherConfig::default();
        pcfg.app = config.app;
        pcfg.stream_key = config.stream_key;
        pcfg.timeout = std::time::Duration::from_millis(config.timeout_ms.max(1000));
        let publisher = Publisher::connect(&config.address, pcfg)?;
        Ok(Arc::new(Self {
            inner: Mutex::new(Some(publisher)),
        }))
    }

    pub fn send_video(&self, pts_ms: u32, is_keyframe: bool, data: Vec<u8>) -> Result<(), RtmpError> {
        let mut guard = self.inner.lock().unwrap();
        let p = guard.as_mut().ok_or(RtmpError::NotPublishing)?;
        p.send_video(pts_ms, is_keyframe, &data).map_err(Into::into)
    }

    pub fn send_audio(&self, pts_ms: u32, data: Vec<u8>) -> Result<(), RtmpError> {
        let mut guard = self.inner.lock().unwrap();
        let p = guard.as_mut().ok_or(RtmpError::NotPublishing)?;
        p.send_audio(pts_ms, &data).map_err(Into::into)
    }

    pub fn finish(&self) -> Result<(), RtmpError> {
        let mut guard = self.inner.lock().unwrap();
        if let Some(p) = guard.take() {
            // Publisher::finish takes &mut self; we have it via take.
            let mut p = p;
            p.finish().map_err(Into::into)
        } else {
            Ok(())
        }
    }
}