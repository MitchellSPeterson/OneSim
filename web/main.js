// SimCore in the browser: one Wasm module (Swift app + SwiftUI module + simcore-web renderer).
// This file supplies a minimal WASI shim, draws the framebuffer, and forwards pointer events.
// Everything is in-process: no server, no network beyond fetching app.wasm.
const POINTS = { w: 402, h: 874 }; // iphone-17-pro, docs/web.md

const canvas = document.getElementById("screen");
const ctx = canvas.getContext("2d");
let wasm, mem; // instance exports, memory

// ---- WASI shim: just what Swift + Rust std touch at startup ----
const ENOSYS = 52, EBADF = 8;
const wasiImpl = {
  random_get(ptr, len) { crypto.getRandomValues(new Uint8Array(mem.buffer, ptr, len)); return 0; },
  args_sizes_get(argc, size) { const v = new DataView(mem.buffer); v.setUint32(argc, 0, true); v.setUint32(size, 0, true); return 0; },
  args_get() { return 0; },
  environ_sizes_get(n, size) { const v = new DataView(mem.buffer); v.setUint32(n, 0, true); v.setUint32(size, 0, true); return 0; },
  environ_get() { return 0; },
  clock_time_get(id, _prec, out) {
    const ns = BigInt(Math.round((id === 0 ? Date.now() : performance.now()) * 1e6));
    new DataView(mem.buffer).setBigUint64(out, ns, true);
    return 0;
  },
  fd_write(fd, iovs, n, nwritten) { // stdout/stderr -> console
    const v = new DataView(mem.buffer);
    let text = "", total = 0;
    for (let i = 0; i < n; i++) {
      const p = v.getUint32(iovs + i * 8, true), l = v.getUint32(iovs + i * 8 + 4, true);
      text += new TextDecoder().decode(new Uint8Array(mem.buffer, p, l));
      total += l;
    }
    v.setUint32(nwritten, total, true);
    (fd === 2 ? console.error : console.log)(text.replace(/\n$/, ""));
    return 0;
  },
  fd_fdstat_get() { return EBADF; },
  fd_prestat_get() { return EBADF }, // no preopened dirs: ends libc's preopen scan
  proc_exit(code) { throw new Error(`proc_exit(${code})`); },
};
// Anything else (files, seek, ...) is unsupported: report ENOSYS rather than failing to link.
const wasi = new Proxy(wasiImpl, { get: (t, k) => t[k] ?? (() => ENOSYS) });

// ---- framebuffer + input ----
function text(ptr, len) { return new TextDecoder().decode(new Uint8Array(mem.buffer, ptr, len)); }

function present() {
  const w = wasm.simcore_web_fb_width(), h = wasm.simcore_web_fb_height();
  if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
  const px = new Uint8ClampedArray(mem.buffer, wasm.simcore_web_fb_ptr(), w * h * 4).slice();
  ctx.putImageData(new ImageData(px, w, h), 0, 0);
}

function toPoints(e) {
  const r = canvas.getBoundingClientRect();
  return [((e.clientX - r.left) * POINTS.w) / r.width, ((e.clientY - r.top) * POINTS.h) / r.height];
}

function pointer(kind, e) {
  const [x, y] = toPoints(e);
  wasm.simcore_web_pointer(kind, x, y); // Swift re-evaluates and resubmits inside this call
  present();
}

async function main() {
  const res = await fetch("app.wasm");
  const { instance } = await WebAssembly.instantiateStreaming(res, { wasi_snapshot_preview1: wasi });
  wasm = instance.exports; mem = wasm.memory;
  wasm._initialize(); // reactor: run constructors, keep the instance alive
  const q = new URLSearchParams(location.search);
  const scale = Number(q.get("scale")) || window.devicePixelRatio || 1;
  const dark = q.has("dark") ? q.get("dark") !== "0" : matchMedia("(prefers-color-scheme: dark)").matches;
  if (wasm.simcore_web_init(scale, dark ? 1 : 0) !== 0) throw new Error("simcore_web_init failed");
  present();
  canvas.addEventListener("pointerdown", (e) => pointer(0, e));
  canvas.addEventListener("pointerup", (e) => pointer(1, e));
  // Debug/test hooks.
  window.simcore = {
    lastTreeText: () => text(wasm.simcore_web_tree_ptr(), wasm.simcore_web_tree_len()),
    setDark(d) { wasm.simcore_web_set_dark(d ? 1 : 0); present(); },
    pointer(kind, x, y) { wasm.simcore_web_pointer(kind, x, y); present(); }, // points
    ready: true,
  };
}

main().catch((e) => { document.getElementById("err").textContent = String(e.stack || e); console.error(e); });
