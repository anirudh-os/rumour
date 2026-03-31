use crate::crypto;
use crate::proto::GossipEnvelope;
use crate::ratelimit::RateLimiter;
use anyhow::Result;
use blake3::Hasher;
use prost::Message;
use rand::{SeedableRng, prelude::{IndexedRandom, IteratorRandom}, rng, rngs::SmallRng};
use ring::signature::{Ed25519KeyPair, KeyPair};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::{
    net::UdpSocket,
    sync::Mutex,
    time::{Duration, Instant, interval},
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
    keypair: Ed25519KeyPair,
    public_key: Vec<u8>,
    
    // Dynamic cache: peer public keys with timestamps (TOFU — Trust On First Use)
    // Stores: sender_id -> (public_key, last_seen_time)
    // Used for key pinning: accept a sender_id + public_key pair once, then reuse that key
    // Evicted after SENDER_TTL of inactivity
    known_peers: Arc<Mutex<HashMap<u64, (Vec<u8>, Instant)>>>,
    
    // Shared state
    seen: Arc<Mutex<HashMap<u64, Instant>>>,
    sender_limits: Arc<Mutex<HashMap<u64, (RateLimiter, Instant)>>>,
    global_rate: Arc<Mutex<RateLimiter>>,
}

impl Node {
    pub async fn new(
        bind_addr: &str,
        peers: Vec<String>,
        fanout: usize,
        private_key_bytes: Vec<u8>,
    ) -> Result<Self> {
        let socket = Arc::new(UdpSocket::bind(bind_addr).await?);
        let keypair = crypto::load_keypair(&private_key_bytes)?;
        let public_key = keypair.public_key().as_ref().to_vec();
        
        // Derive node ID from public key (self-sovereign identity)
        let id = crypto::derive_node_id(&public_key);

        // Parse peer addresses, with proper error handling
        let peer_addrs: Vec<SocketAddr> = peers.iter()
            .map(|p| p.parse::<SocketAddr>()
                .map_err(|e| anyhow::anyhow!("Invalid peer address '{}': {}", p, e)))
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            id,
            socket,
            peers: peer_addrs,
            fanout,
            keypair,
            public_key,
            known_peers: Arc::new(Mutex::new(HashMap::new())),
            seen: Arc::new(Mutex::new(HashMap::new())),
            sender_limits: Arc::new(Mutex::new(HashMap::new())),
            global_rate: Arc::new(Mutex::new(RateLimiter::new(500, 1000))),
        })
    }

    /// Spawns the background tasks (Receiver, GC for seen/limits/known_peers)
    pub fn start_background_tasks(&self) {
        self.spawn_receiver();
        self.spawn_seen_gc();
        self.spawn_limits_gc();
        self.spawn_known_peers_gc();
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

        // Mark as seen before sending so relayed echoes are dropped
        self.seen.lock().await.insert(msg_id, Instant::now());

        // Construct the canonical buffer to sign
        let to_sign = crypto::canonical_message(msg_id, self.id, &payload);
        let signature = crypto::sign(&self.keypair, &to_sign);

        let msg = GossipEnvelope {
            msg_id,
            sender_id: self.id,
            payload,
            signature,
            sender_public_key: self.public_key.clone(),
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
        let known_peers = self.known_peers.clone();
        let fanout = self.fanout;

        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            let mut rng = SmallRng::from_os_rng();

            loop {
                if let Ok((len, src_addr)) = socket.recv_from(&mut buf).await {
                    // Decode envelope
                    if let Ok(msg) = GossipEnvelope::decode(&buf[..len]) {
                        // Step 1: TOFU validation (Trust On First Use) — get key to verify with
                        // Check known_peers cache first. For new senders, verify hash(public_key) == sender_id.
                        // Cache insertion deferred to after signature verification (Step 2).
                        let (authenticated_key, is_new_peer) = {
                            let mut cache = known_peers.lock().await;
                            match cache.get_mut(&msg.sender_id) {
                                Some((cached_key, last_seen)) => {
                                    *last_seen = Instant::now();
                                    (cached_key.clone(), false)
                                }
                                None => {
                                    // New peer: must have public_key that hashes to sender_id
                                    if crypto::derive_node_id(&msg.sender_public_key) != msg.sender_id {
                                        continue; // Forged sender_id
                                    }
                                    (msg.sender_public_key.clone(), true)
                                }
                            }
                        };

                        // Step 2: Verify signature using authenticated public key
                        let to_verify = crypto::canonical_message(msg.msg_id, msg.sender_id, &msg.payload);
                        if crypto::verify(&authenticated_key, &to_verify, &msg.signature).is_err() {
                            continue; // Signature verification failed
                        }

                        // Step 2b: Cache new peers only after signature verification
                        if is_new_peer {
                            let mut cache = known_peers.lock().await;
                            cache.insert(msg.sender_id, (authenticated_key.clone(), Instant::now()));
                        }

                        // Step 3: Global Rate Limit
                        let mut limiter = global_rate.lock().await;
                        if !limiter.allow() {
                            continue;
                        }
                        drop(limiter);

                        // Step 4: Per-Sender Rate Limit
                        let mut limits = sender_limits.lock().await;
                        let entry = limits
                            .entry(msg.sender_id)
                            .or_insert_with(|| (RateLimiter::new(50, 100), Instant::now()));
                        entry.1 = Instant::now(); // Update last seen
                        if !entry.0.allow() {
                            continue;
                        }
                        drop(limits);

                        // Step 5: Deduplication check
                        let mut seen_guard = seen.lock().await;
                        let now = Instant::now();
                        if let Some(ts) = seen_guard.get(&msg.msg_id) {
                            if now.duration_since(*ts) < SEEN_TTL {
                                continue;
                            }
                        }
                        seen_guard.insert(msg.msg_id, now);
                        drop(seen_guard);

                        // All checks passed - deliver locally
                        if let Ok(text) = String::from_utf8(msg.payload) {
                            println!("{}", text);
                        }

                        // Relay the message to random peers (excluding the source)
                        // Filter source first to guarantee effective fanout
                        let mut relay_buf = [0u8; 2048];
                        relay_buf[..len].copy_from_slice(&buf[..len]);
                        
                        for peer in peers.iter().filter(|p| **p != src_addr).choose_multiple(&mut rng, fanout) {
                            let _ = socket.send_to(&relay_buf[..len], peer).await;
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
                seen.lock()
                    .await
                    .retain(|_, ts| Instant::now().duration_since(*ts) < SEEN_TTL);
            }
        });
    }

    fn spawn_limits_gc(&self) {
        let limits = self.sender_limits.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                limits.lock().await.retain(|_, (_, last_seen)| {
                    Instant::now().duration_since(*last_seen) < SENDER_TTL
                });
            }
        });
    }

    fn spawn_known_peers_gc(&self) {
        let known_peers = self.known_peers.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                known_peers.lock().await.retain(|_, (_, last_seen)| {
                    Instant::now().duration_since(*last_seen) < SENDER_TTL
                });
            }
        });
    }
}
