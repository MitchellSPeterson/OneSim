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
    /// The navigation search drawer's height, including the gap between its pill and the first list row.
    pub search_drawer_height: f64,
    /// Search field top padding after presentation replaces the inline navigation bar.
    pub active_search_top: f64,
    /// Keyboard letters show the shift state (lower case after typing); iOS 26 always shows capitals.
    pub key_case_follows_shift: bool,
    /// The leftmost suggestion is the word being typed, in quotes (iOS 27); iOS 26 keeps "I / The / I'm".
    pub typed_word_suggestion: bool,
    /// Draw the suggestion words on every device (iOS 27 captures all show them; iOS 26 only the 17 Pro's).
    pub always_suggestions: bool,
    /// Dark keyboard colors of iOS 27 (panel 32, keys 68; iOS 26: 23 / 61).
    pub kb_dark_27: bool,
    /// Gap between the numeric keyboard panel and its first row.
    pub keypad_top_inset: f64,
    /// `ShareLink`'s share sheet (`swiftui-popovers-ios/share`).
    pub share: ShareLook,
}

/// Share sheet metrics (points, card-local). The card floats `inset` from the sides and bottom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShareLook {
    pub inset: (f64, f64),
    pub card_h: f64,
    pub header_h: f64,
    /// Top-leading corner of the 64 pt preview tile.
    pub tile: (f64, f64),
    /// Leading edge and top of the title line.
    pub title: (f64, f64),
    /// First cell's leading edge, cell pitch, cell width.
    pub cell: (f64, f64, f64),
    /// App icon size and its offset in the cell.
    pub app_icon: (f64, f64, f64),
    /// Label top in the cell (app row, action row).
    pub label_y: (f64, f64),
    /// Separator above the action row (y), and both separators' leading inset.
    pub sep: (f64, f64),
    /// Action circle top in its cell.
    pub action_y: f64,
    /// Activities offered for links (app row, action row); text always gets Reminders, More / Copy, Save to Files.
    pub url_apps: &'static [&'static str],
    pub url_actions: &'static [&'static str],
}

const IOS26: Look = Look {
    row_pad_min: 0.0,
    search_drawer_height: 60.0,
    active_search_top: 8.0,
    key_case_follows_shift: false,
    typed_word_suggestion: false,
    always_suggestions: false,
    kb_dark_27: false,
    keypad_top_inset: 23.0,
    share: ShareLook {
        inset: (9.0, 9.33),
        card_h: 366.67,
        header_h: 104.33,
        tile: (24.33, 24.33),
        title: (103.33, 46.33),
        cell: (18.33, 88.33, 82.0),
        app_icon: (68.0, 7.0, 16.0),
        label_y: (92.0, 92.0),
        sep: (227.33, 24.33),
        action_y: 17.0,
        url_apps: &["Reminders", "More"],
        url_actions: &["Copy", "Add to Reading List"],
    },
};

const IOS27: Look = Look {
    row_pad_min: 20.0,
    search_drawer_height: 54.0,
    active_search_top: 0.0,
    key_case_follows_shift: true,
    typed_word_suggestion: true,
    always_suggestions: true,
    kb_dark_27: true,
    keypad_top_inset: 24.0,
    share: ShareLook {
        inset: (9.0, 9.0),
        card_h: 364.0,
        header_h: 107.67,
        tile: (27.67, 28.0),
        title: (106.67, 50.67),
        cell: (19.67, 87.67, 82.0),
        app_icon: (66.0, 8.0, 16.0),
        label_y: (90.0, 88.0),
        sep: (228.67, 27.67),
        action_y: 16.0,
        url_apps: &["Reminders", "Safari", "More"],
        url_actions: &["Copy", "Open in Safari", "Add to Reading List"],
    },
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

/// Numeric and phone keyboards reserve less space than the alphabetic keyboard.
pub(crate) fn keypad_height(dev: &crate::Device, v: IosVersion) -> f64 {
    let reduction = match v { IosVersion::Ios26 => 28.0, IosVersion::Ios27 => 26.0 };
    keyboard_height(dev, v) - reduction
}
