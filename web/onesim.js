// OneSim driver shared by the browser (main.js) and Node (test/run.mjs): a minimal WASI shim and a
// typed wrapper over the `simcore_web_*` exports of app.wasm. No DOM, no Node APIs.
const SCENE_PHASES = ["active", "inactive", "background"];
const DYNAMIC_TYPE = ["xSmall", "small", "medium", "large", "xLarge", "xxLarge", "xxxLarge", "accessibility1", "accessibility2", "accessibility3", "accessibility4", "accessibility5"];

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
 * For JS-hosted guests (e.g. Kotlin/Wasm Compose), `guest` or `guestUrl` supplies the guest module
 * (defaults to `./guest/guest.mjs` next to `app.wasm`).
 */
export async function load(source, { log = (level, t) => console[level](t), guest = null, guestUrl = null } = {}) {
  const mod = typeof Response !== "undefined" && source instanceof Response
    ? await WebAssembly.compileStreaming(source)
    : await WebAssembly.compile(source);
  let mem, exports;
  const imports = { wasi_snapshot_preview1: makeWasi(() => mem, log) };
  if (WebAssembly.Module.imports(mod).some((i) => i.module === "onesim_guest")) {
    if (!guest) {
      const url = guestUrl ?? (typeof Response !== "undefined" && source instanceof Response && source.url
        ? new URL("./guest/guest.mjs", source.url).href
        : new URL("./guest/guest.mjs", import.meta.url).href);
      guest = await import(url);
    }
    const submitTree = (tree) => {
      if (!tree) return 0;
      const b = enc.encode(tree);
      const ptr = exports.simcore_web_buf(b.length);
      new Uint8Array(exports.memory.buffer, ptr, b.length).set(b);
      const rc = exports.simcore_web_submit_tree(b.length, guest.onesimVersion());
      if (rc !== 0) log("error", "onesim: simcore_web_submit_tree failed");
      return rc;
    };
    imports.onesim_guest = {
      on_init(dark, _w, _h, safeTop, safeBottom) {
        return submitTree(guest.onesimStart(dark !== 0, safeTop, safeBottom));
      },
      on_env(dark, _w, _h, safeTop, safeBottom) {
        return submitTree(guest.onesimEnvironment(dark !== 0, safeTop, safeBottom));
      },
      on_action(id) {
        submitTree(guest.onesimAction(id));
      },
      on_text(field, ptr, len) {
        const text = dec.decode(new Uint8Array(exports.memory.buffer, ptr, len));
        submitTree(guest.onesimTextInput(field, text));
      },
    };
  }
  const instance = await WebAssembly.instantiate(mod, imports);
  exports = instance.exports;
  mem = exports.memory;
  exports._initialize?.(); // reactor: run constructors, keep the instance alive
  return new OneSim(exports, guest);
}

export class OneSim {
  #devices = null;
  constructor(wasm, guest = null) { this.wasm = wasm; this.guest = guest; this.width = 0; this.height = 0; this.scale = 0; }

  get mem() { return this.wasm.memory.buffer; }

  /** Platform of this build ("android" for Compose, "ios" for SwiftUI). */
  get platform() { return this.guest?.onesimPlatform?.() ?? "ios"; }

  /** Device profiles from the Wasm build (crates/onesim-render/src/device.rs). */
  get devices() {
    if (!this.#devices) {
      this.wasm.simcore_web_devices();
      this.#devices = JSON.parse(dec.decode(this.#take()));
    }
    return this.#devices;
  }

  /** Default device profile for this app's platform. */
  get defaultDevice() {
    const devs = this.devices;
    return this.platform === "android" ? (devs.find((d) => d.android) ?? devs[0]) : devs[0];
  }

  /**
   * Start the app on `device` (a name from `devices`, default the platform's default). Viewport in points and
   * scale (device pixels per point) default to the device's own; pass them to override.
   * Renders the first frame.
   */
  init({ device, width, height, scale, dark = false, dynamicType = "large" } = {}) {
    const devs = this.devices, target = device ?? this.defaultDevice.name;
    const dev = devs.find((d) => d.name === target);
    if (!dev) throw new Error(`unknown device ${JSON.stringify(device)} (known: ${devs.map((d) => d.name).join(", ")})`);
    if (this.wasm.simcore_web_set_device(this.#put(dev.name)) !== 0) throw new Error(`unknown device ${JSON.stringify(dev.name)}`);
    width ??= dev.width; height ??= dev.height; scale ??= dev.scale;
    Object.assign(this, { width, height, scale });
    this.setDynamicType(dynamicType);
    if (this.wasm.simcore_web_init(width, height, scale, dark ? 1 : 0) !== 0) throw new Error("simcore_web_init failed");
    return this;
  }

  /**
   * React Native mode (the guest runs in a Worker, web/expo/guest.js): like `init`, but nothing draws until the
   * guest's first tree arrives through `receive`. `android` picks the Android look and device.
   */
  initRn({ device, width, height, scale, dark = false, android = false } = {}) {
    const devs = this.devices, dev = devs.find((d) => (device === undefined ? d.android === android : d.name === device));
    if (!dev || this.wasm.simcore_web_set_device(this.#put(dev.name)) !== 0) throw new Error(`unknown device ${JSON.stringify(device)}`);
    width ??= dev.width; height ??= dev.height; scale ??= dev.scale;
    Object.assign(this, { width, height, scale });
    if (this.wasm.simcore_web_rn_init(width, height, scale, dark ? 1 : 0, android ? 1 : 0) !== 0) throw new Error("simcore_web_rn_init failed");
    return this;
  }
  /** A guest message (protocol section H). True if the frame changed (present it). */
  receive(msg) {
    const json = typeof msg === "string" ? msg : typeof msg?.rawJson === "string" ? msg.rawJson : JSON.stringify(msg);
    const r = this.wasm.simcore_web_rn_receive(this.#put(json));
    if (r < 0) throw new Error(dec.decode(this.#take()));
    return r === 1;
  }
  /** An image file the guest wrote (PNG/JPEG bytes at the path its tree names). True if it decoded (frame redrawn). */
  putImage(path, bytes) {
    const p = enc.encode(path), ptr = this.wasm.simcore_web_buf(p.length + 1 + bytes.length), b = new Uint8Array(this.mem, ptr, p.length + 1 + bytes.length);
    b.set(p); b[p.length] = 0; b.set(bytes, p.length + 1);
    return this.wasm.simcore_web_put_image(b.length) === 1;
  }
  /** Host messages waiting for the guest (post each to it); empties the queue. */
  outbox() {
    return this.wasm.simcore_web_rn_outbox() > 0 ? JSON.parse(dec.decode(this.#take())) : [];
  }

  /** Replace the app's pasteboard (what `PasteButton` pastes) with these strings. True if the guest took it
   *  (SwiftUI builds only). */
  setPasteboard(items) {
    return !!this.wasm.simcore_web_set_pasteboard && this.wasm.simcore_web_set_pasteboard(this.#put(JSON.stringify(items))) === 1;
  }
  /** Strings the app put on the pasteboard (the share sheet's Copy) since the last call, or null. */
  takePasteboard() {
    return this.wasm.simcore_web_take_pasteboard?.() > 0 ? JSON.parse(dec.decode(this.#take())) : null;
  }

  /** Dynamic Type size: xSmall ... large (default) ... xxxLarge, accessibility1 ... accessibility5. */
  setDynamicType(size) {
    const i = DYNAMIC_TYPE.indexOf(size);
    if (i < 0) throw new Error(`unknown dynamic type size ${size}`);
    return this.wasm.simcore_web_set_dynamic_type(i) === 0;
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
  /** Finger drag: phase "begin" (touch-down point) | "move" | "end"; points, `t` event time in ms. */
  drag(phase, x, y, t) {
    const p = { begin: 0, move: 1, end: 2 }[phase];
    if (p === undefined) throw new Error(`drag phase must be begin/move/end, got ${phase}`);
    return this.wasm.simcore_web_drag(p, x, y, t) === 1;
  }
  /** Coalesced finger-drag sample: updates drag state without rebuilding the scene. */
  dragCoalesced(x, y, t) { return this.wasm.simcore_web_drag(3, x, y, t) === 1; }
  /** Advance momentum/bounce by `ms`. True while moving: call again next frame. */
  tick(ms) { return this.wasm.simcore_web_tick(ms) === 1; }
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
  /** SwiftUI scene phase: "active", "inactive" or "background" (steps through inactive, as iOS does). */
  setScenePhase(phase) {
    const i = SCENE_PHASES.indexOf(phase);
    if (i < 0) throw new Error("scene phase must be 'active', 'inactive' or 'background'");
    if (typeof this.wasm.simcore_web_set_scene_phase !== "function") return false;   // older app.wasm
    return this.wasm.simcore_web_set_scene_phase(i) === 0;
  }
  get scenePhase() { return SCENE_PHASES[this.wasm.simcore_web_scene_phase?.() ?? 0]; }
  /** Whether this build of app.wasm can emit display lists for the GPU presenter. */
  get supportsDisplayList() { return typeof this.wasm.simcore_web_set_output === "function"; }
  /** Current frame version, when supported by the runtime. Reading it does not render. */
  get frameVersion() { return this.wasm.simcore_web_frame_version?.(); }
  /** Select what the runtime draws: 0 (or "pixels") = RGBA framebuffer, 1 (or "displaylist") = encoded display list. Resets the resource table. */
  setOutput(mode) {
    const m = mode === "displaylist" ? 1 : mode === "pixels" ? 0 : mode;
    if (this.wasm.simcore_web_set_output(m) !== 0) throw new Error(`unknown output mode ${mode}`);
  }
  /** Latest display list bytes (output mode 1): a view into Wasm memory, valid until the next draw. */
  displayList() {
    const ptr = this.wasm.simcore_web_dl_ptr(), len = this.wasm.simcore_web_dl_len();
    return new Uint8Array(this.mem, ptr, len);
  }
  setScale(scale) { this.scale = scale; this.wasm.simcore_web_set_scale(scale); }

  /** Current frame: RGBA bytes (a copy) and pixel size. */
  framebuffer() {
    const f = this.frameView();
    return { ...f, data: new Uint8Array(f.data) };
  }
  /** Current frame without copying: a view into Wasm memory, valid only until the next call. */
  frameView() {
    const w = this.wasm.simcore_web_fb_width(), h = this.wasm.simcore_web_fb_height();
    const ptr = this.wasm.simcore_web_fb_ptr();
    return { width: w, height: h, data: new Uint8ClampedArray(this.mem, ptr, w * h * 4) };
  }
}
