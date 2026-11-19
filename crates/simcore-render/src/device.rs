//! Device profiles: screen geometry and chrome, shared by the native host and the WebAssembly build.

use simcore_protocol::{IosVersion,Viewport};

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
    /// Right safe-area inset: a column beside the content holding the status bar and the back button (iPhone Duo).
    pub safe_right: f64,
    /// Compact navigation bar: the large title sits in a 58pt bar at 28pt and never collapses (iPhone Duo).
    pub compact_nav: bool,
    /// Wide (iPad-style) keyboard: full-width panel, 79pt key pitch in an 801pt block (iPhone Duo).
    pub wide_keyboard: bool,
    /// Home indicator drawn by the host over SwiftUI content `(x, y, w, h)` (iPhone Duo).
    pub home_indicator: Option<(f64, f64, f64, f64)>,
    /// iOS 27 chrome where it was measured (`tests/fixtures/ios27`): status bar and keyboard height.
    /// Other profiles keep their iOS 26 status bar and get a keyboard 7pt shorter (as on the 17e / 18 Pro).
    pub ios27: Option<Chrome27>,
    /// Android phone (Material status bar and gesture-navigation handle; `safe_top`/`safe_bottom` are the
    /// status-bar and navigation-bar insets an edge-to-edge app draws under). Units are dp.
    pub android: bool,
}

/// iOS 27 status bar and keyboard height (above the bottom inset) of one device.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chrome27 {
    pub status_bar: StatusBar,
    pub kb_base_h: f64,
}

impl Device {
    /// Status bar placement under release `v`.
    pub fn status_bar_for(&self, v: IosVersion) -> StatusBar {
        match (v, self.ios27) {
            (IosVersion::Ios27, Some(c)) => c.status_bar,
            _ => self.status_bar,
        }
    }
    /// Keyboard height above the bottom safe inset under release `v`.
    pub fn kb_base_for(&self, v: IosVersion) -> f64 {
        match (v, self.ios27) {
            (IosVersion::Ios26, _) => self.kb_base_h,
            (IosVersion::Ios27, Some(c)) => c.kb_base_h,
            (IosVersion::Ios27, None) => self.kb_base_h - 7.0,
        }
    }
}

pub const DEFAULT_DEVICE: &str = "iphone-18-pro";

const fn sb(time: (f64, f64), font: f64, icons: (f64, f64), icon_scale: f64) -> StatusBar {
    StatusBar { time, font, icons, icon_scale }
}
const fn vp(width: f64, height: f64) -> Viewport {
    Viewport { width, height }
}

// One const per geometry class; models that render pixel-identically share it (see tests/fixtures/devices).
const C_17_PRO: Device = Device {
    name: "iphone-17-pro",
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
    safe_right: 0.0,
    compact_nav: false,
    wide_keyboard: false,
    home_indicator: None,
    ios27: None,
    android: false,
};
// iOS 27.2 reference device (tests/fixtures/ios27): the 17 Pro's screen with a narrower island and
// a larger status bar (measured: island x 153.3..248.3, clock 63.3..96.7, battery right edge 361.7).
const C_18_PRO: Device = Device {
    name: DEFAULT_DEVICE,
    island: Some((153.4, 13.92, 95.0, 36.83)),
    status_bar: sb((81.0, 32.2), 18.0, (332.7, -0.6), 1.1),
    ios27: Some(Chrome27 {
        status_bar: sb((81.0, 32.2), 18.0, (332.7, -0.6), 1.1),
        kb_base_h: 294.0,
    }),
    ..C_17_PRO
};
// iOS 27.2 (tests/fixtures/ios27/devices/iphone-18-pro-max): the 17 Pro Max screen, island x 172.3..267.3,
// clock 71.7..107, battery right edge 393.7; keyboard top 609 (2pt higher than iOS 26 on the 17 Pro Max).
const C_18_PRO_MAX: Device = Device {
    name: "iphone-18-pro-max",
    island: Some((172.4, 13.92, 95.0, 36.83)),
    ios27: Some(Chrome27 {
        status_bar: sb((89.3, 32.2), 19.0, (362.8, -1.2), 1.17),
        kb_base_h: 313.0,
    }),
    ..C_17_PRO_MAX
};
// iPhone Duo, unfolded (iOS 27.2 seed runtime 24B91016s, tests/fixtures/ios27/devices/iphone-duo): 951x669 pt at 2x,
// landscape-like. Content 887pt wide; the right 64pt column holds the clock (centre 902.5,40), a battery ring around
// the wifi glyph (centre 903,74) and the back button. Title bar 24..82 with a 28pt title; keyboard panel from y 405.
const C_DUO: Device = Device {
    name: "iphone-duo",
    viewport: vp(951.0, 669.0),
    scale: 2.0,
    safe_top: 24.0,
    safe_bottom: 20.0,
    island: None,
    status_bar: sb((902.5, 40.0), 17.0, (0.0, 0.0), 1.1),
    suggestions: true,
    key_pitch: 52.0,
    kb_base_h: 244.0,
    safe_right: 64.0,
    compact_nav: true,
    wide_keyboard: true,
    home_indicator: Some((372.0, 656.3, 225.0, 4.7)),
    ios27: Some(Chrome27 {
        status_bar: sb((902.5, 40.0), 17.0, (0.0, 0.0), 1.1),
        kb_base_h: 244.0,
    }),
    ..C_17_PRO
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
    // iOS 27.2 (tests/fixtures/ios27/devices/iphone-17e): clock 46.3..78, battery right edge 363.3, keyboard top 516.
    ios27: Some(Chrome27 {
        status_bar: sb((62.2, 25.9), 17.0, (336.4, -7.2), 1.02),
        kb_base_h: 294.0,
    }),
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

/// Pixel 10a (Android 17 emulator AVD): 1080x2424 px at 420 dpi = 411.43 x 923.43 dp, density 2.625.
/// Insets measured on the emulator (`dumpsys window`): status bar 142 px, gesture navigation bar 63 px.
const C_PIXEL_10A: Device = Device {
    name: "pixel-10a",
    viewport: vp(1080.0 / 2.625, 2424.0 / 2.625),
    scale: 2.625,
    safe_top: 142.0 / 2.625,
    safe_bottom: 63.0 / 2.625,
    island: None,
    // Measured on the emulator: clock text box centred at (57.9, 26.9) dp, 14 sp; battery nub ends at 382.9 dp.
    status_bar: sb((57.9, 26.9), 14.0, (382.9, 0.0), 1.0),
    home_button: false,
    suggestions: false,
    key_pitch: 53.9,
    kb_base_h: 280.0,
    safe_right: 0.0,
    compact_nav: false,
    wide_keyboard: false,
    home_indicator: None,
    ios27: None,
    android: true,
};

/// Default device of Android sessions.
pub const DEFAULT_ANDROID_DEVICE: &str = "pixel-10a";

const fn named(name: &'static str, d: Device) -> Device {
    Device { name, ..d }
}

/// iPhone profiles for iOS 26.5 and 27.2 (the default first), followed by Android profiles.
pub const DEVICES: &[Device] = &[
    C_18_PRO,
    C_18_PRO_MAX,
    C_DUO,
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
    C_PIXEL_10A,
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
        assert_eq!(DEVICES.len(), 35);
        assert_eq!(DEVICES.iter().filter(|d| d.android).map(|d| d.name).collect::<Vec<_>>(), [DEFAULT_ANDROID_DEVICE]);
        for (i, d) in DEVICES.iter().enumerate() {
            assert!(DEVICES[..i].iter().all(|o| o.name != d.name), "duplicate {}", d.name);
        }
    }
}
