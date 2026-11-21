// Presentation backends for web/main.js: WebGPU first, CPU (putImageData) as the always-available fallback.
//
// The runtime (app.wasm) can produce either RGBA pixels (output mode 0) or an encoded display list (mode 1,
// executed by gpu/onesim_web_gpu_bg.wasm on a WebGPU canvas). State machine:
//
//   start --(renderer=cpu | no navigator.gpu | no display-list export)--> cpu
//   start --(GPU init ok)--> gpu --(render throws | device lost | init-time failure)--> cpu   (one way)
//
// A canvas hands out one context type for its whole life, so switching from WebGPU to CPU requires a fresh canvas.
// Input listeners live on the stage element wrapping it.

const KEY = "simcore.renderer";

/** `?renderer=cpu|gpu|auto`, else localStorage `simcore.renderer`, else auto. */
export function preferredRenderer(q) {
  let v = q.get("renderer");
  if (v == null) { try { v = localStorage.getItem(KEY); } catch { /* storage may be blocked */ } }
  return v === "cpu" || v === "gpu" ? v : "auto";
}

function freeGpu(gpu) {
  try { gpu?.free(); } catch { /* already unusable */ }
}

export class Presenter {
  /** `sim`: the OneSim driver; `stage`: element holding the canvas; `canvas`: the initial canvas. */
  constructor({ sim, stage, canvas, preference = "auto", diag = null, log = console, requestFrame = null }) {
    Object.assign(this, { sim, stage, canvas, preference, diag, log, requestFrame });
    this.mode = "cpu";
    this.reason = "";
    this.gpu = null;
    this.gpuMemory = null;
    this.ctx = null;
    this.imageData = null;
    this.frames = 0;
    this.tainted = false; // the canvas has (had) a webgpu context: it cannot give a 2d one
    this.watch = 0;
  }

  /** Choose a backend before `sim.init`. A pending runtime can load alongside WebGPU startup. */
  async start(runtime = this.sim) {
    const cpuReason = this.preference === "cpu" ? "renderer=cpu requested"
      : !navigator.gpu ? "WebGPU unavailable (navigator.gpu missing)" : "";
    const pending = typeof runtime.then === "function";
    const canPrepare = !cpuReason && (pending || runtime.supportsDisplayList);
    // Until we know the runtime supports display lists, keep its canvas available for a 2d context.
    const canvas = canPrepare && (pending ? this.canvas.cloneNode(false) : this.canvas);
    let prepared = canvas ? this.#prepareGpu(canvas) : null;
    try {
      this.sim = await runtime;
      if (cpuReason) this.#report("cpu", cpuReason);
      else if (!this.sim.supportsDisplayList) this.#report("cpu", "app.wasm has no display-list output (rebuild with scripts/build-web.sh)");
      else {
        const { gpu, gpuMemory, failure } = await prepared;
        prepared = null;
        this.gpu = gpu;
        this.gpuMemory = gpuMemory;
        try {
          if (!gpu) throw failure;
          this.sim.setOutput(1);
          if (canvas !== this.canvas) this.#replaceCanvas(canvas);
          this.tainted = true;
          this.#report("gpu", "");
          this.watch = setInterval(() => {
            const failure = this.#gpuFailure();
            if (failure) this.fallback(failure, this.requestFrame);
          }, 1000);
        } catch (e) {
          this.fallback(`GPU init failed: ${e?.message ?? e}`);
        }
      }
      return this.sim;
    } finally {
      // The runtime can fail or fall back without waiting for speculative GPU work to finish.
      prepared?.then(({ gpu }) => freeGpu(gpu));
    }
  }

  async #prepareGpu(canvas) {
    try {
      const m = await import("./gpu/onesim_web_gpu.js");
      const wasm = await m.default({ module_or_path: new URL("./gpu/onesim_web_gpu_bg.wasm", import.meta.url) });
      if (canvas === this.canvas) this.tainted = true;
      return { gpu: await m.init(canvas), gpuMemory: wasm?.memory ?? null };
    } catch (failure) {
      return { gpu: null, gpuMemory: null, failure };
    }
  }

  /** Show the runtime's current frame. Any GPU failure switches to CPU and shows the frame that way. */
  present() {
    if (this.mode === "gpu") {
      try {
        const dl = this.sim.displayList();
        if (this.gpuMemory && typeof this.gpu.alloc_dl === "function" && typeof this.gpu.render_dl === "function") {
          const ptr = this.gpu.alloc_dl(dl.length);
          new Uint8Array(this.gpuMemory.buffer, ptr, dl.length).set(dl);
          this.gpu.render_dl(dl.length);
        } else {
          this.gpu.render(dl);
        }
        const failure = this.#gpuFailure();
        if (failure) return this.fallback(failure);
        this.frames++;
        return;
      } catch (e) {
        return this.fallback(`GPU render failed: ${e?.message ?? e}`);
      }
    }
    this.#presentCpu();
  }

  /** Switch to CPU pixels for good (device loss, errors) and redraw. */
  fallback(reason, requestFrame = null) {
    clearInterval(this.watch);
    freeGpu(this.gpu);
    this.gpu = null;
    this.gpuMemory = null;
    try { this.sim.setOutput(0); } catch (e) { this.log.error("onesim: cannot switch runtime to CPU output", e); }
    if (this.tainted) {
      this.#replaceCanvas(this.canvas.cloneNode(false));
      this.tainted = false;
    }
    this.ctx = null;
    this.imageData = null;
    this.mode = "cpu";
    this.#report("cpu", reason);
    if (requestFrame) requestFrame();
    else if (this.sim.width) this.#presentCpu();
  }

  #gpuFailure() {
    if (!this.gpu) return "";
    if (this.gpu.lost()) return `GPU device lost: ${this.gpu.error()}`;
    const error = this.gpu.error();
    return error ? `GPU device error: ${error}` : "";
  }

  #presentCpu() {
    const { width, height, data } = this.sim.frameView(); // putImageData copies it out at once
    if (!width) return;
    if (this.canvas.width !== width || this.canvas.height !== height) { this.canvas.width = width; this.canvas.height = height; }
    this.ctx ??= this.canvas.getContext("2d");
    if (!this.imageData || this.imageData.data.buffer !== data.buffer || this.imageData.data.byteOffset !== data.byteOffset
      || this.imageData.width !== width || this.imageData.height !== height) {
      this.imageData = new ImageData(data, width, height);
    }
    this.ctx.putImageData(this.imageData, 0, 0);
    this.frames++;
  }

  #replaceCanvas(canvas) {
    this.canvas.replaceWith(canvas);
    this.canvas = canvas;
  }

  #report(mode, reason) {
    this.mode = mode; this.reason = reason;
    const info = this.gpu ? JSON.parse(this.gpu.info()) : null;
    const text = mode === "gpu"
      ? `renderer: WebGPU (${info.adapter || "adapter"}, ${info.backend}${info.limitations ? `; ${info.limitations}` : ""})`
      : `renderer: CPU${reason ? ` - ${reason}` : ""}`;
    (reason && this.preference !== "cpu" ? this.log.warn : this.log.info).call(this.log, `onesim: ${text}`);
    if (this.diag) this.diag.textContent = text;
    this.info = info;
  }
}
