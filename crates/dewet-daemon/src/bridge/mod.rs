mod messages;

use std::{
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{broadcast, mpsc},
};
use tokio_tungstenite::{
    accept_hdr_async,
    tungstenite::{Message, handshake::server::Request},
};
use tracing::{debug, error, info, warn};

use crate::config::BridgeConfig;

pub use messages::{ChatPacket, ClientMessage, DaemonMessage, MemoryNode};

const INCOMING_BUFFER: usize = 256;
const BROADCAST_BUFFER: usize = 256;

pub struct Bridge {
    incoming_rx: mpsc::Receiver<ClientMessage>,
    outgoing_tx: broadcast::Sender<DaemonMessage>,
}

impl Bridge {
    pub async fn bind(config: BridgeConfig) -> Result<Self> {
        let listener = TcpListener::bind(&config.listen_addr).await?;
        info!("Bridge listening on {}", config.listen_addr);

        let (incoming_tx, incoming_rx) = mpsc::channel(INCOMING_BUFFER);
        let (outgoing_tx, _) = broadcast::channel(BROADCAST_BUFFER);

        let acceptor = BridgeAcceptor {
            listener,
            incoming_tx,
            outgoing_tx: outgoing_tx.clone(),
            max_clients: config.max_clients,
        };

        tokio::spawn(async move {
            if let Err(err) = acceptor.run().await {
                error!(?err, "bridge acceptor exited");
            }
        });

        Ok(Self {
            incoming_rx,
            outgoing_tx,
        })
    }

    pub fn broadcast(&self, message: DaemonMessage) -> Result<()> {
        // Ignore send errors - they just mean no clients are connected
        let _ = self.outgoing_tx.send(message);
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DaemonMessage> {
        self.outgoing_tx.subscribe()
    }

    pub async fn next_message(&mut self) -> Option<ClientMessage> {
        self.incoming_rx.recv().await
    }

    pub fn handle(&self) -> BridgeHandle {
        BridgeHandle {
            outgoing_tx: self.outgoing_tx.clone(),
        }
    }
}

#[derive(Clone)]
pub struct BridgeHandle {
    outgoing_tx: broadcast::Sender<DaemonMessage>,
}

impl BridgeHandle {
    pub fn broadcast(&self, message: DaemonMessage) -> Result<()> {
        // Ignore send errors - they just mean no clients are connected
        let _ = self.outgoing_tx.send(message);
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<DaemonMessage> {
        self.outgoing_tx.subscribe()
    }
}

struct BridgeAcceptor {
    listener: TcpListener,
    incoming_tx: mpsc::Sender<ClientMessage>,
    outgoing_tx: broadcast::Sender<DaemonMessage>,
    max_clients: usize,
}

impl BridgeAcceptor {
    async fn run(self) -> Result<()> {
        let active = Arc::new(AtomicUsize::new(0));

        loop {
            let (stream, addr) = self.listener.accept().await?;
            let current = active.load(Ordering::SeqCst);
            if current >= self.max_clients {
                warn!("Rejecting {addr} – max clients reached ({current})");
                continue;
            }

            let incoming_tx = self.incoming_tx.clone();
            let outgoing_tx = self.outgoing_tx.clone();
            let active_count = active.clone();

            active_count.fetch_add(1, Ordering::SeqCst);

            tokio::spawn(async move {
                if let Err(err) =
                    handle_connection(stream, addr, incoming_tx, outgoing_tx, active_count).await
                {
                    warn!(?err, "Bridge client error");
                }
            });
        }
    }
}

async fn handle_connection(
    stream: TcpStream,
    addr: SocketAddr,
    incoming_tx: mpsc::Sender<ClientMessage>,
    outgoing_tx: broadcast::Sender<DaemonMessage>,
    active: Arc<AtomicUsize>,
) -> Result<()> {
    let callback =
        |req: &Request, response: tokio_tungstenite::tungstenite::handshake::server::Response| {
            debug!("Bridge connection from {addr}: {req:?}");
            Ok(response)
        };
    let ws_stream = accept_hdr_async(stream, callback).await?;
    let (mut writer, mut reader) = ws_stream.split();
    let mut outgoing_rx = outgoing_tx.subscribe();

    // send hello
    let hello = DaemonMessage::Hello {
        version: env!("CARGO_PKG_VERSION").into(),
        capabilities: vec!["bridge".into(), "chat".into(), "optical-memory".into()],
    };
    let _ = outgoing_tx.send(hello);

    let writer_task = tokio::spawn(async move {
        while let Ok(msg) = outgoing_rx.recv().await {
            let payload = serde_json::to_string(&msg)?;
            writer.send(Message::Text(payload)).await?;
        }
        Ok::<(), anyhow::Error>(())
    });

    while let Some(message) = reader.next().await {
        match message {
            Ok(Message::Text(text)) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(parsed) => {
                    if let Err(err) = incoming_tx.send(parsed).await {
                        warn!(?err, "Dropping client message");
                    }
                }
                Err(err) => warn!(?err, "Invalid client payload {text}"),
            },
            Ok(Message::Binary(_)) => {
                warn!("Binary payloads are not supported");
            }
            Ok(Message::Close(frame)) => {
                info!("Client {addr} closed: {frame:?}");
                break;
            }
            Ok(_) => {}
            Err(err) => {
                warn!(?err, "Bridge read error");
                break;
            }
        }
    }

    writer_task.abort();
    let _ = writer_task.await;
    active.fetch_sub(1, Ordering::SeqCst);
    info!("Client {addr} disconnected");
    Ok(())
}
