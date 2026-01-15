# rumour

`rumour` is a lightweight, UDP-based gossip networking layer for distributed systems, designed to be rate-limited and BFT-friendly.

It provides a robust message dissemination substrate suitable for consensus systems, peer-to-peer overlays, and decentralized services.

---

## Features

* Gossip / epidemic broadcast
* UDP transport
* Cryptographic message IDs (BLAKE3)
* Replay protection with TTL-based deduplication
* Global and per-sender rate limiting
* Bounded memory usage
* Designed for adversarial environments
* Composable with BFT and consensus protocols (WIP)

---

## Architecture

```
stdin / application
        ↓
 message hashing (BLAKE3)
        ↓
  gossip fanout
        ↓
 UDP transport
        ↓
 global rate limit
        ↓
 per-sender rate limit
        ↓
 TTL deduplication
        ↓
 gossip relay
```

Messages are disseminated probabilistically and converge rapidly across the peers without centralized coordination.

---

## Message Identity

Each message is assigned a deterministic ID derived from:

```
hash(
  PROTOCOL_ID ||
  sender_id ||
  sequence_number ||
  payload
)
```

This provides:

* Collision resistance
* Replay protection
* Sender-scoped ordering
* Cross-protocol isolation

---

## Rate Limiting & DoS Resistance

rumour applies token-bucket rate limiting at multiple layers:

* Global rate limiting — protects against floods and Sybil attacks
* Per-sender rate limiting — isolates misbehaving peers
* TTL eviction — bounds memory growth for deduplication and sender tracking

The system is designed to drop excess traffic early and cheaply, which is critical for adversarial networks.

---

## Usage

### Run a node

```bash
cargo run -- \
  --id 1 \
  --bind 127.0.0.1:4000 \
  --peers 127.0.0.1:4001 127.0.0.1:4002
```

### Send messages

Messages written to `stdin` are gossiped to peers:

```bash
echo "hello world" | cargo run -- --id 1 ...
```

Received messages are printed to `stdout`.

---

## Intended Use Cases

* BFT / consensus networking substrate
* Peer-to-peer overlays
* Distributed logging or event propagation

---

## Non-Goals

rumour intentionally does **not** provide:

* Message ordering guarantees
* Reliability or retransmission
* Persistence
* Membership management
* Authentication (yet)

## License

See [LICENSE](LICENSE) for details.
