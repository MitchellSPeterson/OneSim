//! Device profiles: screen geometry and chrome, shared by the native host and the WebAssembly build.

use simcore_protocol::Viewport;

/// Status bar placement (points, in a screen of the device's own width).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatusBar {
    /// Centre of the clock text.
    pub time: (f64, f64),
    /// Clock font size (17 on the 17 Pro; smaller on notch phones).
    pub font: f64,
    /// Trailing anchor x and y shift of the signal/wifi/battery cluster.
    pub icons: (f64, f64),
    /// Glyph scale of the cluster relative to the 17 Pro (offsets scale with it).
    pub icon_scale: f64,
}

/// Screen geometry and chrome of a simulated device (points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Device {
    /// Profile name, as accepted by `--device` / `?device=`.
    pub name: &'static str,
    /// Native screen size.
    pub viewport: Viewport,
    /// Device pixels per point.
    pub scale: f64,
    /// Top safe-area inset (also the status bar height).
    pub safe_top: f64,
    /// Bottom safe-area inset (home indicator).
    pub safe_bottom: f64,
    /// Dynamic Island `(x, y, w, h)`.
    pub island: Option<(f64, f64, f64, f64)>,
    /// Status bar clock and icon placement.
    pub status_bar: StatusBar,
    /// Home-button phone (iPhone SE): classic status bar, square screen, icons in the bottom key row.
    pub home_button: bool,
    /// Keyboard shows the static "I / The / I'm" suggestions (only the 17 Pro reference was captured with words).
    pub suggestions: bool,
    /// Distance between key rows (cap height is this minus 11.1).
    pub key_pitch: f64,
    /// Keyboard height above the bottom safe inset (measured 335 total with a 34pt inset).
    pub kb_base_h: f64,
}

pub const DEFAULT_DEVICE: &str = "iphone-17-pro";

const fn sb(time: (f64, f64), font: f64, icons: (f64, f64), icon_scale: f64) -> StatusBar {
    StatusBar { time, font, icons, icon_scale }
}
const fn vp(width: f64, height: f64) -> Viewport {
    Viewport { width, height }
}

// One const per geometry class; models that render pixel-identically share it (see tests/fixtures/devices).
const C_17_PRO: Device = Device {
    name: DEFAULT_DEVICE,
    viewport: vp(402.0, 874.0),
    scale: 3.0,
    safe_top: 62.0,
    safe_bottom: 34.0,
    // Measured from simulator screenshots: x 138.3..263.7, y 14..50.7.
    island: Some((138.4, 13.92, 125.2, 36.83)),
    status_bar: sb((73.0, 32.4), 17.0, (340.0, 0.0), 1.0),
    home_button: false,
    suggestions: true,
    key_pitch: 53.9,
    kb_base_h: 301.0,
};
const C_16: Device = Device {
    suggestions: false,
    viewport: vp(393.0, 852.0),
    safe_top: 59.0,
    island: Some((134.0, 11.33, 125.0, 36.67)),
    status_bar: sb((70.7, 29.1), 17.0, (333.7, -3.33), 1.0),
    ..C_17_PRO
};
const C_16_PLUS: Device = Device {
    suggestions: false,
    viewport: vp(430.0, 932.0),
    safe_top: 59.0,
    island: Some((152.33, 11.33, 125.33, 36.67)),
    status_bar: sb((79.5, 29.0), 18.0, (362.9, -4.66), 1.13),
    key_pitch: 55.9,
    kb_base_h: 311.0,
    ..C_17_PRO
};
const C_17_PRO_MAX: Device = Device {
    viewport: vp(440.0, 956.0),
    island: Some((157.33, 14.0, 125.33, 36.67)),
    status_bar: sb((82.5, 32.4), 18.0, (369.9, -1.33), 1.13),
    safe_top: 62.0,
    ..C_16_PLUS
};
const C_AIR: Device = Device {
    viewport: vp(420.0, 912.0),
    safe_top: 68.0,
    island: Some((147.33, 20.0, 125.33, 36.67)),
    status_bar: sb((76.5, 38.4), 18.0, (355.9, 4.67), 1.13),
    ..C_16_PLUS
};
const C_16E: Device = Device {
    viewport: vp(390.0, 844.0),
    safe_top: 47.0,
    island: None,
    status_bar: sb((51.0, 24.6), 15.6, (348.8, -8.0), 0.95),
    ..C_16
};
const C_14_PLUS: Device = Device {
    suggestions: false,
    viewport: vp(428.0, 926.0),
    safe_top: 47.0,
    island: None,
    status_bar: sb((61.7, 24.1), 17.0, (375.0, -8.33), 1.0),
    key_pitch: 55.9,
    kb_base_h: 311.0,
    ..C_17_PRO
};
const C_13_MINI: Device = Device {
    suggestions: false,
    // 375x812 pt rendered at the panel's 2.88 px/pt (1080x2340 screenshots).
    viewport: vp(375.0, 812.0),
    scale: 2.88,
    safe_top: 50.0,
    island: None,
    status_bar: sb((42.5, 27.2), 15.3, (342.6, -4.8), 0.9),
    key_pitch: 56.2,
    kb_base_h: 310.0,
    ..C_17_PRO
};
const C_11_PRO: Device = Device {
    suggestions: false,
    viewport: vp(375.0, 812.0),
    safe_top: 44.0,
    island: None,
    status_bar: sb((46.0, 22.8), 15.2, (338.5, -9.0), 0.9),
    ..C_17_PRO
};
const C_11_PRO_MAX: Device = Device {
    suggestions: false,
    viewport: vp(414.0, 896.0),
    safe_top: 44.0,
    island: None,
    status_bar: sb((56.2, 22.4), 16.1, (367.5, -10.0), 0.94),
    key_pitch: 55.9,
    kb_base_h: 311.0,
    ..C_17_PRO
};
const C_11: Device = Device { scale: 2.0, safe_top: 48.0, status_bar: sb((50.3, 24.7), 15.9, (373.5, -7.83), 0.94), ..C_11_PRO_MAX };
const C_SE: Device = Device {
    suggestions: false,
    viewport: vp(375.0, 667.0),
    scale: 2.0,
    safe_top: 20.0,
    safe_bottom: 0.0,
    island: None,
    status_bar: sb((187.5, 10.25), 11.6, (345.4, 0.9), 0.92),
    home_button: true,
    kb_base_h: 260.0,
    ..C_17_PRO
};

const fn named(name: &'static str, d: Device) -> Device {
    Device { name, ..d }
}

/// Every iPhone the iOS 26.5 Simulator ships; the default comes first.
pub const DEVICES: &[Device] = &[
    C_17_PRO,
    named("iphone-17", C_17_PRO),
    named("iphone-16-pro", C_17_PRO),
    named("iphone-17-pro-max", C_17_PRO_MAX),
    named("iphone-16-pro-max", C_17_PRO_MAX),
    named("iphone-air", C_AIR),
    named("iphone-16-plus", C_16_PLUS),
    named("iphone-15-plus", C_16_PLUS),
    named("iphone-15-pro-max", C_16_PLUS),
    named("iphone-14-pro-max", C_16_PLUS),
    named("iphone-16", C_16),
    named("iphone-15", C_16),
    named("iphone-15-pro", C_16),
    named("iphone-14-pro", C_16),
    named("iphone-16e", C_16E),
    named("iphone-17e", C_16E),
    named("iphone-14", C_16E),
    named("iphone-13", C_16E),
    named("iphone-13-pro", C_16E),
    named("iphone-12", C_16E),
    named("iphone-12-pro", C_16E),
    named("iphone-14-plus", C_14_PLUS),
    named("iphone-13-pro-max", C_14_PLUS),
    named("iphone-12-pro-max", C_14_PLUS),
    named("iphone-13-mini", C_13_MINI),
    named("iphone-12-mini", C_13_MINI),
    named("iphone-11-pro", C_11_PRO),
    named("iphone-11-pro-max", C_11_PRO_MAX),
    named("iphone-11", C_11),
    named("iphone-se-3", C_SE),
    named("iphone-se-2", C_SE),
];

pub fn device(name: &str) -> Option<&'static Device> {
    DEVICES.iter().find(|d| d.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_unique_and_default_first() {
        assert_eq!(DEVICES[0].name, DEFAULT_DEVICE);
        assert_eq!(DEVICES.len(), 31);
        for (i, d) in DEVICES.iter().enumerate() {
            assert!(DEVICES[..i].iter().all(|o| o.name != d.name), "duplicate {}", d.name);
        }
    }
}
