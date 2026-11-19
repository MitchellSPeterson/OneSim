//! Blocking texture readback (native only).

use std::sync::{Arc, Mutex};
use std::time::Instant;

use onesim_render::FrameTimings;
use vello::wgpu;

/// A reusable staging buffer for reading `Rgba8Unorm` textures back to the CPU.
#[derive(Default)]
pub(crate) struct Readback {
    staging: Option<(u64, wgpu::Buffer)>,
}

impl Readback {
    /// Copy `tex` (`w` x `h`, straight alpha) out as tightly packed RGBA, premultiplying if asked. Adds the time spent
    /// to `t` (`encode`: submitting the copy, `gpu`: waiting for the GPU, `readback`: copying out).
    pub fn read(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, tex: &wgpu::Texture, (w, h): (u32, u32), premultiply: bool, t: &mut FrameTimings) -> Result<Vec<u8>, String> {
        let t0 = Instant::now();
        let bpr = (w * 4).div_ceil(256) * 256;
        let size = bpr as u64 * h as u64;
        if self.staging.as_ref().is_none_or(|s| s.0 != size) {
            let buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("onesim-readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
            self.staging = Some((size, buf));
        }
        let Some((_, buf)) = self.staging.as_ref() else { return Err("no staging buffer".into()) };
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("onesim-readback") });
        enc.copy_texture_to_buffer(
            tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        queue.submit([enc.finish()]);
        let result = Arc::new(Mutex::new(None));
        let r = result.clone();
        buf.slice(..).map_async(wgpu::MapMode::Read, move |res| {
            if let Ok(mut g) = r.lock() {
                *g = Some(res);
            }
        });
        t.encode += t0.elapsed();
        let t1 = Instant::now();
        device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| format!("GPU wait: {e:?}"))?;
        t.gpu += t1.elapsed();
        let t2 = Instant::now();
        let mapped = result.lock().ok().and_then(|mut g| g.take());
        match mapped {
            Some(Ok(())) => {}
            Some(Err(e)) => return Err(format!("readback map: {e}")),
            None => return Err("readback never completed".into()),
        }
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        {
            let view = buf.slice(..).get_mapped_range().map_err(|e| format!("readback range: {e}"))?;
            let row = w as usize * 4;
            for src in view.chunks_exact(bpr as usize) {
                let src = &src[..row];
                let (src4, _) = src.as_chunks::<4>();
                // Opaque rows (nearly all of a full-screen frame) are a plain copy.
                // A reduction lets the compiler check multiple alpha bytes per instruction.
                if !premultiply || src4.iter().fold(255, |alpha, p| alpha & p[3]) == 255 {
                    out.extend_from_slice(src);
                    continue;
                }
                for s in src4 {
                    let a = s[3] as u32;
                    if a == 255 {
                        out.extend_from_slice(s);
                    } else {
                        // Vello writes straight alpha; the capture convention is premultiplied.
                        let pm = |c: u8| {
                            let p = c as u32 * a + 128;
                            ((p + (p >> 8)) >> 8) as u8
                        };
                        out.extend_from_slice(&[pm(s[0]), pm(s[1]), pm(s[2]), s[3]]);
                    }
                }
            }
        }
        buf.unmap();
        t.readback += t2.elapsed();
        Ok(out)
    }
}

/// Read a whole `Rgba8Unorm` texture (with `COPY_SRC`) back as tightly packed straight-alpha RGBA, blocking until the
/// GPU is done. For hosts that drive [`crate::Executor`] themselves and tests.
pub fn read_texture(device: &wgpu::Device, queue: &wgpu::Queue, tex: &wgpu::Texture) -> Result<Vec<u8>, String> {
    Readback::default().read(device, queue, tex, (tex.width(), tex.height()), false, &mut FrameTimings::default())
}
