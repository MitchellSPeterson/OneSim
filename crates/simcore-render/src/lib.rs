//! Scene graph, layout, rasterization, hit testing and UI inspection for SimCore.
//!
//! `layout::swiftui::layout` turns a protocol `ViewNode` into a framework-neutral
//! [`Scene`]; everything else works on the scene only.

pub mod layout {
    pub mod swiftui;
}
mod raster;
mod scene;
mod text;

pub use raster::{render, render_png};
pub use layout::swiftui::SceneParams;
pub use scene::{Color, Glyph, Node, Rect, Role, Scene, ScrollInfo, Shadow, TextRun};
pub use text::{tracking_em, FontSpec, Fonts, TextAlign};

use simcore_protocol::ViewNode;

/// Layout a view tree into a scene (see [`SceneParams`]).
pub fn build_scene(root: &ViewNode, params: &SceneParams, fonts: &mut Fonts) -> Scene {
    layout::swiftui::layout(root, params, fonts)
}
