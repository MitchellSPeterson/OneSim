// OneSim in the browser: draws the framebuffer of web/onesim.js's driver to a <canvas> and turns
// DOM events (pointer, wheel, drag, keyboard, resize) into points-space input.
import { load } from "./onesim.js";
import { Presenter, preferredRenderer } from "./renderer.js";

// Input listeners sit on the stage, not the canvas: the presenter may swap the canvas element when it
// falls back from WebGPU to the 2D context (web/renderer.js).
const stage = document.getElementById("stage");
const diag = document.getElementById("diag");
const q = new URLSearchParams(location.search);
const num = (k) => (q.has(k) && Number.isFinite(Number(q.get(k))) && Number(q.get(k)) > 0 ? Number(q.get(k)) : null);

function viewportFromUrl(devices, defaultName) {
  const name = q.get("device") ?? defaultName;
  const dev = devices.find((d) => d.name === name);
  if (!dev) throw new Error(`unknown device ${JSON.stringify(name)} (known: ${devices.map((d) => d.name).join(", ")})`);
  return { device: name, width: num("width") ?? dev.width, height: num("height") ?? dev.height, scale: num("scale") };
}

async function main() {
  const sim = await load(await fetch("app.wasm"));
  const vp = viewportFromUrl(sim.devices, sim.defaultDevice.name);
  const dark = q.has("dark") ? q.get("dark") !== "0" : matchMedia("(prefers-color-scheme: dark)").matches;
  const presenter = new Presenter({ sim, stage, canvas: document.getElementById("screen"), preference: preferredRenderer(q), diag });
  if (q.has("diag")) diag.hidden = false;
  await presenter.start(); // before init: the first frame is made in the chosen form
  const dpr = () => window.devicePixelRatio || 1;

  // Fit the canvas into the window keeping the aspect ratio; render at the physical resolution
  // (CSS size x devicePixelRatio) unless ?scale= pins it.
  let cssScale = 1; // CSS px per point
  function fit() {
    const w = sim.width || vp.width, h = sim.height || vp.height; // before init() the sim has no size yet
    cssScale = Math.min(innerWidth / w, innerHeight / h);
    presenter.canvas.style.width = `${w * cssScale}px`;
    presenter.canvas.style.height = `${h * cssScale}px`;
    return vp.scale ?? Math.min(4, Math.max(1, Math.round(cssScale * dpr() * 4) / 4));
  }
  const present = () => presenter.present();
  // Run an input/environment change, then show the new frame.
  // Animation clock: after any input, run frames while momentum/bounce is moving.
  let ticking = false;
  const animate = () => {
    if (ticking) return;
    ticking = true;
    let last = performance.now();
    const frame = (now) => {
      const moved = sim.tick(Math.min(now - last, 100)); // a stalled tab must not teleport
      last = now;
      if (moved) present();
      if (moved) requestAnimationFrame(frame); else ticking = false;
    };
    requestAnimationFrame(frame);
  };
  const act = (f) => { const r = f(); present(); animate(); return r; };

  sim.init({ device: vp.device, width: vp.width, height: vp.height, scale: fit(), dark, dynamicType: q.get("dynamicType") ?? "large" });
  present();
  let resizing = 0;
  addEventListener("resize", () => {
    cancelAnimationFrame(resizing);
    resizing = requestAnimationFrame(() => { const s = fit(); if (s !== sim.scale) act(() => sim.setScale(s)); });
  });

  const toPoints = (e) => {
    const r = presenter.canvas.getBoundingClientRect();
    return [((e.clientX - r.left) * sim.width) / r.width, ((e.clientY - r.top) * sim.height) / r.height];
  };

  // Pointer: down/up are taps; moving past a small threshold turns the gesture into a drag-scroll
  // (rubber band at the edges, momentum and bounce after lifting).
  let drag = null; // { id, x, y: touch-down point, scrolling }
  stage.addEventListener("pointerdown", (e) => {
    if (e.button > 0) return;
    stage.setPointerCapture(e.pointerId);
    const [x, y] = toPoints(e);
    drag = { id: e.pointerId, cx: e.clientX, cy: e.clientY, x, y, scrolling: false };
    act(() => sim.pointer("down", x, y));
  });
  stage.addEventListener("pointermove", (e) => {
    if (!drag || e.pointerId !== drag.id) return;
    if (!drag.scrolling && Math.hypot(e.clientX - drag.cx, e.clientY - drag.cy) < 8) return;
    if (!drag.scrolling) sim.drag("begin", drag.x, drag.y, e.timeStamp);
    drag.scrolling = true;
    for (const c of e.getCoalescedEvents?.() ?? [e]) {
      const [x, y] = toPoints(c);
      sim.drag("move", x, y, c.timeStamp);
    }
    present();
  });
  const lift = (e) => {
    if (!drag || e.pointerId !== drag.id) return;
    const wasScroll = drag.scrolling;
    drag = null;
    const [x, y] = toPoints(e);
    if (wasScroll) act(() => sim.drag("end", x, y, e.timeStamp));
    else if (e.type === "pointerup") act(() => sim.pointer("up", x, y)); // a cancelled touch is no tap
  };
  stage.addEventListener("pointerup", lift);
  stage.addEventListener("pointercancel", lift);

  // Wheel: CSS pixels -> points; positive deltaY reveals later content, like native.
  stage.addEventListener("wheel", (e) => {
    e.preventDefault();
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? sim.height : 1 / cssScale;
    const [x, y] = toPoints(e);
    act(() => sim.scroll(x, y, e.deltaX * unit, e.deltaY * unit));
  }, { passive: false });

  // Physical keyboard, only while a text field is focused (the on-canvas keyboard shows).
  addEventListener("keydown", (e) => {
    if (!sim.focused || e.ctrlKey || e.metaKey || e.altKey || e.isComposing) return;
    if (e.key === "Backspace") act(() => sim.key("backspace"));
    else if (e.key === "Enter") act(() => sim.key("enter"));
    else if ([...e.key].length === 1) act(() => sim.typeByUser(e.key));
    else return;
    e.preventDefault();
  });

  // Debug/test hooks.
  window.onesim = window.simcore = {
    ready: true,
    lastTreeText: () => sim.lastTreeText(),
    inspect: () => sim.inspect(),
    setViewport: (w, h) => act(() => {
      const changed = sim.setViewport(w, h);
      const scale = fit();
      if (scale !== sim.scale) sim.setScale(scale);
      return changed;
    }),
    setAppearance: (mode) => act(() => sim.setAppearance(mode)),
    setDark: (d) => act(() => sim.setAppearance(d ? "dark" : "light")),
    pointer: (kind, x, y) => act(() => sim.pointer(["down", "up", "click"][kind] ?? kind, x, y)), // points
    scroll: (x, y, dx, dy) => act(() => sim.scroll(x, y, dx, dy)),
    drag: (phase, x, y, t) => act(() => sim.drag(phase, x, y, t)),
    type: (text) => act(() => sim.type(text)),
    key: (name) => act(() => sim.key(name)),
    click: (label) => act(() => sim.clickLabel(label)),
    find: (label) => sim.find(label),
    sim,
  };
  // Which backend is showing and why: mode "gpu" | "cpu", `reason` = why it is CPU when WebGPU was not used.
  window.onesimRenderer = window.simcoreRenderer = {
    get mode() { return presenter.mode; },
    get reason() { return presenter.reason; },
    get requested() { return presenter.preference; },
    get info() { return presenter.info; },
    get frames() { return presenter.frames; },
    get canvas() { return presenter.canvas; },
    fallback: (reason = "forced") => presenter.fallback(reason),
  };
}

main().catch((e) => { document.getElementById("err").textContent = String(e.stack || e); console.error(e); });
