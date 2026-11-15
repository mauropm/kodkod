# Architecture

## Layering

```
+--------------------------------------------------------------+
| TUI (ratatui/crossterm)            screens, palette, modals  |
+--------------------------------------------------------------+
| Application (src/app.rs)           send pipeline, receive    |
|                                    pipeline, retries, node   |
|                                    presence, history         |
+------------------+-------------------+-----------------------+
| Protocol (KKP)   | Privacy/GPS       | Telemetry/Sensors     |
| wire/frag/dedup  | policy engine     | opt-in collectors     |
+------------------+-------------------+-----------------------+
| Device abstraction (command/event channels)                  |
|   mock  |  RUI3 AT adapter  |  Meshtastic protobuf adapter   |
+--------------------------------------------------------------+
| Transport: USB CDC serial (tokio-serial), BLE scan (btleplug) |
+--------------------------------------------------------------+
```

Key decisions:

1. **Devices are tasks, not traits.** Each firmware adapter owns its transport in
   a spawned tokio task and communicates through
   `mpsc::Sender<DeviceCommand>` / `mpsc::Receiver<DeviceEvent>`. This avoids
   dyn-async friction and makes the mock device a first-class citizen.
2. **The application layer is dialect-agnostic.** Adapters normalise inbound
   traffic into KKP frames (`protocol::Frame`). Meshtastic TEXT_MESSAGE payloads
   are wrapped into synthetic unauthenticated Data frames; PRIVATE_APP traffic
   passes through as native KKP frames so end-to-end crypto works over both
   firmwares.
3. **Privacy is an engine, not flags sprinkled in UI code.** Every outbound
   location-bearing payload must pass `privacy::prepare_share`, which enforces
   the audience hierarchy and precision reduction.
4. **Per-fragment AEAD.** Large messages are split first; each fragment is sealed
   independently (XChaCha20-Poly1305, AAD = fragment header). This keeps
   authentication exact even when fragments travel independently over lossy
   radio links.

## Data flow (send)

```
UI input -> App::send_text
  -> key lookup (contact key > channel key)     [DM without key = refused]
  -> Frame{flags} + per-fragment seal
  -> DeviceHandle.send_frame(wire, want_ack)
  -> PendingAck registered                      [retry scheduler in tick()]
```

## Data flow (receive)

```
DeviceEvent::Frame(meta, bytes)
  -> Frame::from_wire                [failure -> crc_errors++]
  -> Deduper (sender,msg_id,frag_idx)[duplicates dropped]
  -> key lookup -> open() verify     [failure -> dropped + notice]
  -> ReplayGuard sliding window      [replays dropped]
  -> Reassembler                     [timeout 30 s, bounded buffers]
  -> MessageStore + AppEvent::Message -> TUI
```

## Storage

Append-only JSONL at `$XDG_DATA_HOME/lora-chat/history.jsonl` with automatic
rotation past 10 MB. Chosen over SQLite to keep the dependency tree small and
corruption recovery trivial; the format is line-delimited JSON so other tools
can consume it directly. `--export` writes a copy anywhere you like.

## Configuration

`$XDG_CONFIG_HOME/lora-chat/config.toml` holds preferences. Secrets live in a
separate `keys.toml` (mode 0600). Profiles are small override files under
`profiles/<name>.toml`; they can only *tighten* defaults and never silently
enable transmission — global GPS sharing always requires interactive
confirmation regardless of profile contents.

## Firmware adapters

| Adapter | Status |
|---|---|
| Mock | full chat simulation, loss/duplication/corruption injection |
| RUI3 AT | implemented against documented command set; syntax unit-tested from real forum logs |
| Meshtastic | framing + protobuf subset implemented; field numbers cross-checked against meshtastic/protobufs |
| BLE scan | behind `ble` feature; discovery only for now |

See docs/hardware.md for what has been verified on live hardware.
