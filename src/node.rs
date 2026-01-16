use crate::proto::GossipEnvelope;
use crate::ratelimit::RateLimiter;
use anyhow::Result;
use blake3::Hasher;
use prost::Message;
use rand::{prelude::IndexedRandom, rngs::SmallRng, SeedableRng, rng};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{
    net::UdpSocket,
    sync::Mutex,
    time::{interval, Duration, Instant},
};

const PROTOCOL_ID: &[u8] = b"GOSSIP";
const SEEN_TTL: Duration = Duration::from_secs(30);
const SENDER_TTL: Duration = Duration::from_secs(300);

/// Represents the state of a Gossip Node
pub struct Node {
    pub id: u64,
    socket: Arc<UdpSocket>,
    peers: Vec<SocketAddr>,
    fanout: usize,
    // Shared state
    seen: Arc<Mutex<HashMap<u64, Instant>>>,
    sender_limits: Arc<Mutex<HashMap<u64, (RateLimiter, Instant)>>>,
    global_rate: Arc<Mutex<RateLimiter>>,
}

impl Node {
    pub async fn new(id: u64, bind_addr: &str, peers: Vec<String>, fanout: usize) -> Result<Self> {
        let socket = Arc::new(UdpSocket::bind(bind_addr).await?);
        let peer_addrs: Vec<SocketAddr> = peers.iter().map(|p| p.parse().unwrap()).collect();

        //println!("Node {} listening on {}", id, bind_addr);

        Ok(Self {
            id,
            socket,
            peers: peer_addrs,
            fanout,
            seen: Arc::new(Mutex::new(HashMap::new())),
            sender_limits: Arc::new(Mutex::new(HashMap::new())),
            global_rate: Arc::new(Mutex::new(RateLimiter::new(500, 1000))),
        })
    }

    /// Spawns the background tasks (Receiver, GC)
    pub fn start_background_tasks(&self) {
        self.spawn_receiver();
        self.spawn_seen_gc();
        self.spawn_limits_gc();
    }

    /// Generates a hash ID and broadcasts the message to random peers
    pub async fn broadcast(&self, payload: Vec<u8>, seq: u64) -> Result<()> {
        let mut hasher = Hasher::new();
        hasher.update(PROTOCOL_ID);
        hasher.update(&self.id.to_le_bytes());
        hasher.update(&seq.to_le_bytes());
        hasher.update(&payload);

        let hash = hasher.finalize();
        let msg_id = u64::from_le_bytes(hash.as_bytes()[..8].try_into().unwrap());

        let msg = GossipEnvelope {
            msg_id,
            sender_id: self.id,
            payload,
        };

        let mut buf = Vec::new();
        msg.encode(&mut buf)?;

        let mut rng = rng();
        let targets = self.peers.choose_multiple(&mut rng, self.fanout);

        for peer in targets {
            self.socket.send_to(&buf, peer).await?;
        }
        Ok(())
    }

    fn spawn_receiver(&self) {
        let socket = self.socket.clone();
        let seen = self.seen.clone();
        let peers = self.peers.clone();
        let global_rate = self.global_rate.clone();
        let sender_limits = self.sender_limits.clone();

        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            loop {
                if let Ok((len, _)) = socket.recv_from(&mut buf).await {
                    // Global Rate Limit
                    let mut limiter = global_rate.lock().await;
                    if !limiter.allow() { continue; }
                    drop(limiter);

                    if let Ok(msg) = GossipEnvelope::decode(&buf[..len]) {
                        // Per-Sender Rate Limit
                        let mut limits = sender_limits.lock().await;
                        let entry = limits
                            .entry(msg.sender_id)
                            .or_insert_with(|| (RateLimiter::new(50, 100), Instant::now()));
                        entry.1 = Instant::now(); // Update last seen
                        if !entry.0.allow() { continue; }
                        drop(limits);

                        // Deduplication
                        let mut seen_guard = seen.lock().await;
                        let now = Instant::now();
                        if let Some(ts) = seen_guard.get(&msg.msg_id) {
                            if now.duration_since(*ts) < SEEN_TTL { continue; }
                        }
                        seen_guard.insert(msg.msg_id, now);
                        drop(seen_guard);

                        // Log and Re-gossip
                        //println!("RECV {} FROM {} SIZE {}", msg.msg_id, msg.sender_id, msg.payload.len());

                        let mut rng = SmallRng::from_os_rng();
                        let targets: Vec<_> = peers.choose_multiple(&mut rng, 3).collect();

                        for peer in targets {
                            let _ = socket.send_to(&buf[..len], peer).await;
                        }
                    }
                }
            }
        });
    }

    fn spawn_seen_gc(&self) {
        let seen = self.seen.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                seen.lock().await.retain(|_, ts| Instant::now().duration_since(*ts) < SEEN_TTL);
            }
        });
    }

    fn spawn_limits_gc(&self) {
        let limits = self.sender_limits.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                limits.lock().await.retain(|_, (_, last_seen)| Instant::now().duration_since(*last_seen) < SENDER_TTL);
            }
        });
    }
}