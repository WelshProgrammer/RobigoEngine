//! Zoom factors and the filters that scale a frame to the output.

use std::fmt;
use std::str::FromStr;

/// A zoom factor in quarter steps: 4 quarters draws one world pixel per output pixel.
///
/// A rational factor keeps the world target's size and the output-to-world pixel mapping exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Zoom(u8);

impl Zoom {
    /// One world pixel per output pixel.
    pub const ONE: Self = Self(4);

    /// A zoom of `quarters` / 4, or `None` for zero.
    #[must_use]
    pub const fn from_quarters(quarters: u8) -> Option<Self> {
        if quarters == 0 { None } else { Some(Self(quarters)) }
    }

    /// The factor in quarters.
    #[must_use]
    pub const fn quarters(self) -> u8 {
        self.0
    }

    /// The factor: output pixels per world pixel.
    #[must_use]
    pub fn factor(self) -> f32 {
        f32::from(self.0) / 4.0
    }

    /// The size of the world area an output of `width` by `height` pixels shows, rounded up so
    /// it always covers the output.
    #[must_use]
    pub fn world_size(self, width: u32, height: u32) -> (u32, u32) {
        let world = |out: u32| {
            let quarters = u64::from(self.0);
            u32::try_from((u64::from(out) * 4).div_ceil(quarters)).unwrap_or(u32::MAX)
        };
        (world(width), world(height))
    }

    /// The filter that scales the resolved world to the output at this zoom.
    #[must_use]
    pub const fn filter(self) -> ScaleFilter {
        match self.0 {
            q if q < 4 => ScaleFilter::Area,
            q if q % 4 == 0 => ScaleFilter::Nearest,
            _ => ScaleFilter::SharpBilinear,
        }
    }
}

impl Default for Zoom {
    fn default() -> Self {
        Self::ONE
    }
}

impl fmt::Display for Zoom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / 4;
        match self.0 % 4 {
            0 => write!(f, "{whole}"),
            1 => write!(f, "{whole}.25"),
            2 => write!(f, "{whole}.5"),
            _ => write!(f, "{whole}.75"),
        }
    }
}

/// The text is not a positive multiple of 0.25 up to 63.75.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("zoom '{0}' is not a positive multiple of 0.25 up to 63.75")]
pub struct BadZoom(String);

impl FromStr for Zoom {
    type Err = BadZoom;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || BadZoom(s.to_owned());
        let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
        let whole: u8 = if whole.is_empty() {
            0
        } else {
            whole.parse().map_err(|_| bad())?
        };
        let quarter = match fraction.trim_end_matches('0') {
            "" => 0,
            "25" => 1,
            "5" => 2,
            "75" => 3,
            _ => return Err(bad()),
        };
        whole
            .checked_mul(4)
            .and_then(|q| q.checked_add(quarter))
            .and_then(Self::from_quarters)
            .ok_or_else(bad)
    }
}

/// How the resolved world is scaled to the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScaleFilter {
    /// Each output pixel takes the world pixel under its centre; exact at whole zooms.
    Nearest,
    /// Nearest within each world pixel, blended across the one-pixel seams between them.
    SharpBilinear,
    /// Each output pixel averages the world pixels it covers, weighted by area.
    Area,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_parses_quarters() {
        assert_eq!("1".parse(), Ok(Zoom::ONE));
        assert_eq!("0.5".parse::<Zoom>().map(Zoom::quarters), Ok(2));
        assert_eq!(".75".parse::<Zoom>().map(Zoom::quarters), Ok(3));
        assert_eq!("2.50".parse::<Zoom>().map(Zoom::quarters), Ok(10));
        assert!("0".parse::<Zoom>().is_err());
        assert!("0.3".parse::<Zoom>().is_err());
        assert!("64".parse::<Zoom>().is_err());
        assert_eq!(Zoom::from_quarters(6).unwrap().to_string(), "1.5");
        assert_eq!(Zoom::from_quarters(1).unwrap().to_string(), "0.25");
    }

    #[test]
    fn world_size_covers_the_output() {
        assert_eq!(Zoom::ONE.world_size(640, 480), (640, 480));
        assert_eq!(Zoom::from_quarters(8).unwrap().world_size(641, 480), (321, 240));
        assert_eq!(Zoom::from_quarters(2).unwrap().world_size(640, 480), (1280, 960));
        assert_eq!(Zoom::from_quarters(3).unwrap().world_size(10, 10), (14, 14));
    }

    #[test]
    fn filters_follow_the_zoom() {
        let filter = |q| Zoom::from_quarters(q).unwrap().filter();
        assert_eq!(filter(4), ScaleFilter::Nearest);
        assert_eq!(filter(12), ScaleFilter::Nearest);
        assert_eq!(filter(6), ScaleFilter::SharpBilinear);
        assert_eq!(filter(2), ScaleFilter::Area);
    }
}
