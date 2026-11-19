//! Per-release differences in how iOS draws the same views (`SceneParams::ios`).
//!
//! Everything not listed here is shared by every supported release. Values are measured on the
//! Simulator runtimes pinned in `docs/compatibility/versions.json`: iOS 26.5 on the iPhone 17 Pro
//! (`tests/fixtures/showcase-ios` etc.) and iOS 27.2 on the iPhone 18 Pro (`tests/fixtures/ios27`).

use onesim_protocol::IosVersion;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// Least horizontal padding inside inset-grouped list cards (row content, headers, separators).
    /// iOS 26 pads by the side margin (16, or 20 on 414pt+ screens); iOS 27 by 20 on every width.
    pub row_pad_min: f64,
    /// Keyboard letters show the shift state (lower case after typing); iOS 26 always shows capitals.
    pub key_case_follows_shift: bool,
    /// The leftmost suggestion is the word being typed, in quotes (iOS 27); iOS 26 keeps "I / The / I'm".
    pub typed_word_suggestion: bool,
    /// Draw the suggestion words on every device (iOS 27 captures all show them; iOS 26 only the 17 Pro's).
    pub always_suggestions: bool,
    /// Dark keyboard colors of iOS 27 (panel 32, keys 68; iOS 26: 23 / 61).
    pub kb_dark_27: bool,
}

const IOS26: Look = Look {
    row_pad_min: 0.0,
    key_case_follows_shift: false,
    typed_word_suggestion: false,
    always_suggestions: false,
    kb_dark_27: false,
};

const IOS27: Look = Look {
    row_pad_min: 20.0,
    key_case_follows_shift: true,
    typed_word_suggestion: true,
    always_suggestions: true,
    kb_dark_27: true,
};

pub fn look(v: IosVersion) -> &'static Look {
    match v {
        IosVersion::Ios26 => &IOS26,
        IosVersion::Ios27 => &IOS27,
    }
}

/// Total software keyboard height, including the bottom safe inset.
pub fn keyboard_height(dev: &crate::Device, v: IosVersion) -> f64 {
    let base = if dev.android { dev.kb_base_h } else { dev.kb_base_for(v) };
    base + dev.safe_bottom
}
