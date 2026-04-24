use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserViewport {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor: f32,
    pub scroll_x: f64,
    pub scroll_y: f64,
}

impl Default for BrowserViewport {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            device_scale_factor: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }
}

impl BrowserViewport {
    pub fn logical_to_physical_point(&self, point: BrowserPoint) -> BrowserPhysicalPoint {
        BrowserPhysicalPoint {
            x: scale_coordinate(point.x, self.device_scale_factor),
            y: scale_coordinate(point.y, self.device_scale_factor),
        }
    }

    pub fn physical_to_logical_point(&self, point: BrowserPhysicalPoint) -> BrowserPoint {
        let scale = self.device_scale_factor.max(f32::EPSILON) as f64;
        BrowserPoint {
            x: point.x as f64 / scale,
            y: point.y as f64 / scale,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserPhysicalPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserElementRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl BrowserElementRect {
    pub fn center(&self) -> BrowserPoint {
        BrowserPoint {
            x: self.x + self.width / 2.0,
            y: self.y + self.height / 2.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserElement {
    pub index: u32,
    pub role: String,
    pub label: String,
    pub text: String,
    pub rect: BrowserElementRect,
    pub selector: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSnapshot {
    pub epoch: u64,
    pub url: Option<String>,
    pub title: Option<String>,
    pub viewport: BrowserViewport,
    pub elements: Vec<BrowserElement>,
}

impl BrowserSnapshot {
    pub fn empty(epoch: u64) -> Self {
        Self {
            epoch,
            url: None,
            title: None,
            viewport: BrowserViewport::default(),
            elements: Vec::new(),
        }
    }

    pub fn element(&self, index: u32) -> Option<&BrowserElement> {
        self.elements.iter().find(|element| element.index == index)
    }
}

fn scale_coordinate(value: f64, device_scale_factor: f32) -> i32 {
    let scaled = value * device_scale_factor.max(f32::EPSILON) as f64;
    scaled.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32
}
