//! Scene graph, layout, rasterization, hit testing and UI inspection for OneSim.
//!
//! `layout::swiftui::layout` turns a protocol `ViewNode` into a framework-neutral
//! [`Scene`]; everything else works on the scene only.

pub mod layout {
    pub mod compose;
    pub mod rn;
    pub mod swiftui;
}
pub mod backend;
mod chrome;
mod device;
pub mod display;
mod display_build;
pub mod glass;
mod image;
pub use image::put as put_image;
mod interaction;
pub mod look;
mod raster;
pub mod rn_input;
mod scene;
mod symbols;
mod symbols_map;
mod text;

pub use backend::{BackendKind, BackendPref, CpuRenderer, Diagnostics, Frame, FrameTimings, RenderError, Renderer};
pub use display_build::{Builder, CACHE_MAX_ENTRIES, CACHE_TTL_FRAMES};
pub use raster::{apply_backdrop, apply_backdrop_op, clip_chain, clip_radius, needs_backdrop, node_sprite, render, render_png, sprite_key, Sprite, SpriteKey, SpriteLayer};
pub use symbols::{capability as symbols_capability, SymbolMask, SymbolMetrics, SymbolSpec};
pub use chrome::{autocap, ChromeParams};
pub use device::{device, Device, StatusBar, DEFAULT_ANDROID_DEVICE, DEFAULT_DEVICE, DEVICES};
pub use interaction::{resolve_label, scroll_anchor_offset, DragPhase, Effect, Interaction};
pub use layout::swiftui::SceneParams;
pub use scene::{Color, Glyph, Gradient, ImageRun, Node, Rect, Role, Scene, ScrollInfo, SheetInfo, SheetPos, Shadow, SymbolRun, TextRun};
pub use text::{fonts_capability, inter_tracking_em, tracking_em, FontSpec, Fonts, Span, TextAlign};

use onesim_protocol::ViewNode;

/// Layout a view tree into a scene (see [`SceneParams`]).
pub fn build_scene(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    match root.kind {
        onesim_protocol::ViewKind::Rn { .. } => layout::rn::layout(root, params, fonts),
        onesim_protocol::ViewKind::Compose { .. } => layout::compose::layout(root, params, fonts),
        // A React Native alert (`Alert.alert`): RN content under SwiftUI alert layers.
        onesim_protocol::ViewKind::Presentation { ref content, .. } if matches!(content.kind, onesim_protocol::ViewKind::Rn { .. }) => layout::rn::layout(root, params, fonts),
        _ => layout::swiftui::layout(root, params, fonts),
    }
}

/// `SIMCORE_FALLBACK=1` forces the open-licensed fallbacks (Inter, Phosphor Icons) even where Apple's resources exist.
pub fn fallback_forced() -> bool {
    std::env::var("SIMCORE_FALLBACK").is_ok_and(|v| v == "1")
}
