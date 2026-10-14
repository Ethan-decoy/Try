use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopupCorner {
    #[default]
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
}

impl PopupCorner {
    pub fn index(self) -> i32 {
        match self {
            Self::BottomRight => 0,
            Self::BottomLeft => 1,
            Self::TopRight => 2,
            Self::TopLeft => 3,
        }
    }

    pub fn from_index(index: i32) -> Option<Self> {
        match index {
            0 => Some(Self::BottomRight),
            1 => Some(Self::BottomLeft),
            2 => Some(Self::TopRight),
            3 => Some(Self::TopLeft),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedPopupPosition {
    pub monitor: String,
    pub x: f64,
    pub y: f64,
}

impl SavedPopupPosition {
    pub fn is_valid(&self) -> bool {
        !self.monitor.is_empty()
            && self.monitor.len() <= 256
            && self.x.is_finite()
            && self.y.is_finite()
            && (0.0..=100_000.0).contains(&self.x)
            && (0.0..=100_000.0).contains(&self.y)
    }
}

/// Coordinates and dimensions are physical pixels; offsets and margins are logical.
#[derive(Clone, Debug)]
pub struct WorkArea {
    pub monitor: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

impl WorkArea {
    /// A tag faces the screen interior: left-hand cards use the right corner.
    pub fn tag_on_right(&self, position: (i32, i32), size: (u32, u32)) -> bool {
        let card_center = i64::from(position.0) * 2 + i64::from(size.0);
        let area_center = i64::from(self.x) * 2 + i64::from(self.width);
        card_center <= area_center
    }

    pub fn place(
        &self,
        size: (u32, u32),
        corner: PopupCorner,
        saved: Option<&SavedPopupPosition>,
    ) -> (i32, i32) {
        let margin = (24.0 * self.scale).round() as i32;
        let left = self.x + margin.min((self.width.saturating_sub(size.0) / 2) as i32);
        let top = self.y + margin.min((self.height.saturating_sub(size.1) / 2) as i32);
        let right = (self.x + self.width as i32 - size.0 as i32 - margin).max(left);
        let bottom = (self.y + self.height as i32 - size.1 as i32 - margin).max(top);
        if let Some(saved) = saved.filter(|saved| saved.monitor == self.monitor && saved.is_valid())
        {
            return (
                (self.x + (saved.x * self.scale).round() as i32).clamp(left, right),
                (self.y + (saved.y * self.scale).round() as i32).clamp(top, bottom),
            );
        }
        (
            if matches!(corner, PopupCorner::BottomRight | PopupCorner::TopRight) {
                right
            } else {
                left
            },
            if matches!(corner, PopupCorner::BottomRight | PopupCorner::BottomLeft) {
                bottom
            } else {
                top
            },
        )
    }

    pub fn remember(&self, position: (i32, i32)) -> SavedPopupPosition {
        SavedPopupPosition {
            monitor: self.monitor.clone(),
            x: (f64::from(position.0 - self.x) / self.scale).max(0.0),
            y: (f64::from(position.1 - self.y) / self.scale).max(0.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restores_logical_offsets_and_clamps_after_monitor_changes() {
        let area = WorkArea {
            monitor: "left_display".into(),
            x: -1920,
            y: 0,
            width: 1920,
            height: 1040,
            scale: 1.0,
        };
        assert_eq!(
            area.place((320, 230), PopupCorner::BottomRight, None),
            (-344, 786)
        );
        let saved = area.remember((-700, 550));
        let smaller = WorkArea {
            x: -1280,
            width: 1280,
            height: 700,
            scale: 1.5,
            ..area
        };
        assert_eq!(
            smaller.place((480, 345), PopupCorner::BottomRight, Some(&saved)),
            (-516, 319)
        );
        let missing = SavedPopupPosition {
            monitor: "removed_display".into(),
            ..saved
        };
        assert_eq!(
            smaller.place((480, 345), PopupCorner::TopLeft, Some(&missing)),
            (-1244, 36)
        );
    }

    #[test]
    fn tags_face_inward_on_negative_coordinates_and_scaled_displays() {
        let area = WorkArea {
            monitor: "left_display".into(),
            x: -1920,
            y: 0,
            width: 1920,
            height: 1040,
            scale: 1.5,
        };
        assert!(area.tag_on_right((-1884, 36), (480, 384)));
        assert!(!area.tag_on_right((-516, 620), (480, 384)));
        assert!(area.tag_on_right((-1200, 100), (480, 384)));
        assert!(!area.tag_on_right((-1199, 100), (480, 384)));
        let second = WorkArea { x: 0, ..area };
        assert!(second.tag_on_right((36, 100), (480, 384)));
    }
}
