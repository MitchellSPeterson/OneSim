// Presentation backends for web/main.js: WebGPU first, CPU (putImageData) as the always-available fallback.
//
// The runtime (app.wasm) can produce either RGBA pixels (output mode 0) or an encoded display list (mode 1,
// executed by gpu/simcore_web_gpu_bg.wasm on a WebGPU canvas). State machine:
//
//   start --(renderer=cpu | no navigator.gpu | no display-list export)--> cpu
//   start --(GPU init ok)--> gpu --(render throws | device lost | init-time failure)--> cpu   (one way)
//
// A canvas hands out one context type for its whole life, so every switch to CPU after a `webgpu` context was
// requested swaps in a fresh <canvas> (same id/style). Input listeners live on the stage element wrapping it.

const KEY = "simcore.renderer";

/** `?renderer=cpu|gpu|auto`, else localStorage `simcore.renderer`, else auto. */
export function preferredRenderer(q) {
  let v = q.get("renderer");
  if (v == null) { try { v = localStorage.getItem(KEY); } catch { /* storage may be blocked */ } }
  return v === "cpu" || v === "gpu" ? v : "auto";
}

export class Presenter {
  /** `sim`: the SimCore driver; `stage`: element holding the canvas; `canvas`: the initial canvas. */
  constructor({ sim, stage, canvas, preference = "auto", diag = null, log = console }) {
    Object.assign(this, { sim, stage, canvas, preference, diag, log });
    this.mode = "cpu";
    this.reason = "";
    this.gpu = null;
    this.ctx = null;
    this.frames = 0;
    this.tainted = false; // the canvas has (had) a webgpu context: it cannot give a 2d one
    this.watch = 0;
  }

  /** Choose a backend. Call before `sim.init` so the first frame is already in the right form. Never throws. */
  async start() {
    if (this.preference === "cpu") return this.#report("cpu", "renderer=cpu requested");
    if (!navigator.gpu) return this.#report("cpu", "WebGPU unavailable (navigator.gpu missing)");
    if (!this.sim.supportsDisplayList) return this.#report("cpu", "app.wasm has no display-list output (rebuild with scripts/build-web.sh)");
    try {
      const m = await import("./gpu/simcore_web_gpu.js");
      await m.default({ module_or_path: new URL("./gpu/simcore_web_gpu_bg.wasm", import.meta.url) });
      this.tainted = true;
      this.gpu = await m.init(this.canvas);
      this.sim.setOutput(1);
      this.mode = "gpu";
      this.#report("gpu", "");
      this.watch = setInterval(() => { if (this.gpu?.lost()) this.fallback(`GPU device lost: ${this.gpu.error()}`); }, 1000);
    } catch (e) {
      this.fallback(`GPU init failed: ${e?.message ?? e}`);
    }
  }

  /** Show the runtime's current frame. Any GPU failure switches to CPU and shows the frame that way. */
  present() {
    if (this.mode === "gpu") {
      try {
        const dl = this.sim.displayList();
        this.gpu.render(dl);
        if (this.gpu.lost()) throw new Error(`GPU device lost: ${this.gpu.error()}`);
        this.frames++;
        return;
      } catch (e) {
        this.fallback(`GPU render failed: ${e?.message ?? e}`);
      }
    }
    this.#presentCpu();
  }

  /** Switch to CPU pixels for good (device loss, errors) and redraw. */
  fallback(reason) {
    clearInterval(this.watch);
    if (this.gpu) { try { this.gpu.free(); } catch { /* already unusable */ } this.gpu = null; }
    try { this.sim.setOutput(0); } catch (e) { this.log.error("simcore: cannot switch runtime to CPU output", e); }
    if (this.tainted) this.#freshCanvas();
    this.ctx = null;
    this.mode = "cpu";
    this.#report("cpu", reason);
    if (this.sim.width) this.#presentCpu();
  }

  #presentCpu() {
    const { width, height, data } = this.sim.frameView(); // putImageData copies it out at once
    if (!width) return;
    if (this.canvas.width !== width || this.canvas.height !== height) { this.canvas.width = width; this.canvas.height = height; }
    this.ctx ??= this.canvas.getContext("2d");
    this.ctx.putImageData(new ImageData(data, width, height), 0, 0);
    this.frames++;
  }

  #freshCanvas() {
    const old = this.canvas, c = document.createElement("canvas");
    c.id = old.id; c.className = old.className; c.style.cssText = old.style.cssText;
    c.width = old.width; c.height = old.height;
    old.replaceWith(c);
    this.canvas = c;
    this.tainted = false;
  }

  #report(mode, reason) {
    this.mode = mode; this.reason = reason;
    const info = this.gpu ? JSON.parse(this.gpu.info()) : null;
    const text = mode === "gpu"
      ? `renderer: WebGPU (${info.adapter || "adapter"}, ${info.backend}${info.limitations ? `; ${info.limitations}` : ""})`
      : `renderer: CPU${reason ? ` - ${reason}` : ""}`;
    (reason && this.preference !== "cpu" ? this.log.warn : this.log.info).call(this.log, `simcore: ${text}`);
    if (this.diag) this.diag.textContent = text;
    this.info = info;
  }
}
