//! GPU backend (wgpu + Vello) for SimCore display lists.
//!
//! The crate has two layers:
//!
//! - [`Executor`] runs a [`simcore_render::display::DisplayList`] into a texture. It needs no fonts, scene or
//!   layout: text, icons and symbols arrive as pre-rasterized sprites, every shape and gradient is already in device
//!   pixels. Vector content is drawn by Vello into an `Rgba8Unorm` storage texture. Liquid Glass and scroll-edge
//!   effects read the backdrop, which Vello cannot: the frame is split there, each segment is rendered by Vello into
//!   the target, copied into an accumulation texture and composited by compute passes (see [`glass`]); the next
//!   segment starts by drawing the accumulation texture as a full-frame image. With `default-features = false` it
//!   builds for `wasm32-unknown-unknown` (`Executor::new_async`).
//! - With the default `native` feature, [`GpuRenderer`] implements [`simcore_render::Renderer`] headlessly:
//!   `simcore_render::Builder` lowers the scene, the executor runs it, and the target is read back once per frame.

mod executor;
mod glass;
#[cfg(feature = "native")]
mod headless;
#[cfg(feature = "native")]
mod readback;

pub use executor::{target_usage, ExecStats, Executor};
#[cfg(feature = "native")]
pub use headless::{adapter_available, GpuRenderer};
#[cfg(feature = "native")]
pub use readback::read_texture;
pub use vello::wgpu;
