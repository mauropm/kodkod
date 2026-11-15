use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GpsPrecision {
    #[default]
    Exact,
    M10,
    M100,
    Km1,
    Km10,
    City,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Coordinates {
    pub lat: f64,
    pub lon: f64,
}

impl GpsPrecision {
    pub fn label(self) -> &'static str {
        match self {
            GpsPrecision::Exact => "exact",
            GpsPrecision::M10 => "10 m",
            GpsPrecision::M100 => "100 m",
            GpsPrecision::Km1 => "1 km",
            GpsPrecision::Km10 => "10 km",
            GpsPrecision::City => "city",
        }
    }

    fn grid_degrees(self) -> f64 {
        match self {
            GpsPrecision::Exact | GpsPrecision::M10 => 0.0,
            GpsPrecision::M100 => 100.0 / 111_320.0,
            GpsPrecision::Km1 => 1000.0 / 111_320.0,
            GpsPrecision::Km10 => 10_000.0 / 111_320.0,
            GpsPrecision::City => 0.05,
        }
    }

    pub fn reduce(self, c: Coordinates) -> Coordinates {
        let grid = self.grid_degrees();
        if grid == 0.0 {
            if self == GpsPrecision::Exact {
                c
            } else {
                round_to(c, 1.0 / 111_320.0 * 10.0)
            }
        } else {
            Coordinates {
                lat: (c.lat / grid).floor() * grid + grid / 2.0,
                lon: (c.lon / grid).floor() * grid + grid / 2.0,
            }
        }
    }
}

fn round_to(c: Coordinates, step: f64) -> Coordinates {
    Coordinates {
        lat: (c.lat / step).round() * step,
        lon: (c.lon / step).round() * step,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Audience {
    #[default]
    Nobody,
    Contacts(Vec<String>),
    Channels(Vec<String>),
    TrustedNodes,
    Everyone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacySettings {
    pub share_location: bool,
    pub share_approx_location_only: bool,
    pub location_precision: GpsPrecision,
    pub share_altitude: bool,
    pub share_speed: bool,
    pub share_heading: bool,
    pub include_timestamp: bool,
    pub location_audience: Audience,
    pub location_global_confirmations: u32,
    pub share_battery: bool,
    pub share_telemetry: bool,
    pub share_presence: bool,
    pub share_last_seen: bool,
    pub share_signal_stats: bool,
    pub share_hardware_info: bool,
    pub respond_to_discovery: bool,
    pub emergency_overrides_privacy: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            share_location: false,
            share_approx_location_only: true,
            location_precision: GpsPrecision::Km1,
            share_altitude: false,
            share_speed: false,
            share_heading: false,
            include_timestamp: false,
            location_audience: Audience::Nobody,
            location_global_confirmations: 0,
            share_battery: false,
            share_telemetry: false,
            share_presence: false,
            share_last_seen: false,
            share_signal_stats: true,
            share_hardware_info: false,
            respond_to_discovery: true,
            emergency_overrides_privacy: false,
        }
    }
}

impl PrivacySettings {
    pub fn can_share_location_with(
        &self,
        audience_check: impl FnOnce(&Audience) -> bool,
    ) -> Result<()> {
        if !self.share_location {
            return Err(Error::PrivacyBlocked("location sharing disabled".into()));
        }
        match &self.location_audience {
            Audience::Nobody => Err(Error::PrivacyBlocked("location audience is Nobody".into())),
            Audience::Everyone if self.location_global_confirmations == 0 => {
                Err(Error::PrivacyBlocked(
                    "sharing with Everyone requires explicit confirmation".into(),
                ))
            }
            aud if audience_check(aud) => Ok(()),
            _ => Err(Error::PrivacyBlocked(
                "recipient not in allowed audience".into(),
            )),
        }
    }

    pub fn confirm_everyone(&mut self) {
        self.location_global_confirmations += 1;
    }

    pub fn revoke_everyone(&mut self) {
        self.location_audience = Audience::Nobody;
        self.location_global_confirmations = 0;
    }

    pub fn status_line(&self) -> String {
        format!(
            "GPS: {} | Telemetry: {} | Presence: {} | Battery: {}",
            if self.share_location {
                "SHARING"
            } else {
                "OFF"
            },
            on_off(self.share_telemetry),
            on_off(self.share_presence),
            on_off(self.share_battery),
        )
    }
}

fn on_off(b: bool) -> &'static str {
    if b {
        "ON"
    } else {
        "OFF"
    }
}

#[cfg(test)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;

    #[test]
    fn defaults_block_location() {
        let p = PrivacySettings::default();
        assert!(p.can_share_location_with(|_| true).is_err());
        assert!(p.status_line().contains("GPS: OFF"));
    }

    #[test]
    fn nobody_audience_blocks_even_when_enabled() {
        let mut p = PrivacySettings::default();
        p.share_location = true;
        assert!(p.can_share_location_with(|_| true).is_err());
    }

    #[test]
    fn everyone_requires_confirmation() {
        let mut p = PrivacySettings::default();
        p.share_location = true;
        p.location_audience = Audience::Everyone;
        assert!(p.can_share_location_with(|_| true).is_err());
        p.confirm_everyone();
        assert!(p.can_share_location_with(|_| true).is_ok());
    }

    #[test]
    fn contact_allowlist_enforced() {
        let mut p = PrivacySettings::default();
        p.share_location = true;
        p.location_audience = Audience::Contacts(vec!["BETA".into()]);
        assert!(p
            .can_share_location_with(
                |a| matches!(a, Audience::Contacts(c) if c.contains(&"BETA".to_string()))
            )
            .is_ok());
        assert!(p
            .can_share_location_with(
                |a| matches!(a, Audience::Contacts(c) if c.contains(&"EVE".to_string()))
            )
            .is_err());
    }

    #[test]
    fn revoke_resets_confirmation() {
        let mut p = PrivacySettings::default();
        p.share_location = true;
        p.location_audience = Audience::Everyone;
        p.confirm_everyone();
        p.revoke_everyone();
        assert!(p.can_share_location_with(|_| true).is_err());
        assert_eq!(p.location_audience, Audience::Nobody);
    }

    fn coord(lat: f64, lon: f64) -> Coordinates {
        Coordinates { lat, lon }
    }

    #[test]
    fn exact_precision_unchanged() {
        let c = coord(52.5200066, 13.404954);
        assert_eq!(GpsPrecision::Exact.reduce(c), c);
    }

    #[test]
    fn m10_snaps_but_stays_close() {
        let c = coord(52.5200066, 13.404954);
        let r = GpsPrecision::M10.reduce(c);
        assert!((r.lat - c.lat).abs() < 0.0002, "{}", r.lat - c.lat);
        assert_ne!(r.lat, c.lat, "should be quantized");
    }

    #[test]
    fn km_grid_reduces_resolution() {
        let c = coord(52.5200066, 13.404954);
        for prec in [GpsPrecision::Km1, GpsPrecision::Km10, GpsPrecision::City] {
            let r = prec.reduce(c);
            let grid = match prec {
                GpsPrecision::Km1 => 1000.0 / 111_320.0,
                GpsPrecision::Km10 => 10_000.0 / 111_320.0,
                GpsPrecision::City => 0.05,
                _ => unreachable!(),
            };
            let dlat = (r.lat - c.lat).abs();
            let dlon = (r.lon - c.lon).abs();
            assert!(dlat <= grid, "{prec:?} lat moved more than one grid cell");
            assert!(dlon <= grid, "{prec:?} lon moved more than one grid cell");
        }
    }

    #[test]
    fn same_cell_maps_identically() {
        let a = coord(52.52101, 13.40501);
        let b = coord(52.52149, 13.40599);
        assert_eq!(GpsPrecision::Km1.reduce(a), GpsPrecision::Km1.reduce(b));
    }
}
