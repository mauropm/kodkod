use crate::error::Result;
use crate::privacy::{Coordinates, PrivacySettings};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GpsFix {
    pub lat: f64,
    pub lon: f64,
    pub altitude_m: Option<f32>,
    pub speed_mps: Option<f32>,
    pub heading_deg: Option<f32>,
    pub sats: Option<u8>,
    pub fix_time: u32,
}

impl GpsFix {
    pub fn coords(&self) -> Coordinates {
        Coordinates {
            lat: self.lat,
            lon: self.lon,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum GpsStatus {
    Off,
    NotDetected,
    NoFix { sats: Option<u8> },
    Fix(GpsFix),
}

impl GpsStatus {
    pub fn label(&self) -> &'static str {
        match self {
            GpsStatus::Off => "OFF",
            GpsStatus::NotDetected => "NOT DETECTED",
            GpsStatus::NoFix { .. } => "NO FIX",
            GpsStatus::Fix(_) => "SHARING",
        }
    }
}

pub trait LocationSource: Send {
    fn status(&mut self) -> GpsStatus;

    fn fix_if_enabled(&mut self, gps_enabled: bool) -> GpsStatus {
        if !gps_enabled {
            return GpsStatus::Off;
        }
        self.status()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SharedLocation {
    pub lat: f64,
    pub lon: f64,
    pub altitude_m: Option<f32>,
    pub speed_mps: Option<f32>,
    pub heading_deg: Option<f32>,
    pub include_timestamp: bool,
    pub precision_label: String,
}

pub fn prepare_share(
    privacy: &PrivacySettings,
    fix: &GpsFix,
    audience_check: impl FnOnce(&crate::privacy::Audience) -> bool,
) -> Result<SharedLocation> {
    privacy.can_share_location_with(audience_check)?;
    let precision = privacy.location_precision;
    let reduced = precision.reduce(fix.coords());
    Ok(SharedLocation {
        lat: reduced.lat,
        lon: reduced.lon,
        altitude_m: if privacy.share_altitude {
            fix.altitude_m
        } else {
            None
        },
        speed_mps: if privacy.share_speed {
            fix.speed_mps
        } else {
            None
        },
        heading_deg: if privacy.share_heading {
            fix.heading_deg
        } else {
            None
        },
        include_timestamp: privacy.include_timestamp,
        precision_label: precision.label().into(),
    })
}

pub struct DisabledProvider;

impl LocationSource for DisabledProvider {
    fn status(&mut self) -> GpsStatus {
        GpsStatus::NotDetected
    }
}

#[derive(Default)]
pub struct MockProvider {
    t: u32,
}

impl LocationSource for MockProvider {
    fn status(&mut self) -> GpsStatus {
        self.t += 1;
        if self.t < 2 {
            GpsStatus::NoFix { sats: Some(0) }
        } else {
            GpsStatus::Fix(GpsFix {
                lat: -33.865143,
                lon: 151.209900,
                altitude_m: Some(58.0),
                speed_mps: Some(0.4),
                heading_deg: Some(180.0),
                sats: Some(9),
                fix_time: crate::protocol::now_secs(),
            })
        }
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::privacy::{Audience, GpsPrecision};

    fn privacy_sharing_everyone_confirmed(precision: GpsPrecision) -> PrivacySettings {
        let mut p = PrivacySettings::default();
        p.share_location = true;
        p.location_audience = Audience::Everyone;
        p.confirm_everyone();
        p.location_precision = precision;
        p
    }

    fn sample_fix() -> GpsFix {
        GpsFix {
            lat: -33.865143,
            lon: 151.209900,
            altitude_m: Some(58.0),
            speed_mps: Some(1.2),
            heading_deg: Some(90.0),
            sats: Some(8),
            fix_time: 1000,
        }
    }

    #[test]
    fn share_blocked_by_default_privacy() {
        let p = PrivacySettings::default();
        assert!(prepare_share(&p, &sample_fix(), |_| true).is_err());
    }

    #[test]
    fn share_respects_precision_and_field_flags() {
        let mut p = privacy_sharing_everyone_confirmed(GpsPrecision::Km1);
        let out = prepare_share(&p, &sample_fix(), |_| true).expect("ok");
        assert_eq!(out.precision_label, "1 km");
        assert!(out.altitude_m.is_none(), "altitude off by default");

        p.share_altitude = true;
        p.share_speed = true;
        p.include_timestamp = true;
        let out2 = prepare_share(&p, &sample_fix(), |_| true).expect("ok");
        assert_eq!(out2.altitude_m, Some(58.0));
        assert_eq!(out2.speed_mps, Some(1.2));
        assert!(out2.include_timestamp);
    }

    #[test]
    fn share_denied_for_outside_audience() {
        let p = privacy_sharing_everyone_confirmed(GpsPrecision::Exact);
        let res = prepare_share(
            &p,
            &sample_fix(),
            |aud| matches!(aud, Audience::Contacts(c) if c.contains(&"EVE".into())),
        );
        assert!(res.is_err());
    }

    #[test]
    fn exact_precision_preserves_coordinates() {
        let p = privacy_sharing_everyone_confirmed(GpsPrecision::Exact);
        let out = prepare_share(&p, &sample_fix(), |_| true).expect("ok");
        assert!((out.lat - sample_fix().lat).abs() < 1e-9);
    }

    #[test]
    fn disabled_provider_reports_not_detected() {
        let mut src = DisabledProvider;
        assert_eq!(src.status(), GpsStatus::NotDetected);
    }

    #[test]
    fn disabled_gps_flag_returns_off_without_querying() {
        let mut src = MockProvider::default();
        assert_eq!(src.fix_if_enabled(false), GpsStatus::Off);
    }
}
