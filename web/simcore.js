// SimCore driver shared by the browser (main.js) and Node (test/run.mjs): a minimal WASI shim and a
// typed wrapper over the `simcore_web_*` exports of app.wasm. No DOM, no Node APIs.
export const DEVICES = { "iphone-17-pro": { width: 402, height: 874, scale: 3 } };
export const DEFAULT_DEVICE = "iphone-17-pro";

const ENOSYS = 52, EBADF = 8;
const dec = new TextDecoder(), enc = new TextEncoder();

/** The Swift runtime's calls into WASI: just what Swift + Rust std touch at startup. */
function makeWasi(getMem, log) {
  const impl = {
    random_get(ptr, len) { crypto.getRandomValues(new Uint8Array(getMem().buffer, ptr, len)); return 0; },
    args_sizes_get(argc, size) { const v = new DataView(getMem().buffer); v.setUint32(argc, 0, true); v.setUint32(size, 0, true); return 0; },
    args_get() { return 0; },
    environ_sizes_get(n, size) { const v = new DataView(getMem().buffer); v.setUint32(n, 0, true); v.setUint32(size, 0, true); return 0; },
    environ_get() { return 0; },
    clock_time_get(id, _prec, out) {
      const ns = BigInt(Math.round((id === 0 ? Date.now() : performance.now()) * 1e6));
      new DataView(getMem().buffer).setBigUint64(out, ns, true);
      return 0;
    },
    fd_write(fd, iovs, n, nwritten) { // stdout/stderr -> log
      const v = new DataView(getMem().buffer);
      let text = "", total = 0;
      for (let i = 0; i < n; i++) {
        const p = v.getUint32(iovs + i * 8, true), l = v.getUint32(iovs + i * 8 + 4, true);
        text += dec.decode(new Uint8Array(getMem().buffer, p, l));
        total += l;
      }
      v.setUint32(nwritten, total, true);
      log(fd === 2 ? "error" : "log", text.replace(/\n$/, ""));
      return 0;
    },
    fd_fdstat_get() { return EBADF; },
    fd_prestat_get() { return EBADF; }, // no preopened dirs: ends libc's preopen scan
    proc_exit(code) { throw new Error(`proc_exit(${code})`); },
  };
  // Anything else (files, seek, ...) is unsupported: report ENOSYS rather than failing to link.
  return new Proxy(impl, { get: (t, k) => t[k] ?? (() => ENOSYS) });
}

/**
 * Instantiate app.wasm. `source` is a fetch `Response` (browser, streaming compile) or bytes.
 * `log(level, text)` receives the guest's stdout/stderr.
 */
export async function load(source, { log = (level, t) => console[level](t) } = {}) {
  let mem;
  const imports = { wasi_snapshot_preview1: makeWasi(() => mem, log) };
  const { instance } = typeof Response !== "undefined" && source instanceof Response
    ? await WebAssembly.instantiateStreaming(source, imports)
    : await WebAssembly.instantiate(source, imports);
  mem = instance.exports.memory;
  instance.exports._initialize(); // reactor: run constructors, keep the instance alive
  return new SimCore(instance.exports);
}

export class SimCore {
  constructor(wasm) { this.wasm = wasm; this.width = 0; this.height = 0; this.scale = 0; }

  get mem() { return this.wasm.memory.buffer; }

  /** Start the app: viewport in points, scale = device pixels per point. Renders the first frame. */
  init({ width = 402, height = 874, scale = 3, dark = false } = {}) {
    Object.assign(this, { width, height, scale });
    if (this.wasm.simcore_web_init(width, height, scale, dark ? 1 : 0) !== 0) throw new Error("simcore_web_init failed");
    return this;
  }

  // Scratch buffer in Wasm memory for strings in and JSON/results out.
  #put(str) {
    const b = enc.encode(str), ptr = this.wasm.simcore_web_buf(b.length); // may grow memory: read it after
    new Uint8Array(this.mem, ptr, b.length).set(b);
    return b.length;
  }
  #take() {
    const len = this.wasm.simcore_web_buf_len(), ptr = this.wasm.simcore_web_buf(len);
    return new Uint8Array(this.mem, ptr, len);
  }

  /** Pointer event in points: kind "down" | "up" | "click". Returns whether anything changed. */
  pointer(kind, x, y) { return this.wasm.simcore_web_pointer({ down: 0, up: 1, click: 2 }[kind], x, y) === 1; }
  click(x, y) { return this.pointer("click", x, y); }
  /** Scroll the scroller under (x, y) by (dx, dy) points; positive dy reveals later content. */
  scroll(x, y, dx, dy) { return this.wasm.simcore_web_scroll(x, y, dx, dy) === 1; }
  /** Text typed into the focused field. Throws if none is focused. */
  type(text) { return this.#key(0, text); }
  /** Text typed by a person (physical keyboard): software-keyboard shift follows auto-capitalization. */
  typeByUser(text) { return this.#key(2, text); }
  /** "backspace" | "enter" in the focused field. */
  key(name) { return this.#key(1, name); }
  #key(kind, s) {
    const r = this.wasm.simcore_web_key(kind, this.#put(s));
    if (r < 0) throw new Error("no focused text field (click a text field first)");
    return r === 1;
  }
  get focused() { return this.wasm.simcore_web_focused() === 1; }

  /** Point (points) of a control by label, with the CLI's `--click` syntax (`Label`, `Label#increment`, keys). */
  find(label) {
    const n = this.#put(label);
    const r = this.wasm.simcore_web_find(n);
    const out = this.#take();
    if (r !== 0) throw new Error(dec.decode(out));
    const v = new DataView(out.buffer, out.byteOffset, 16);
    return [v.getFloat64(0, true), v.getFloat64(8, true)];
  }
  /** Click a control by label. */
  clickLabel(label) {
    const [x, y] = this.find(label);
    return this.click(x, y);
  }

  /** Inspection tree (protocol UiNode), as `inspect_ui_tree`. */
  inspect() {
    if (this.wasm.simcore_web_inspect() !== 0) throw new Error("no tree yet");
    return JSON.parse(dec.decode(this.#take()));
  }
  /** Text of every node in the inspection tree, in order (for assertions). */
  texts() {
    const out = [];
    const walk = (n) => { if (n.text != null) out.push(n.text); n.children.forEach(walk); };
    walk(this.inspect());
    return out;
  }
  /** Latest guest tree JSON (ViewNode). */
  lastTreeText() {
    const ptr = this.wasm.simcore_web_tree_ptr(), len = this.wasm.simcore_web_tree_len();
    return dec.decode(new Uint8Array(this.mem, ptr, len));
  }

  setViewport(width, height) {
    this.width = width; this.height = height;
    return this.wasm.simcore_web_set_viewport(width, height) === 0;
  }
  setAppearance(mode) {
    if (mode !== "dark" && mode !== "light") throw new Error("appearance must be 'dark' or 'light'");
    return this.wasm.simcore_web_set_dark(mode === "dark" ? 1 : 0) === 0;
  }
  setScale(scale) { this.scale = scale; this.wasm.simcore_web_set_scale(scale); }

  /** Current frame: RGBA bytes (a copy) and pixel size. */
  framebuffer() {
    const w = this.wasm.simcore_web_fb_width(), h = this.wasm.simcore_web_fb_height();
    const ptr = this.wasm.simcore_web_fb_ptr();
    return { width: w, height: h, data: new Uint8Array(this.mem, ptr, w * h * 4).slice() };
  }
}
