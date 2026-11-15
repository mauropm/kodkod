# lora-chat

A privacy-first Linux terminal chat application for the
[RAK4631](https://docs.rakwireless.com/product-categories/wisblock/rak4631/overview/)
WisBlock core module (Adafruit WisCore RAK4631 included), designed for
off-grid LoRa communication.

- **TUI**: keyboard-driven ratatui interface — chat, nodes, radio config,
  privacy, telemetry, diagnostics.
- **Firmware aware**: auto-detects **Meshtastic** or **RAK RUI3** firmware over
  USB serial and speaks each one's documented protocol. A built-in mock device
  simulates the whole experience without hardware.
- **Privacy by default**: GPS is opt-in at three independent gates
  (acquisition / storage / transmission), with precision reduction and an
  audience model. The status bar always shows what is being shared.
- **Encrypted where it matters**: XChaCha20-Poly1305 per fragment,
  replay protection, deduplication, authenticated ACKs, exponential retry.
  Direct messages without a key are refused rather than sent in plaintext.

## Quick start

```bash
cargo install --path .          # or: cargo build --release

lora-chat --scan                # find devices: USB serial + Bluetooth peers
lora-chat --mock                # try the full TUI with simulated peers
lora-chat                       # auto-connect to a detected device
lora-chat --device /dev/ttyACM0
lora-chat --transport ble       # chat over Bluetooth (Meshtastic GATT)
lora-chat --transport ble --device f3ca   # pick a BLE peer by name/MAC fragment
```

No hardware attached? `--mock` runs everything: beacons from simulated nodes
BETA/CHARLIE/DELTA, lossy links, acknowledgements.

## CLI

```
lora-chat [--config PATH] [--device DEV] [--profile NAME] [--mock]
          [--scan] [--diagnostics] [--send TEXT] [--node NAME]
          [--json-status] [--export PATH] [--log-level LEVEL]
```

## Keys

Channel/contact keys live in `~/.config/lora-chat/keys.toml` (created 0600):

```toml
[channels]
General = "6a1f...64 hex chars..."

[contacts]
BETA = "9c02..."
```

Messages on channels with a shared key show as **Encrypted**; keyless channel
traffic is labelled **Unauthenticated broadcast** in the UI. Direct messages
require the recipient's key — lora-chat refuses to leak them in plaintext.

## Keyboard

| Key | Action |
|---|---|
| F1–F8 | Help / Nodes / Chat / Device / Radio / Privacy / Telemetry / Diagnostics |
| Tab / Shift+Tab | move between panels |
| Enter | send message |
| Esc | back to chat |
| Ctrl+C / Ctrl+L | quit / repaint |
| `:` | command palette (`:nodes :radio :privacy :gps :gps-share-everyone :telemetry :send :reset :quit`) |

## Permissions

Serial ports need the `dialout` group (no root required):

```bash
sudo usermod -aG dialout $USER      # then log out/in
sudo cp packaging/udev/70-lora-chat.rules /etc/udev/rules.d/
```

The udev rule also stops ModemManager from grabbing the RAK4631.

## Distribution packages

Build from source everywhere; system packages:

```bash
# Arch (AUR-style PKGBUILD pattern)
sudo pacman -S --needed base rust
makepkg -f packaging/arch/PKGBUILD && sudo pacman -U lora-chat-*.pkg.tar.zst

# Debian / Ubuntu
sudo apt install build-essential pkg-config
cargo deb --help >/dev/null 2>&1 || cargo install cargo-deb
cargo deb && sudo dpkg -i target/debian/lora-chat_*.deb

# Fedora
sudo dnf install rust cargo
rpmbuild -bb packaging/rpm/lora-chat.spec   # spec provided as a starting point
```

A systemd **user** unit template lives in `packaging/systemd/`.

## Documentation

- [docs/hardware.md](docs/hardware.md) — verified hardware/firmware research
- [docs/architecture.md](docs/architecture.md) — layering & data flow
- [docs/protocol.md](docs/protocol.md) — KKP wire protocol spec
- [docs/security.md](docs/security.md) — threat model & honest limitations
- [docs/privacy.md](docs/privacy.md) — location/telemetry policy engine
- [docs/troubleshooting.md](docs/troubleshooting.md) — fixes + firmware matrix
- [docs/development.md](docs/development.md) — building, testing, conventions
- [examples/](examples/) — example config and profiles

## Status

See the verification matrix in docs/hardware.md for exactly what was tested
against real hardware versus validated against vendor documentation. Features
requiring optional WisBlock modules (GNSS, environment sensors) report their
absence instead of pretending.

## License

MIT — see [LICENSE](LICENSE).
