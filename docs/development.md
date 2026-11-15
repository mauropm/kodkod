# Development

## Build & test

```
cargo build            # debug build
cargo test             # 84 tests: unit + integration (no hardware needed)
cargo clippy --all-targets   # must stay clean
cargo build --release
cargo build --features ble    # enables BLE scanning via btleplug/BlueZ
```

The mock device (`--mock`) provides the full chat loop — beacons, chat peers,
acknowledgements with configurable loss/duplication/corruption — so every
feature except real RF can be exercised on any Linux machine.

## Layout

| Path | Contents |
|---|---|
| `src/protocol/` | KKP wire codec, fragmentation, dedup |
| `src/crypto/` | AEAD seal/open, replay window, keystore |
| `src/device/` | transport traits, mock, serial, rui3, meshtastic, ble |
| `src/app.rs` | orchestrator: pipelines, retries, presence |
| `src/tui/` | ratatui screens and input handling |
| `tests/` | end-to-end app-level tests over the mock path |

## Testing philosophy

- Protocol/crypto/privacy logic is pure and unit-tested heavily.
- Adapters are tested against fixtures captured from vendor documentation and
  forum logs (e.g. real `+EVT:RXP2P` lines).
- Integration tests drive `App` directly with crafted frames; no TTY needed.
- TUI rendering is kept thin (state → widgets) so state transitions are tested
  at the App level.

## Adding a firmware adapter

1. Create `src/device/<name>/` with a `connect(path) -> (DeviceHandle,
   Receiver<DeviceEvent>)` function.
2. Normalise inbound traffic into KKP frames (see Meshtastic adapter for the
   encapsulation pattern).
3. Add detection to `device::probe_firmware`.
4. Extend the compatibility matrix in docs/troubleshooting.md.

## Conventions

- No `unwrap()` in non-test code paths that touch device data.
- Errors: `thiserror` enum in `src/error.rs`; adapters never panic on wire data.
- Every capability claim maps to an explicit `CapabilityState`
  (supported / detected / not detected / requires firmware support /
  requires optional module / not tested). Never mark hardware features as
  supported merely because code compiles.

## Verification checklist for hardware changes

After touching a device-facing module, re-run against live hardware:

1. `lora-chat --scan` identifies the unit and its firmware.
2. `lora-chat --diagnostics` completes without error.
3. A two-node exchange round-trips (or mock parity if only one node exists).

Record what was verified in docs/hardware.md — honestly distinguishing
*verified*, *documented*, and *unverified*.
