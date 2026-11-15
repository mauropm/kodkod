# Troubleshooting

## Device detection

| Symptom | Cause / fix |
|---|---|
| `--scan` shows nothing | Check `lsusb` for `239a:8029` (or `23a1:*`). If missing, replug the USB cable. |
| Port found but firmware `Unknown` | The board may have just rebooted (opening some ports resets it). Wait ~10 s and re-run; the probe retries automatically three times. |
| `cannot open /dev/ttyACM0: Permission denied` | Add yourself to `dialout`: `sudo usermod -aG dialout $USER`, then log out/in. Install the udev rule from `packaging/udev/`. |
| Responses only sometimes | Another program may hold the port (ModemManager, meshtasticd, a serial monitor). `fuser /dev/ttyACM0` should print nothing before starting lora-chat. |
| Board resets whenever you open a terminal on the port | Normal CDC-open reset behaviour of some builds; lora-chat opens without asserting modem lines to avoid it where possible. |

## Bluetooth

```bash
lora-chat --transport ble                  # chat over BLE (auto-finds Meshtastic/RAK)
lora-chat --transport ble --device f3ca    # filter by name substring or MAC/id
lora-chat --scan                           # lists BLE peers too
```

| Symptom | Fix |
|---|---|
| `no matching BLE device found` | Ensure the node is powered/advertising and the host has a BT adapter (`bluetoothctl show`). |
| `toradio write: In Progress` | The node rejects GATT writes (ATT Busy). Power-cycle the node; stale client slots after abrupt disconnects cause this. Also try `bluetoothctl disconnect <MAC>` first. |
| Permission errors | Ensure your user can use BlueZ (`bluetoothctl scan on` should work without root). |

## Firmware-specific notes

- **Meshtastic**: radio settings are read-only through the client API by
  design; configure region/frequency with a Meshtastic admin client or app.
  Chat works over TEXT_MESSAGE (interop) and PRIVATE_APP (end-to-end crypto).
- **RUI3**: first connection switches the module to P2P mode (`AT+NWM=0`),
  which reboots it — expect a ~3 s pause. If parameters are rejected, check the
  regional caps shown in the Radio screen.
- **Unknown custom firmware**: lora-chat will not talk to undocumented
  interfaces. Flash Meshtastic or RUI3 yourself if desired — flashing is always
  manual and never triggered by this program.

## Radio link issues

- No messages arrive: verify both ends share frequency/SF/BW/CR/preamble/sync
  word exactly; a mismatch is total silence with no error.
- High CRC error count: marginal SNR; try lowering SF distance expectations,
  raise SF (slower), or reduce TX payload size.
- Messages stuck `Retrying`: acks need a shared key path; check `keys.toml`
  has the same `channels.General` hex key on both ends.

## GPS

GPS shows `NOT DETECTED` unless a GNSS WisBlock module (e.g. RAK1910) is
installed **and** the connected firmware exposes it. It stays `OFF` until you
enable acquisition (`:gps`). Nothing is transmitted without the separate
transmission gate + confirmation.

## Logs

Run with `RUST_LOG=debug` (or `--log-level trace`) to see frame-level detail on
stderr. Logs intentionally exclude keys, plaintext and coordinates.

## Resetting state

Message history: `$XDG_DATA_HOME/lora-chat/history.jsonl` (delete or let it
rotate). Keys/config: `$XDG_CONFIG_HOME/lora-chat/`.
