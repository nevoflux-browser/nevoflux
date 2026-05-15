use nevoflux_protocol::{Channel, DaemonEnvelope, ProxyEnvelope};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, Mutex};
use tokio::time::timeout;

const MAX_FRAME_SIZE: u32 = 32 * 1024 * 1024;

pub struct PocClient {
    proxy_id: String,
    writer: Arc<Mutex<BufWriter<OwnedWriteHalf>>>,
    inbox: mpsc::Receiver<DaemonEnvelope>,
}

impl PocClient {
    pub async fn connect(port: u16, proxy_id: impl Into<String>) -> io::Result<Self> {
        let proxy_id = proxy_id.into();
        let stream = TcpStream::connect(("127.0.0.1", port)).await?;
        let (rh, wh) = stream.into_split();
        let writer = Arc::new(Mutex::new(BufWriter::new(wh)));

        // Registration frame must be a raw JSON object {type, proxy_id},
        // NOT a ProxyEnvelope — matches handle_proxy_connection in
        // daemon/src/server.rs.
        let register = serde_json::json!({
            "type": "register",
            "proxy_id": proxy_id,
        });
        write_frame(&writer, &register).await?;

        let (tx, rx) = mpsc::channel::<DaemonEnvelope>(256);
        tokio::spawn(read_loop(rh, tx));

        Ok(Self {
            proxy_id,
            writer,
            inbox: rx,
        })
    }

    /// Wrap `payload` in a ProxyEnvelope on the Chat channel, send it,
    /// and return the generated `request_id`.
    pub async fn send_chat(&self, payload: serde_json::Value) -> io::Result<String> {
        let request_id = format!("poc-{}", uuid::Uuid::new_v4().simple());
        let env = ProxyEnvelope::new(&self.proxy_id, &request_id, Channel::Chat, payload);
        let v = serde_json::to_value(&env)?;
        write_frame(&self.writer, &v).await?;
        Ok(request_id)
    }

    pub async fn recv(&mut self) -> Option<DaemonEnvelope> {
        self.inbox.recv().await
    }

    /// Drain inbox until an envelope matches `predicate` (or until
    /// `within` elapses). Unmatched envelopes are discarded.
    pub async fn recv_matching<F>(
        &mut self,
        within: Duration,
        mut predicate: F,
    ) -> Option<DaemonEnvelope>
    where
        F: FnMut(&DaemonEnvelope) -> bool,
    {
        let deadline = tokio::time::Instant::now() + within;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            match timeout(remaining, self.inbox.recv()).await {
                Ok(Some(env)) if predicate(&env) => return Some(env),
                Ok(Some(_)) => continue,
                Ok(None) => return None,
                Err(_) => return None,
            }
        }
    }
}

async fn write_frame(
    writer: &Arc<Mutex<BufWriter<OwnedWriteHalf>>>,
    value: &serde_json::Value,
) -> io::Result<()> {
    let data =
        serde_json::to_vec(value).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let len = u32::try_from(data.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame too large"))?;
    let mut w = writer.lock().await;
    w.write_all(&len.to_le_bytes()).await?;
    w.write_all(&data).await?;
    w.flush().await?;
    Ok(())
}

async fn read_loop(read_half: OwnedReadHalf, tx: mpsc::Sender<DaemonEnvelope>) {
    let mut reader = BufReader::new(read_half);
    loop {
        let mut len_buf = [0u8; 4];
        if reader.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let len = u32::from_le_bytes(len_buf);
        if len == 0 || len > MAX_FRAME_SIZE {
            break;
        }
        let mut buf = vec![0u8; len as usize];
        if reader.read_exact(&mut buf).await.is_err() {
            break;
        }
        let Ok(env) = serde_json::from_slice::<DaemonEnvelope>(&buf) else {
            continue;
        };
        if tx.send(env).await.is_err() {
            break;
        }
    }
}

/// Locate the daemon TCP port. Order of precedence:
///   1. `NEVOFLUX_POC_DAEMON_PORT` env var.
///   2. `<data_dir>/daemon.port` (Dev mode — manually started daemon).
///   3. `<data_dir>/daemon-managed.port` (managed daemon without
///      explicit port).
pub fn discover_port() -> io::Result<u16> {
    if let Ok(s) = std::env::var("NEVOFLUX_POC_DAEMON_PORT") {
        return s.trim().parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("NEVOFLUX_POC_DAEMON_PORT is not a u16: {:?}", s),
            )
        });
    }
    let data_dir: PathBuf = directories::ProjectDirs::from("com", "nevoflux", "nevoflux")
        .map(|d| d.data_dir().to_path_buf())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "could not locate data directory")
        })?;
    for name in &["daemon.port", "daemon-managed.port"] {
        let p = data_dir.join(name);
        if let Ok(s) = std::fs::read_to_string(&p) {
            if let Ok(port) = s.trim().parse::<u16>() {
                return Ok(port);
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!(
            "no daemon port: set NEVOFLUX_POC_DAEMON_PORT or start daemon \
             so it writes {}/daemon.port",
            data_dir.display()
        ),
    ))
}
