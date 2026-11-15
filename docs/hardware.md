# Hardware research: RAK4631 / Adafruit WisCore RAK4631

Status of every claim in this file is marked **[verified]** (checked against official
documentation or observed on the attached unit), **[documented]** (official vendor
documentation only), or **[unverified]** (assumed, needs hardware confirmation).

## 1. The module

The RAK4631 is a WisBlock **Core** module built around:

- Nordic **nRF52840**: ARM Cortex-M4F @ 64 MHz, 1 MB flash, 256 KB RAM, BLE 5.0
  (TX power -20 to +4 dBm). **[documented]**
- Semtech **SX1262** LoRa transceiver, up to **+22 dBm**, PA boost mode.
  SX1262 init requirements for this board: DIO2 controls the antenna switch,
  DIO3 controls the TCXO supply, DCDC regulator enabled (not LDO), P1.07 left alone.
  **[documented]** (RAK4631 datasheet)
- Variants: RAK4631(H) covers 779–923 MHz; RAK4631(L) covers 433–470 MHz.
  RAK4631 vs RAK4631-R are **hardware-identical**; only stock firmware differs
  (Arduino BSP vs RUI3). **[documented]**
- The "RAK4631" is a RAK4630 stamp plus an expansion PCB with WisBlock
  connectors. USB is CDC ACM. **[documented]**

## 2. GPS/sensors are NOT part of the module — [verified]

The RAK4631 provides **no GNSS receiver and no environmental sensors**. Those come
from optional WisBlock modules plugged into a base board (IO/sensor slots):

| Function | Module | Chip |
|---|---|---|
| GNSS | RAK1910 | u-blox MAX-7Q |
| Temp/humidity | RAK1901 | SHTC3 |
| Pressure | RAK1902 | LPS22HB |
| Light | RAK1903 | OPT3001 |
| Accelerometer | RAK1904 | LIS3DH |
| Environment (all-in-one) | RAK1906 | BME680 |

lora-chat therefore treats GPS and sensors as **capabilities to detect**, never as
assumptions. When absent the UI reports `not detected` / `requires optional module`.

## 3. The attached unit

```
USB ID      239a:8029   ("Adafruit" VID, product string "WisCore RAK4631 Board")
Serial      CE35C36C6A2B9074
/dev/ttyACM0 (CDC ACM)
BLE         nRF52840 controller present on board (advertising not observed)
```
**[verified]**

## 4. Firmware identification — what we actually found

Probing history on this unit (all read-only; no LoRa transmissions were made):

1. Serial silence to `AT` probes at 9600/19200/38400/57600/115200/230400/421600/
   921600 baud → rules out default RUI3 AT console. **[verified]**
2. Opening the port with pyserial (which asserts DTR/RTS) repeatedly reset the
   board; one capture caught a **Meshtastic boot banner**: ANSI-coloured `DEBUG`
   log lines enumerating `config.proto`, `module.proto`, `channels.proto`.
   → The unit runs **Meshtastic firmware**. **[verified]**
3. A non-DTR-asserting open followed by Meshtastic `want_config_id` handshake was
   answered once with valid framed `FromRadio` traffic, confirming the wire
   protocol implementation. Later probes went silent after the repeated
   DTR-reset storms wedged the CDC stack; a physical replug is expected to
   recover it. **[verified once, then device unresponsive]**

Consequences implemented in lora-chat:

- Firmware auto-detection order: passive listen → RUI3 `AT` probe →
  Meshtastic `want_config` handshake → boot-banner heuristic → unknown.
- Probes retry three times with backoff because freshly-rebooted devices answer
  late.
- Nothing ever flashes or reconfigures the device implicitly.

## 5. Documented host protocols supported by lora-chat

### RUI3 AT command interface (RAKwireless official) [documented]

Verified against RAKwireless docs/forum sources:

- Mode switch: `AT+NWM=0` (P2P LoRa; may reboot the module)
- Combined radio setup: `AT+P2P=<freq Hz>:<sf>:<bw kHz>:<cr>:<preamble>:<power dBm>`
- Individual fallbacks: `AT+PFREQ`, `AT+PSF`, `AT+PBW` (index: 0=125k, 1=250k,
  2=500k), `AT+PCR` (0=4/5 … 3=4/8), `AT+PPL`, `AT+PTP` (max 22)
- Receive: `AT+PRECV=65533` continuous RX that still permits TX without leaving RX;
  `65535` continuous until first packet; `65534` indefinite; `0` stop.
  Known quirk: reconfiguring P2P while in RX requires `AT+PRECV=0` first, and
  `AT_BUSY_ERROR` can appear when already receiving — handled automatically.
- Send: `AT+PSEND=<hex payload>`
- Events: `+EVT:RXP2P:<rssi>:<snr>:<hex>`, `+EVT:RXP2P RECEIVE TIMEOUT`,
  `+EVT:RXP2P: RECEIVE ERROR`, `+EVT:TXP2P DONE`
- Version: `AT+VER=?` returns e.g. `RUI_4.1.0_RAK4631`

### Meshtastic client API [documented]

- Stream framing over USB serial: `0x94 0xC3 <len_hi> <len_lo>` + protobuf
  (`ToRadio` towards device, `FromRadio` from device), max payload 512 bytes.
  Host should send four `0x94` bytes as resync before the first request.
- Connect flow: send `ToRadio.want_config_id`; device streams node DB, config,
  channels, metadata, then echoes `config_complete_id`.
- Mesh packets carry `Data{portnum, payload}`; TEXT_MESSAGE_APP=1,
  NODEINFO_APP=4, ROUTING_APP=5 (acks reference `request_id`),
  TELEMETRY_APP=67 (battery/voltage), PRIVATE_APP=256 (opaque).
- Radio settings live in `Config.LoRaConfig` and cannot be changed through the
  plain client API — lora-chat surfaces this honestly instead of pretending.

## 6. Regional power/frequency facts encoded in lora-chat [documented]

Region policies (band edges, conservative power caps, duty-cycle notes) are
centralised in `src/regions.rs`. The SX1262 hardware cap (22 dBm) always applies.
The table is deliberately conservative; it is not legal advice.

## 7. Known unknowns

- Battery reporting over RUI3 AT has no documented generic query on the nRF52
  build; marked *requires firmware support* rather than guessed.
- BLE transport: implemented (default feature) against the documented Meshtastic
  GATT layout — service `6BA1B218-15A8-461F-9FA8-5DCAE273EAFD`,
  FromRadio `2c55e69e…` (read), ToRadio `f75c76d2…` (write),
  FromNum `5a3d6e49…` (notify). Verified live against a real node
  (`Meshtastic_f3ca`): discovery ✓, connect ✓, service/characteristic discovery ✓,
  reads ✓, FromNum subscription ✓. **Writes currently rejected by the node**
  (ATT Busy on every ToRadio WriteValue, raw BlueZ confirmed) — the node's BLE
  client slot appears wedged from prior abrupt disconnects; a node power-cycle
  is expected to clear it. Status: end-to-end BLE chat *pending node reboot*.
- WisBlock-API-V2 custom `ATC+` command set is not implemented (roadmap).
