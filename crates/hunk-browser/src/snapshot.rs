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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserElementRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserElement {
    pub index: u32,
    pub role: String,
    pub label: String,
    pub text: String,
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
