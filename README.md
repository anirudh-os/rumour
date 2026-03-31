# rumour

`rumour` is a lightweight, UDP-based gossip networking layer for distributed systems, designed to be secure, rate-limited, and BFT-ready.

It provides a robust message dissemination substrate suitable for consensus systems, peer-to-peer overlays, and decentralized services.

---

## Features

- Gossip / epidemic broadcast with fanout
- UDP transport
- Cryptographic message IDs (BLAKE3)
- Self-sovereign node identity (ID derived from public key)
- Ed25519 message signing and verification (via `ring`)
- Trust On First Use (TOFU) key pinning with cache eviction
- Replay protection with TTL-based deduplication
- Global and per-sender token-bucket rate limiting
- Bounded memory with background GC on all state
- \Composable with BFT and consensus protocols (WIP)
---

## Architecture

```bash
stdin / application
        ↓
  BLAKE3 message ID
        ↓
  Ed25519 signing
        ↓
  gossip fanout (UDP)
        ↓
 [ receiver pipeline ]
        ↓
  protobuf decode
        ↓
  TOFU key resolution
  (cache hit → use pinned key)
  (cache miss → verify hash(pubkey) == sender_id, then pin after sig check)
        ↓
  Ed25519 signature verification
        ↓
  global rate limit  (500 msg/s, token bucket)
        ↓
  per-sender rate limit  (50 msg/s, token bucket)
        ↓
  TTL deduplication  (30s window)
        ↓
  deliver + epidemic relay (fanout, source excluded)
```

Messages are disseminated probabilistically and converge rapidly across peers without centralized coordination.

---

## Node Identity

Each node derives its ID from its own public key:

```rust
node_id = blake3(ed25519_public_key)[0..8]
```

This provides self-sovereign identity, i.e., no coordinator assigns IDs, no registry is required. Any node can join the network independently by generating a keypair. Node identity is stable as long as the keypair is persisted.

---

## Message Identity

Each message is assigned a deterministic ID derived from:

```rust
blake3(PROTOCOL_ID || sender_id || seq || payload)[0..8]
```

This provides:

- Collision resistance
- Replay protection
- Sender-scoped ordering
- Cross-protocol isolation

---

## Message Authentication

Every message is signed with the sender's Ed25519 private key over a canonical buffer:

```rust
signature = ed25519_sign(msg_id || sender_id || payload)
```

Receivers verify signatures using TOFU key pinning:

1. First message from a sender: verify `hash(sender_public_key) == sender_id`, then verify signature, then pin the key
2. Subsequent messages: use pinned key directly, skip hash derivation
3. Any message with an unrecognised sender ID or invalid signature is hard-dropped before rate limiting

This means sender identity cannot be forged without breaking Ed25519 or finding a BLAKE3 preimage.

---

## Rate Limiting & DoS Resistance

rumour applies token-bucket rate limiting at two layers:

- Global rate limit — 500 msg/s, protects against floods and Sybil attacks
- Per-sender rate limit — 50 msg/s, isolates misbehaving peers

Both limiters use a fractional token accumulator to avoid systematic under-delivery at high call frequencies.

Rate limiting is applied after cryptographic verification, so invalid packets are dropped cheaply without consuming legitimate budget.

---

## Memory Bounds

All mutable state is bounded by TTL-based GC running as background tasks:

| State | TTL | GC Interval |
| --- | --- | --- |
| `seen` (dedup) | 30s | 5s |
| `sender_limits` | 300s | 60s |
| `known_peers` (TOFU cache) | 300s | 60s |

---

## Usage

### Generate a keypair (once, on first boot)

```bash
rumour keygen --output node.key
```

This prints your node ID and public key, and saves the private key to disk. This file is your node's persistent identity.

### Run a node

```bash
rumour node \
  --bind 127.0.0.1:4000 \
  --key-file node.key \
  --peers 127.0.0.1:4001 127.0.0.1:4002
```

### Send messages

Messages written to `stdin` are gossipped to peers:

```bash
echo "hello world" | rumour node --bind 127.0.0.1:4000 --key-file node.key --peers ...
```

Received messages are printed to `stdout`.

### Fanout

Control the relay fanout (default 3):

```bash
rumour node --bind ... --key-file ... --peers ... --fanout 5
```

---

## Intended Use Cases

- BFT / consensus networking substrate
- Drone swarm communication (adversarial WiFi environments)
- Peer-to-peer overlays
- Distributed event propagation

---

## Non-Goals

rumour intentionally does **not** provide:

- Message ordering guarantees
- Reliability or retransmission
- Persistence
- Membership management (dynamic peer discovery is a layer above)
- Full BFT consensus (planned — this is the dissemination substrate)

---

## License

See [LICENSE](LICENSE) for details.
