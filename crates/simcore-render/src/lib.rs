//! Scene graph, layout, rasterization, hit testing and UI inspection for SimCore.
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
pub mod glass;
mod image;
mod interaction;
mod raster;
mod scene;
mod symbols;
mod symbols_map;
mod text;

pub use backend::{BackendKind, BackendPref, CpuRenderer, Diagnostics, Frame, FrameTimings, RenderError, Renderer};
pub use raster::{apply_backdrop, clip_radius, needs_backdrop, node_sprite, render, render_png, sprite_key, Sprite, SpriteKey, SpriteLayer};
pub use symbols::{capability as symbols_capability, SymbolMask, SymbolMetrics, SymbolSpec};
pub use chrome::{autocap, ChromeParams};
pub use device::{device, Device, StatusBar, DEFAULT_ANDROID_DEVICE, DEFAULT_DEVICE, DEVICES};
pub use interaction::{resolve_label, DragPhase, Effect, Interaction};
pub use layout::swiftui::SceneParams;
pub use scene::{Color, Glyph, Gradient, ImageRun, Node, Rect, Role, Scene, ScrollInfo, SheetInfo, SheetPos, Shadow, SymbolRun, TextRun};
pub use text::{fonts_capability, inter_tracking_em, tracking_em, FontSpec, Fonts, Span, TextAlign};

use simcore_protocol::ViewNode;

/// Layout a view tree into a scene (see [`SceneParams`]).
pub fn build_scene(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    match root.kind {
        simcore_protocol::ViewKind::Rn { .. } => layout::rn::layout(root, params, fonts),
        simcore_protocol::ViewKind::Compose { .. } => layout::compose::layout(root, params, fonts),
        // A React Native alert (`Alert.alert`): RN content under SwiftUI alert layers.
        simcore_protocol::ViewKind::Presentation { ref content, .. } if matches!(content.kind, simcore_protocol::ViewKind::Rn { .. }) => layout::rn::layout(root, params, fonts),
        _ => layout::swiftui::layout(root, params, fonts),
    }
}

/// `SIMCORE_FALLBACK=1` forces the open-licensed fallbacks (Inter, Phosphor Icons) even where Apple's resources exist.
pub fn fallback_forced() -> bool {
    std::env::var("SIMCORE_FALLBACK").is_ok_and(|v| v == "1")
}
