# KKP — the lora-chat P2P protocol (version 1)

KKP is the compact binary protocol lora-chat speaks over raw LoRa P2P (via RUI3)
and, encapsulated, over Meshtastic PRIVATE_APP traffic. It exists so that
end-to-end encryption and message identity do not depend on which firmware the
radio runs.

## Frame layout

```
+--------+--------+------------+------------+---------+-----------+
| ver=1  | type<<4| flags      | (varints)  | nonce24 | payload   |
+--------+--------+------------+------------+---------+-----------+
```

- byte 0: protocol version (currently `0x01`)
- byte 1: high nibble = frame type, low nibble = flags
- varint fields in order: msg_id u32, sender u32, seq u16, timestamp u32
- two bytes: frag_total u8, frag_index u8
- if FLAG_ENCRYPTED: 24-byte XChaCha nonce
- varint payload length + payload

Frame types: Data=1, Ack=2, Hello=3, Telemetry=4, NodeInfo=5.
Flags: `0x01` encrypted, `0x02` authenticated, `0x04` emergency,
`0x08` want-ack.

Typical sizes: header ≈ 14–20 bytes; a 160-byte chat message fits one LoRa
frame at SF7/BW125 with room to spare.

## Fragmentation

Messages larger than `chat.max_frame_payload` (default 200 B) are split into
fragments sharing `msg_id`; each fragment carries its own `seq`
(base seq + index), `frag_total`, `frag_index`. The reassembler:

- buffers by (sender, msg_id), expires after 30 s,
- rejects conflicting frag_totals,
- caps concurrent buffers to resist flooding.

## Deduplication

LRU set keyed on `(sender, msg_id, frag_index)`, capacity 1024. Radio-level
duplicates (a real phenomenon on LoRa mesh/flood networks and injected at high
rate by the mock device) are dropped here.

## Replay protection

Per-sender sliding window of 64 sequence numbers, wrapping-safe. A replayed
frame is dropped *after* dedup catches exact repeats but before any content is
processed.

## Crypto

- AEAD: **XChaCha20-Poly1305** (RustCrypto `chacha20poly1305`). 192-bit random
  nonces remove counter-management risk across reboots.
- AAD binds: version, type, flags, msg_id, sender, seq, timestamp,
  fragment counters, payload length. Header tampering fails authentication.
- Keys are 32 bytes, hex-encoded in `keys.toml` (`channels.*`,
  `contacts.<name>`).
- ACK frames are authenticated when a key is available (empty plaintext +
  tag authenticates the AAD).

Security label shown in the UI:
- `Encrypted` — sealed with a shared key (channel or contact PSK)
- `Unauthenticated broadcast` — plaintext channel broadcast (visible to all)
- Direct messages without a key are refused outright.

## Acknowledgements & retries

Frames flagged want-ack expect an Ack frame (or Meshtastic ROUTING_APP ack)
referencing `msg_id`. The retry scheduler (in `App::tick`) retransmits all
fragment wires up to `chat.retry_count` times with exponential backoff
(`ack_timeout_secs × 3^n`), then marks the message `Failed`. Delivery states:
Sending → Delivered | Retrying(n left) → Failed.

## Encapsulation over Meshtastic

Outbound: unencrypted single-fragment UTF-8 Data frames go out as
TEXT_MESSAGE_APP (so stock Meshtastic peers can read them); anything else goes
as PRIVATE_APP opaque bytes. Inbound TEXT_MESSAGE payloads are wrapped into
synthetic KKP Data frames (sender = node number, id = packet id) so downstream
handling is uniform. Routing acks map back onto `msg_id` via `request_id`.

## Limits

max message 4096 B · max frame payload configurable (default 200 B) ·
reassembly window 30 s · dedup LRU 1024 · replay window 64 ·
node registry cap 512 (LRU by last-seen).
