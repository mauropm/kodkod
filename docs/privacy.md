# Privacy model

lora-chat treats **location, telemetry and presence as sensitive by default**.

## Location controls (independent switches)

- GPS hardware acquisition — when off, no GNSS module is queried at all
- Local storage of fixes
- Transmission master gate
- Audience: Nobody → Selected contacts → Selected channels → Trusted nodes → Everyone
- Field-level: altitude, speed, heading, timestamp — each separately gated
- Precision reduction applied at share time:

| Setting | Behaviour |
|---|---|
| exact | raw coordinates |
| 10 m | rounded to 10 m grid |
| 100 m | snapped to 100 m cell centre |
| 1 km | snapped to 1 km cell centre (default) |
| 10 km | snapped to 10 km cell centre |
| city | ~5.5 km grid |

`gps::prepare_share` is the only code path that produces outbound coordinates;
it refuses unless *both* the master switch and audience rules allow it, then
applies precision and strips unapproved fields.

## Global sharing confirmation flow

Enabling `share_location` with audience `Everyone` requires a two-step modal
(y, then y again). The status bar turns red while `GPS: SHARING` is active and
the privacy line (`GPS | Telemetry | Presence | Battery`) is visible on every
screen. `:gps-share-everyone` toggles it off instantly.

## Profiles

Profiles (`examples/profiles/*.toml`) adjust convenience flags (presence,
battery, telemetry). They deliberately **cannot** set location sharing to
Everyone or flip the transmit master gate — those require interactive
confirmation in the running app.

## Presence & discovery

Hello beacons advertise your node name every 30 s while connected. Turning off
`share_presence` stops beacons; discovery responses honour
`privacy.respond_to_discovery`.

## What the UI always shows

`GPS: OFF | Telemetry: OFF | Presence: ON | Battery: OFF`
— computed live from the effective settings, never cached state.
