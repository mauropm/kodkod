# Security & threat model

## Scope and assumptions

- The radio medium is **public**. Anyone in range with an SDR can record
  everything, including encrypted frames' timing and sizes.
- A stolen laptop exposes whatever is on disk. lora-chat stores keys with mode
  0600 under the user's config directory; it does not (yet) integrate a
  DBus secret service.
- The firmware running on the RAK4631 is trusted: it could exfiltrate keys fed
  to it or misbehave. lora-chat never flashes firmware silently.
- No forward secrecy: channel/contact keys are long-term pre-shared secrets.

## Threats and mitigations

| Threat | Mitigation |
|---|---|
| Passive eavesdropping of chat | XChaCha20-Poly1305 with per-message random nonces; plaintext DMs refused by policy |
| Message forgery / modification | Poly1305 tag over ciphertext + header bound as AAD |
| Replay of old frames | dedup LRU + per-sender sliding-window sequence check |
| Node impersonation | PSK possession authenticates senders within a key group; UI shows security label so unauthenticated traffic is never disguised |
| Flooding / DoS | bounded reassembly buffers (8 messages, 30 s expiry), dedup LRU 1024, node registry cap 512, retry ceiling (3), rate-limited beacons (30 s) |
| Malicious config packets | host never applies unsolicited configuration; Meshtastic adapter rejects remote radio changes entirely |
| Accidental location disclosure | privacy engine gates every location payload; global sharing needs double interactive confirmation; profiles cannot override |
| Stolen machine | keys are file-permission protected only — documented residual risk; keep disk encryption enabled |

## Honest limitations

- Broadcast channels use group PSKs: any member can read, and membership
  compromise is invisible to others.
- No asymmetric key exchange yet (`X25519` handshake is designed but not
  implemented); contact "keys" today are shared secrets exchanged out of band.
- ACKs from unknown-key senders are accepted as transport signals (they cannot
  fabricate message content, only delivery state).
- The threat model does not (yet) cover side channels such as packet-timing
  fingerprinting.

## Logging discipline

The default log level is INFO and logs never contain: key material, message
plaintext, GPS coordinates, or AppKeys. Debug/trace levels may show frame
headers and hex payloads — enable them only when debugging.
