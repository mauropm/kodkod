use crate::device::types::{CapabilityState, SensorDesc};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Temperature,
    Humidity,
    Pressure,
    AirQuality,
    Acceleration,
    Motion,
    Light,
    Battery,
    Gnss,
    Gpio,
    Adc,
    Nfc,
}

impl Capability {
    pub fn label(self) -> &'static str {
        match self {
            Capability::Temperature => "temperature",
            Capability::Humidity => "humidity",
            Capability::Pressure => "pressure",
            Capability::AirQuality => "air quality",
            Capability::Acceleration => "acceleration",
            Capability::Motion => "motion",
            Capability::Light => "light",
            Capability::Battery => "battery",
            Capability::Gnss => "gnss",
            Capability::Gpio => "gpio",
            Capability::Adc => "adc",
            Capability::Nfc => "nfc",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SensorReading {
    pub sensor_id: String,
    pub capability: Capability,
    pub value: f64,
    pub unit: &'static str,
}

pub trait Sensor: Send {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn capabilities(&self) -> Vec<Capability>;
    fn read(&mut self) -> crate::error::Result<Vec<SensorReading>>;
}

#[derive(Default)]
pub struct MockEnvironmentSensor {
    tick: u8,
}

impl Sensor for MockEnvironmentSensor {
    fn id(&self) -> &str {
        "rak1906-mock"
    }

    fn name(&self) -> &str {
        "RAK1906 environment (mock)"
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Temperature,
            Capability::Humidity,
            Capability::Pressure,
        ]
    }

    fn read(&mut self) -> crate::error::Result<Vec<SensorReading>> {
        self.tick = self.tick.wrapping_add(1);
        let t = 21.0 + (self.tick % 10) as f64 * 0.1;
        Ok(vec![
            SensorReading {
                sensor_id: self.id().into(),
                capability: Capability::Temperature,
                value: t,
                unit: "°C",
            },
            SensorReading {
                sensor_id: self.id().into(),
                capability: Capability::Humidity,
                value: 45.0,
                unit: "%",
            },
            SensorReading {
                sensor_id: self.id().into(),
                capability: Capability::Pressure,
                value: 1013.2,
                unit: "hPa",
            },
        ])
    }
}

pub struct NotDetectedPlaceholder {
    desc: SensorDesc,
}

impl NotDetectedPlaceholder {
    pub fn from_desc(desc: SensorDesc) -> Self {
        Self { desc }
    }
    pub fn desc(&self) -> &SensorDesc {
        &self.desc
    }
}

impl Sensor for NotDetectedPlaceholder {
    fn id(&self) -> &str {
        &self.desc.id
    }
    fn name(&self) -> &str {
        &self.desc.name
    }
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
    fn read(&mut self) -> crate::error::Result<Vec<SensorReading>> {
        Err(crate::error::Error::NotDetected(self.desc.name.clone()))
    }
}

#[derive(Default)]
pub struct SensorRegistry {
    sensors: Vec<Box<dyn Sensor>>,
}

impl SensorRegistry {
    pub fn register(&mut self, s: Box<dyn Sensor>) {
        self.sensors.push(s);
    }

    pub fn len(&self) -> usize {
        self.sensors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sensors.is_empty()
    }

    pub fn scan_all(&mut self) -> Vec<(String, crate::error::Result<Vec<SensorReading>>)> {
        self.sensors
            .iter_mut()
            .map(|s| (format!("{} ({})", s.name(), s.id()), s.read()))
            .collect()
    }
}

pub fn placeholders_from_capabilities(caps: &[SensorDesc]) -> Vec<NotDetectedPlaceholder> {
    caps.iter()
        .filter(|d| {
            matches!(
                d.state,
                CapabilityState::NotDetected | CapabilityState::RequiresOptionalModule
            )
        })
        .map(|d| NotDetectedPlaceholder::from_desc(d.clone()))
        .collect()
}
