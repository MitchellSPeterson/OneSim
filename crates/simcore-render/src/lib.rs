//! Scene graph, layout, rasterization, hit testing and UI inspection for SimCore.
//!
//! `layout::swiftui::layout` turns a protocol `ViewNode` into a framework-neutral
//! [`Scene`]; everything else works on the scene only.

pub mod layout {
    pub mod swiftui;
}
mod chrome;
pub mod glass;
mod interaction;
mod raster;
mod scene;
mod symbols;
mod symbols_map;
mod text;

pub use raster::{render, render_png};
pub use symbols::{capability as symbols_capability, SymbolMask, SymbolMetrics, SymbolSpec};
pub use chrome::{autocap, ChromeParams};
pub use interaction::{resolve_label, DragPhase, Effect, Interaction};
pub use layout::swiftui::SceneParams;
pub use scene::{Color, Glyph, Node, Rect, Role, Scene, ScrollInfo, Shadow, SymbolRun, TextRun};
pub use text::{fonts_capability, inter_tracking_em, tracking_em, FontSpec, Fonts, TextAlign};

use simcore_protocol::ViewNode;

/// Layout a view tree into a scene (see [`SceneParams`]).
pub fn build_scene(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    layout::swiftui::layout(root, params, fonts)
}

/// `SIMCORE_FALLBACK=1` forces the open-licensed fallbacks (Inter, Phosphor Icons) even where Apple's resources exist.
pub fn fallback_forced() -> bool {
    std::env::var("SIMCORE_FALLBACK").is_ok_and(|v| v == "1")
}
