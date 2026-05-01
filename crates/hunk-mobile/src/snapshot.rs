use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileViewport {
    pub width: u32,
    pub height: u32,
}

impl Default for MobileViewport {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobilePoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileElementRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl MobileElementRect {
    pub fn center(&self) -> MobilePoint {
        MobilePoint {
            x: self.x + (self.width / 2) as i32,
            y: self.y + (self.height / 2) as i32,
        }
    }

    pub fn max_x(&self) -> i32 {
        self.x.saturating_add(self.width as i32)
    }

    pub fn max_y(&self) -> i32 {
        self.y.saturating_add(self.height as i32)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileElement {
    pub index: u32,
    pub role: String,
    pub label: String,
    pub text: String,
    pub rect: MobileElementRect,
    pub enabled: bool,
    pub clickable: bool,
    pub focusable: bool,
    pub focused: bool,
    pub scrollable: bool,
    pub selected: bool,
    pub checked: bool,
    pub resource_id: Option<String>,
    pub class_name: Option<String>,
    pub package_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileSnapshot {
    pub epoch: u64,
    pub device_id: Option<String>,
    pub viewport: MobileViewport,
    pub elements: Vec<MobileElement>,
}

impl MobileSnapshot {
    pub fn empty(epoch: u64) -> Self {
        Self {
            epoch,
            device_id: None,
            viewport: MobileViewport::default(),
            elements: Vec::new(),
        }
    }

    pub fn element(&self, index: u32) -> Option<&MobileElement> {
        self.elements.iter().find(|element| element.index == index)
    }
}
