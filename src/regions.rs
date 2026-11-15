use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Bandwidth {
    Bw125,
    Bw250,
    Bw500,
}

impl Bandwidth {
    pub fn khz(self) -> u32 {
        match self {
            Bandwidth::Bw125 => 125,
            Bandwidth::Bw250 => 250,
            Bandwidth::Bw500 => 500,
        }
    }
    pub fn all() -> [Bandwidth; 3] {
        [Bandwidth::Bw125, Bandwidth::Bw250, Bandwidth::Bw500]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodingRate {
    Cr45,
    Cr46,
    Cr47,
    Cr48,
}

impl CodingRate {
    pub fn denom(self) -> u8 {
        match self {
            CodingRate::Cr45 => 5,
            CodingRate::Cr46 => 6,
            CodingRate::Cr47 => 7,
            CodingRate::Cr48 => 8,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            CodingRate::Cr45 => "4/5",
            CodingRate::Cr46 => "4/6",
            CodingRate::Cr47 => "4/7",
            CodingRate::Cr48 => "4/8",
        }
    }
    pub fn all() -> [CodingRate; 4] {
        [
            CodingRate::Cr45,
            CodingRate::Cr46,
            CodingRate::Cr47,
            CodingRate::Cr48,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Region {
    Us915,
    Eu868,
    Eu433,
    Cn779,
    Cn470,
    In865,
    Au915,
    Kr920,
    As923,
    Ru864,
}

impl Region {
    pub const ALL: [Region; 10] = [
        Region::Us915,
        Region::Eu868,
        Region::Eu433,
        Region::Cn779,
        Region::Cn470,
        Region::In865,
        Region::Au915,
        Region::Kr920,
        Region::As923,
        Region::Ru864,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Region::Us915 => "US915",
            Region::Eu868 => "EU868",
            Region::Eu433 => "EU433",
            Region::Cn779 => "CN779",
            Region::Cn470 => "CN470",
            Region::In865 => "IN865",
            Region::Au915 => "AU915",
            Region::Kr920 => "KR920",
            Region::As923 => "AS923",
            Region::Ru864 => "RU864",
        }
    }

    pub fn from_label(s: &str) -> Option<Region> {
        Region::ALL
            .iter()
            .copied()
            .find(|r| r.label().eq_ignore_ascii_case(s))
    }

    pub fn policy(self) -> RegionPolicy {
        match self {
            Region::Us915 => RegionPolicy {
                region: self,
                min_hz: 902_000_000,
                max_hz: 928_000_000,
                max_power_dbm: 30,
                duty_cycle_percent: None,
                note: "FCC Part 15.247 digital modulation. Power limit assumes compliant antenna setup; EIRP rules apply.",
            },
            Region::Au915 => RegionPolicy {
                region: self,
                min_hz: 915_000_000,
                max_hz: 928_000_000,
                max_power_dbm: 30,
                duty_cycle_percent: None,
                note: "AS/NZS 4268 band 915-928 MHz.",
            },
            Region::Eu868 => RegionPolicy {
                region: self,
                min_hz: 863_000_000,
                max_hz: 870_000_000,
                max_power_dbm: 16,
                duty_cycle_percent: Some(10.0),
                note: "ERC REC 70-03. Sub-band dependent ERP limits (25 mW typical) and duty cycles (0.1%-10%). Conservative 16 dBm cap.",
            },
            Region::Eu433 => RegionPolicy {
                region: self,
                min_hz: 433_050_000,
                max_hz: 434_790_000,
                max_power_dbm: 12,
                duty_cycle_percent: Some(10.0),
                note: "ERC REC 70-03 annex 1, band I/d. 10 mW ERP, 10% duty cycle.",
            },
            Region::Cn779 => RegionPolicy {
                region: self,
                min_hz: 779_000_000,
                max_hz: 787_000_000,
                max_power_dbm: 19,
                duty_cycle_percent: Some(1.0),
                note: "China 779-787 MHz ISM.",
            },
            Region::Cn470 => RegionPolicy {
                region: self,
                min_hz: 470_000_000,
                max_hz: 510_000_000,
                max_power_dbm: 17,
                duty_cycle_percent: None,
                note: "China 470-510 MHz.",
            },
            Region::In865 => RegionPolicy {
                region: self,
                min_hz: 865_000_000,
                max_hz: 867_000_000,
                max_power_dbm: 20,
                duty_cycle_percent: None,
                note: "WPC 865-867 MHz, max ERP 1W.",
            },
            Region::Kr920 => RegionPolicy {
                region: self,
                min_hz: 920_900_000,
                max_hz: 923_300_000,
                max_power_dbm: 14,
                duty_cycle_percent: None,
                note: "Korea 920.9-923.3 MHz.",
            },
            Region::As923 => RegionPolicy {
                region: self,
                min_hz: 915_000_000,
                max_hz: 928_000_000,
                max_power_dbm: 16,
                duty_cycle_percent: Some(1.0),
                note: "ASEAN 915-928 MHz varies by country; verify local regulation.",
            },
            Region::Ru864 => RegionPolicy {
                region: self,
                min_hz: 864_000_000,
                max_hz: 865_000_000,
                max_power_dbm: 16,
                duty_cycle_percent: Some(1.0),
                note: "Russia 864-865 MHz.",
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RegionPolicy {
    pub region: Region,
    pub min_hz: u64,
    pub max_hz: u64,
    pub max_power_dbm: i8,
    pub duty_cycle_percent: Option<f32>,
    pub note: &'static str,
}

pub const SX1262_MAX_POWER_DBM: i8 = 22;

#[derive(Debug, Clone, PartialEq)]
pub struct RadioSettings {
    pub region: Region,
    pub frequency_khz: f64,
    pub tx_power_dbm: i8,
    pub spreading_factor: u8,
    pub bandwidth: Bandwidth,
    pub coding_rate: CodingRate,
    pub preamble_len: u16,
    pub sync_word: u16,
    pub crc_on: bool,
    pub iq_inverted: bool,
    pub explicit_header: bool,
}

impl Default for RadioSettings {
    fn default() -> Self {
        Self {
            region: Region::Us915,
            frequency_khz: 915_000.0,
            tx_power_dbm: 14,
            spreading_factor: 7,
            bandwidth: Bandwidth::Bw125,
            coding_rate: CodingRate::Cr45,
            preamble_len: 8,
            sync_word: 0x1234,
            crc_on: true,
            iq_inverted: false,
            explicit_header: true,
        }
    }
}

pub fn validate(settings: &RadioSettings) -> crate::error::Result<RadioSettings> {
    let p = settings.region.policy();
    let hz = (settings.frequency_khz * 1000.0).round() as u64;
    if !(p.min_hz..=p.max_hz).contains(&hz) {
        return Err(crate::error::Error::IllegalRadioConfig {
            region: p.region.label().into(),
            reason: format!(
                "frequency {:.3} MHz outside legal band {:.3}-{:.3} MHz",
                hz as f64 / 1e6,
                p.min_hz as f64 / 1e6,
                p.max_hz as f64 / 1e6
            ),
        });
    }
    let eff_cap = p.max_power_dbm.min(SX1262_MAX_POWER_DBM);
    if settings.tx_power_dbm > eff_cap {
        return Err(crate::error::Error::IllegalRadioConfig {
            region: p.region.label().into(),
            reason: format!(
                "tx power {} dBm exceeds regional cap {} dBm (SX1262 max {} dBm)",
                settings.tx_power_dbm, eff_cap, SX1262_MAX_POWER_DBM
            ),
        });
    }
    if settings.tx_power_dbm < -9 {
        return Err(crate::error::Error::IllegalRadioConfig {
            region: p.region.label().into(),
            reason: "tx power below SX1262 minimum (-9 dBm)".into(),
        });
    }
    if !(5..=12).contains(&settings.spreading_factor) {
        return Err(crate::error::Error::IllegalRadioConfig {
            region: p.region.label().into(),
            reason: format!(
                "spreading factor {} outside SF5..SF12",
                settings.spreading_factor
            ),
        });
    }
    if !(6..=40).contains(&settings.preamble_len) {
        return Err(crate::error::Error::IllegalRadioConfig {
            region: p.region.label().into(),
            reason: format!("preamble {} outside 6..40", settings.preamble_len),
        });
    }
    Ok(settings.clone())
}

pub fn estimate_airtime_ms(s: &RadioSettings, payload_len: usize) -> f64 {
    let sf = s.spreading_factor as f64;
    let bw = s.bandwidth.khz() as f64 * 1000.0;
    let sym_time = (1u64 << s.spreading_factor) as f64 / bw;
    let de = if sf >= 11.0 && bw == 125_000.0 {
        1.0
    } else {
        0.0
    };
    let cr_denom = s.coding_rate.denom() as f64;
    let n_payload = 8.0
        + ((8.0 * payload_len as f64 - 4.0 * sf + 28.0 + 16.0
            - 20.0 * (if s.explicit_header { 1.0 } else { 0.0 }))
            / (4.0 * (sf - 2.0 * de)))
            .ceil()
            * (cr_denom - 4.0 + 4.0);
    let preamble_syms = s.preamble_len as f64 + 4.25;
    (preamble_syms + 8.0 + n_payload.max(0.0)) * sym_time * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn us_settings() -> RadioSettings {
        RadioSettings::default()
    }

    #[test]
    fn accepts_default_us915() {
        assert!(validate(&us_settings()).is_ok());
    }

    #[test]
    fn rejects_out_of_band_frequency() {
        let mut s = us_settings();
        s.frequency_khz = 868_000.0;
        let err = validate(&s).unwrap_err();
        assert!(matches!(
            err,
            crate::error::Error::IllegalRadioConfig { .. }
        ));
    }

    #[test]
    fn rejects_excess_power_for_region() {
        let mut s = us_settings();
        s.tx_power_dbm = 31;
        assert!(validate(&s).is_err());

        let mut e = us_settings();
        e.region = Region::Eu868;
        e.frequency_khz = 868_300.0;
        e.tx_power_dbm = 20;
        assert!(validate(&e).is_err());
        e.tx_power_dbm = 16;
        assert!(validate(&e).is_ok(), "EU868 allows up to conservative cap");
    }

    #[test]
    fn rejects_bad_sf_and_preamble() {
        let mut s = us_settings();
        s.spreading_factor = 13;
        assert!(validate(&s).is_err());
        s.spreading_factor = 7;
        s.preamble_len = 100;
        assert!(validate(&s).is_err());
    }

    #[test]
    fn sx1262_hardware_cap_applies_even_where_regulation_is_loose() {
        let mut s = us_settings();
        s.tx_power_dbm = 30;
        assert!(validate(&s).is_err());
    }

    #[test]
    fn airtime_positive_and_grows_with_sf() {
        let short = estimate_airtime_ms(&us_settings(), 20);
        assert!(short > 0.0);
        let mut slow = us_settings();
        slow.spreading_factor = 12;
        assert!(estimate_airtime_ms(&slow, 20) > short * 10.0);
    }
}
