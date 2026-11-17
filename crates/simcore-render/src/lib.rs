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
pub use scene::{Color, Node, Rect, Role, Scene, TextRun};
pub use text::Fonts;

use simcore_protocol::{Appearance, ViewNode};

/// Convenience: layout a view tree for a viewport.
pub fn build_scene(root: &ViewNode, viewport: (f64, f64), appearance: Appearance, fonts: &mut Fonts) -> Scene {
    layout::swiftui::layout(root, viewport, appearance, fonts)
}
