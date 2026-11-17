//! Physical-pixel desktop fitting. No GPU-vendor policy or persisted display state.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Geometry {
    pub work: Rect,
    pub dpi: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub client_width: u32,
    pub client_height: u32,
    pub outer: Rect,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Mode {
    pub width: u32,
    pub height: u32,
    pub refresh: f64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Fit {
    pub mode: Mode,
    pub windows_scale: f64,
    pub guest_scale: f64,
    pub left: i32,
    pub top: i32,
}
// Only select common modes actually advertised by the current Guest output.
const COMMON: &[(u32, u32)] = &[
    (7680, 4320),
    (5120, 2880),
    (5120, 2160),
    (3840, 2160),
    (3440, 1440),
    (2560, 1600),
    (2560, 1440),
    (2560, 1080),
    (2048, 1152),
    (1920, 1440),
    (1920, 1200),
    (1920, 1080),
    (1680, 1050),
    (1600, 1200),
    (1600, 900),
    (1440, 900),
    (1400, 1050),
    (1366, 768),
    (1360, 768),
    (1280, 1024),
    (1280, 960),
    (1280, 800),
    (1280, 768),
    (1280, 720),
    (1024, 768),
    (848, 480),
    (800, 600),
    (640, 480),
];

pub fn choose(g: &Geometry, modes: &[Mode]) -> Result<Fit, String> {
    let width = i64::from(g.work.right) - i64::from(g.work.left) - i64::from(g.frame_width);
    let height = i64::from(g.work.bottom) - i64::from(g.work.top) - i64::from(g.frame_height);
    if !(48..=768).contains(&g.dpi) || width <= 0 || height <= 0 || width > 32767 || height > 32767
    {
        return Err("Invalid Windows work area, frame metrics or DPI".into());
    }
    let mode = modes
        .iter()
        .filter(|m| {
            COMMON.contains(&(m.width, m.height))
                && i64::from(m.width) <= width
                && i64::from(m.height) <= height
                && m.refresh.is_finite()
                && (55.0..=61.0).contains(&m.refresh)
        })
        .max_by(|a, b| {
            (u64::from(a.width) * u64::from(a.height), a.width)
                .cmp(&(u64::from(b.width) * u64::from(b.height), b.width))
                .then_with(|| a.refresh.total_cmp(&b.refresh))
        })
        .ok_or("No advertised common desktop resolution fits the Windows work area")?
        .clone();
    let windows_scale = f64::from(g.dpi) / 96.0;
    // The pinned Hyprland compositor requires integral logical extents and uses
    // 1/120 scale steps. Choose a clean divisor before submitting its monitor rule.
    let target = (windows_scale * 120.0).round() as u32;
    let step = (60..=960u32)
        .filter(|s| (mode.width * 120) % s == 0 && (mode.height * 120) % s == 0)
        .min_by_key(|s| (s.abs_diff(target), std::cmp::Reverse(*s)))
        .ok_or("No integral compositor scale for the selected mode")?;
    Ok(Fit {
        left: g.work.left + ((width - i64::from(mode.width)) / 2) as i32,
        top: g.work.top + ((height - i64::from(mode.height)) / 2) as i32,
        mode,
        windows_scale,
        guest_scale: f64::from(step) / 120.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn geometry(w: i32, h: i32, dpi: u32) -> Geometry {
        Geometry {
            work: Rect {
                left: 0,
                top: 0,
                right: w,
                bottom: h,
            },
            dpi,
            frame_width: 16,
            frame_height: 39,
            client_width: 784,
            client_height: 441,
            outer: Rect {
                left: 0,
                top: 0,
                right: 800,
                bottom: 480,
            },
        }
    }
    fn modes() -> Vec<Mode> {
        COMMON
            .iter()
            .map(|&(width, height)| Mode {
                width,
                height,
                refresh: 60.0,
            })
            .collect()
    }
    #[test]
    fn largest_advertised_mode_fits_frame_and_taskbar_without_dpi_double_scaling() {
        for dpi in [96, 120, 144, 168, 192] {
            let f = choose(&geometry(1920, 1040, dpi), &modes()).unwrap();
            assert_eq!((f.mode.width, f.mode.height), (1600, 900));
            assert_eq!(f.windows_scale, f64::from(dpi) / 96.0);
            assert!((f.mode.width as f64 / f.guest_scale).fract().abs() < 0.00001);
        }
        let f = choose(&geometry(3840, 2080, 192), &modes()).unwrap();
        assert_eq!((f.mode.width, f.mode.height), (3440, 1440));
        assert_eq!(f.guest_scale, 2.0);
    }
    #[test]
    fn negative_monitor_coordinates_and_portrait_stay_inside_work_area() {
        let mut g = geometry(1080, 1880, 144);
        g.work = Rect {
            left: -1080,
            top: -200,
            right: 0,
            bottom: 1680,
        };
        let f = choose(&g, &modes()).unwrap();
        assert_eq!((f.mode.width, f.mode.height), (1024, 768));
        assert!(f.left >= g.work.left && f.left + f.mode.width as i32 + 16 <= g.work.right);
        assert!(f.top >= g.work.top && f.top + f.mode.height as i32 + 39 <= g.work.bottom);
    }
    #[test]
    fn unsupported_modes_and_insufficient_space_do_not_fabricate_success() {
        assert!(choose(&geometry(600, 400, 96), &modes()).is_err());
        assert!(choose(&geometry(1920, 1040, 0), &modes()).is_err());
        assert!(choose(&geometry(1920, 1040, 96), &[]).is_err());
        let limited = vec![Mode {
            width: 1280,
            height: 720,
            refresh: 59.94,
        }];
        assert_eq!(
            choose(&geometry(1920, 1040, 96), &limited)
                .unwrap()
                .mode
                .width,
            1280
        );
    }
}
