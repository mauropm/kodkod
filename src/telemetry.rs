use crate::config::TelemetryConfig;
use crate::gps::SharedLocation;
use crate::privacy::PrivacySettings;
use crate::sensors::SensorRegistry;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct TelemetryField {
    pub key: String,
    pub value: String,
}

pub struct TelemetryOutcome(pub Vec<TelemetryField>);

pub fn build_payload(
    cfg: &TelemetryConfig,
    privacy: &PrivacySettings,
    battery_percent: Option<u8>,
    battery_voltage: Option<f32>,
    location: Option<&SharedLocation>,
    sensors: &mut SensorRegistry,
) -> TelemetryOutcome {
    let mut fields = Vec::new();
    if !cfg.enabled || !privacy.share_telemetry {
        return TelemetryOutcome(fields);
    }
    if cfg.include_battery && privacy.share_battery {
        if let Some(pct) = battery_percent {
            fields.push(TelemetryField {
                key: "battery".into(),
                value: format!("{pct}%"),
            });
        }
        if let Some(v) = battery_voltage {
            fields.push(TelemetryField {
                key: "voltage".into(),
                value: format!("{v:.2}V"),
            });
        }
    }
    if cfg.include_location {
        if let Some(loc) = location {
            fields.push(TelemetryField {
                key: "location".into(),
                value: format!("{:.5},{:.5}", loc.lat, loc.lon),
            });
            if let Some(alt) = loc.altitude_m {
                fields.push(TelemetryField {
                    key: "altitude".into(),
                    value: format!("{alt:.0}m"),
                });
            }
        }
    }
    if cfg.include_environment {
        for (_, readings) in sensors.scan_all() {
            if let Ok(rs) = readings {
                for r in rs {
                    fields.push(TelemetryField {
                        key: r.capability.label().replace(' ', "_"),
                        value: format!("{:.1}{}", r.value, r.unit),
                    });
                }
            }
        }
    }
    TelemetryOutcome(fields)
}

pub fn is_due(last_sent_secs: u32, now_secs: u32, interval_seconds: u64) -> bool {
    if interval_seconds == 0 {
        return false;
    }
    now_secs.saturating_sub(last_sent_secs) as u64 >= interval_seconds
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::config::TelemetryConfig;
    use crate::gps::SharedLocation;
    use crate::privacy::{Audience, PrivacySettings};

    fn telemetry_cfg() -> TelemetryConfig {
        TelemetryConfig {
            enabled: true,
            interval_seconds: 300,
            include_battery: true,
            include_location: true,
            include_environment: false,
        }
    }

    fn privacy_all_confirmed() -> PrivacySettings {
        let mut p = PrivacySettings::default();
        p.share_telemetry = true;
        p.share_battery = true;
        p.location_audience = Audience::Everyone;
        p.confirm_everyone();
        p.share_location = true;
        p
    }

    fn sample_loc() -> SharedLocation {
        SharedLocation {
            lat: -33.86,
            lon: 151.20,
            altitude_m: Some(58.0),
            speed_mps: None,
            heading_deg: None,
            include_timestamp: false,
            precision_label: "1 km".into(),
        }
    }

    #[test]
    fn disabled_telemetry_sends_nothing() {
        let mut tc = telemetry_cfg();
        tc.enabled = false;
        let out = build_payload(
            &tc,
            &privacy_all_confirmed(),
            Some(80),
            Some(4.0),
            None,
            &mut Default::default(),
        );
        assert!(out.0.is_empty());
    }

    #[test]
    fn privacy_telemetry_flag_gates_everything() {
        let tc = telemetry_cfg();
        let mut privacy = privacy_all_confirmed();
        privacy.share_telemetry = false;
        let out = build_payload(
            &tc,
            &privacy,
            Some(80),
            Some(4.0),
            None,
            &mut Default::default(),
        );
        assert!(
            out.0.is_empty(),
            "share_telemetry=false must suppress all telemetry"
        );
    }

    #[test]
    fn battery_included_only_when_both_flags_on() {
        let tc = telemetry_cfg();
        let privacy = privacy_all_confirmed();
        let out = build_payload(
            &tc,
            &privacy,
            Some(82),
            Some(3.9),
            None,
            &mut Default::default(),
        );
        assert!(out.0.iter().any(|f| f.key == "battery" && f.value == "82%"));

        let mut privacy2 = privacy.clone();
        privacy2.share_battery = false;
        let out2 = build_payload(
            &tc,
            &privacy2,
            Some(82),
            Some(3.9),
            None,
            &mut Default::default(),
        );
        assert!(!out2.0.iter().any(|f| f.key == "battery"));
    }

    #[test]
    fn location_needs_prepared_share_input() {
        let tc = telemetry_cfg();
        let privacy = privacy_all_confirmed();
        let with_loc = build_payload(
            &tc,
            &privacy,
            None,
            None,
            Some(&sample_loc()),
            &mut Default::default(),
        );
        assert!(with_loc.0.iter().any(|f| f.key == "location"));
    }

    #[test]
    fn scheduler_due_logic() {
        assert!(!is_due(100, 200, 300));
        assert!(is_due(100, 400, 300));
        assert!(!is_due(0, 1000, 0), "interval 0 disables scheduling");
    }
}
