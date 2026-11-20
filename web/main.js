// OneSim in the browser: draws the framebuffer of web/onesim.js's driver to a <canvas> and turns
// DOM events (pointer, wheel, drag, keyboard, resize) into points-space input.
// ?metro=URL or ?bundle=URL (a `scripts/build-web.sh --expo` site): an Expo / React Native app whose JavaScript runs in
// a Web Worker (web/expo/guest.js); its trees are drawn here and input goes back to it as messages.
import { load } from "./onesim.js";
import { Presenter, preferredRenderer } from "./renderer.js";

// Input listeners sit on the stage, not the canvas: the presenter may swap the canvas element when it
// falls back from WebGPU to the 2D context (web/renderer.js).
const stage = document.getElementById("stage");
const diag = document.getElementById("diag");
const loader = document.getElementById("loader");
const q = new URLSearchParams(location.search);
const num = (k) => (q.has(k) && Number.isFinite(Number(q.get(k))) && Number(q.get(k)) > 0 ? Number(q.get(k)) : null);

function formatDeviceName(name) {
  if (name === "iphone-se-3") return "iPhone SE (3rd gen)";
  if (name === "iphone-se-2") return "iPhone SE (2nd gen)";
  return name.split("-").map((part) => {
    if (part === "iphone") return "iPhone";
    if (part === "ipad") return "iPad";
    if (part === "pixel") return "Pixel";
    if (/^\d+[a-z]$/.test(part)) return part;
    return part.charAt(0).toUpperCase() + part.slice(1);
  }).join(" ");
}

function viewportFromUrl(devices, defaultName) {
  const name = q.get("device") ?? defaultName;
  const dev = devices.find((d) => d.name === name);
  if (!dev) throw new Error(`unknown device ${JSON.stringify(name)} (known: ${devices.map((d) => d.name).join(", ")})`);
  return { device: name, width: num("width") ?? dev.width, height: num("height") ?? dev.height, scale: num("scale"), dev };
}

function initHeaderChrome() {
  const index = document.body.dataset.index;
  const title = q.get("title") || document.body.dataset.title;
  const framework = q.get("framework") || document.body.dataset.framework;
  if (index) {
    const back = document.getElementById("back");
    back.href = index;
    back.hidden = false;
  }
  if (title) {
    document.title = `${title} — OneSim`;
    document.getElementById("meta-name").textContent = title;
    const fwEl = document.getElementById("meta-fw");
    if (framework) {
      document.body.dataset.framework = framework.toLowerCase();
      fwEl.textContent = framework;
      fwEl.hidden = false;
    } else {
      fwEl.hidden = true;
    }
    document.getElementById("app-meta").hidden = false;
  }
}

async function main() {
  initHeaderChrome();
  const rn = q.has("metro") || q.has("bundle"), android = q.get("platform") === "android";
  // React Native mode never calls the module's JS-guest imports (Compose's); the Worker is the guest.
  const sim = await load(await fetch("app.wasm"), rn ? { guest: {} } : {});
  const devices = sim.devices;
  const vp = viewportFromUrl(devices, rn ? devices.find((d) => d.android === android).name : sim.defaultDevice.name);
  const platform = rn ? (android ? "android" : "ios") : sim.platform;
  stage.dataset.platform = platform; // picks the iPhone or Pixel frame in index.html
  document.body.dataset.platform = platform;
  if (vp.device.startsWith("iphone-se")) stage.dataset.classic = "true";

  let dark = document.documentElement.dataset.theme === "dark";
  const applyPageTheme = (isDark) => {
    dark = isDark;
    document.documentElement.dataset.theme = isDark ? "dark" : "light";
    const btn = document.getElementById("theme-btn");
    const next = isDark ? "Switch to light mode" : "Switch to dark mode";
    btn.title = next;
    btn.setAttribute("aria-label", next);
  };
  applyPageTheme(dark);

  const presenter = new Presenter({ sim, stage, canvas: document.getElementById("screen"), preference: preferredRenderer(q), diag });
  const rendererBtn = document.getElementById("renderer-btn");
  const rendererDot = document.getElementById("renderer-dot");
  const rendererText = document.getElementById("renderer-text");
  let shownMode = "";
  const syncRendererUi = () => {
    if (shownMode === presenter.mode) return;
    shownMode = presenter.mode;
    rendererDot.dataset.mode = presenter.mode;
    rendererText.textContent = presenter.mode === "gpu" ? "WebGPU" : "CPU";
    rendererBtn.title = diag.textContent || `Renderer: ${rendererText.textContent}`;
  };
  if (q.has("diag")) {
    diag.hidden = false;
    rendererBtn.setAttribute("aria-pressed", "true");
  }
  rendererBtn.addEventListener("click", () => {
    diag.hidden = !diag.hidden;
    rendererBtn.setAttribute("aria-pressed", String(!diag.hidden));
  });

  // Device selector / label filtered to this build's platform.
  const platformDevices = devices.filter((d) => (platform === "android" ? d.android : !d.android));
  const deviceWrap = document.getElementById("device-wrap");
  const deviceSep = document.getElementById("device-sep");
  const deviceLabel = document.getElementById("device-label");
  if (platformDevices.length > 0) {
    deviceLabel.textContent = formatDeviceName(vp.device);
    deviceLabel.hidden = false;
    deviceWrap.hidden = false;
    deviceSep.hidden = false;
  }
  if (platformDevices.length > 1) {
    const select = document.getElementById("device-select");
    for (const d of platformDevices) {
      const opt = document.createElement("option");
      opt.value = d.name;
      opt.textContent = formatDeviceName(d.name);
      if (d.name === vp.device) opt.selected = true;
      select.append(opt);
    }
    select.hidden = false;
    deviceWrap.classList.add("interactive");
    select.addEventListener("change", () => {
      const next = new URLSearchParams(location.search);
      next.set("device", select.value);
      location.search = next.toString();
    });
  }

  await presenter.start(); // before init: the first frame is made in the chosen form
  syncRendererUi();
  const dpr = () => window.devicePixelRatio || 1;

  // Fit the canvas into the window keeping the aspect ratio; render at the physical resolution
  // (CSS size x devicePixelRatio) unless ?scale= pins it.
  let cssScale = 1; // CSS px per point
  function fit() {
    const w = sim.width || vp.width, h = sim.height || vp.height; // before init() the sim has no size yet
    // Leave room for the phone frame (bezel + hardware buttons) and the top studio toolbar.
    const fw = w + 46, fh = h + (stage.dataset.classic ? 104 : 38);
    cssScale = Math.max(0.2, Math.min(1, (innerWidth - 40) / fw, (innerHeight - 116) / fh));
    stage.style.setProperty("--s", cssScale);
    presenter.canvas.style.width = `${w * cssScale}px`;
    presenter.canvas.style.height = `${h * cssScale}px`;
    return vp.scale ?? Math.min(4, Math.max(1, Math.round(cssScale * dpr() * 4) / 4));
  }
  let shown = !rn; // React Native: nothing to show before the guest's first tree
  let flush = () => {}; // React Native: post the host messages input produced to the guest
  let trees = 0; // React Native: trees received (tests wait for them to stop)
  const present = () => {
    if (shown) {
      presenter.present();
      syncRendererUi();
      if (loader && !loader.hidden) loader.hidden = true;
    }
    flush();
  };
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
      if (moved) present(); else flush();
      if (moved) requestAnimationFrame(frame); else ticking = false;
    };
    requestAnimationFrame(frame);
  };
  const act = (f) => { const r = f(); present(); animate(); return r; };

  if (rn) {
    sim.initRn({ device: vp.device, width: vp.width, height: vp.height, scale: fit(), dark, android });
    const ios = q.get("ios") ?? "27.2";
    if (!android) sim.wasm.simcore_web_set_ios(ios.startsWith("26") ? 0 : 1);
    const { startGuest } = await import("./expo/guest.js");
    const env = {
      SIMCORE_PLATFORM: android ? "android" : "ios", SIMCORE_DEVICE: vp.device, SIMCORE_OS_VERSION: android ? "17" : ios,
      SIMCORE_VIEWPORT: `${vp.width}x${vp.height}`, SIMCORE_SCALE: String(vp.dev.scale), SIMCORE_APPEARANCE: dark ? "dark" : "light",
      SIMCORE_INSETS: `${vp.dev.safeTop},${vp.dev.safeBottom}`, SIMCORE_DYNAMIC_TYPE: q.get("dynamicType") ?? "large",
      ...(q.has("metro") ? { SIMCORE_METRO_URL: q.get("metro") } : { SIMCORE_BUNDLE_URL: q.get("bundle"), SIMCORE_HMR: "0" }),
    };
    const guest = startGuest({
      workerUrl: new URL("guest/browser-worker.js", location.href),
      env,
      onMessage(m) {
        if (m.type === "hello" || m.type === "tree" || m.type === "focus_request" || m.type === "scroll_to") {
          if (m.type === "tree") trees++;
          try { if (sim.receive(m)) shown = true; } catch (e) { console.warn("onesim:", e.message); }
          present();
        } else if (m.type === "error") console.error("guest:", m.message);
        else if (m.type === "event") console.debug("guest event", m.name, m.detail);
      },
      onFile(path, bytes) { if (sim.putImage(path, bytes)) present(); },
    });
    flush = () => { for (const m of sim.outbox()) guest.post(m); };
    window.onesimGuest = guest;
  } else {
    sim.init({ device: vp.device, width: vp.width, height: vp.height, scale: fit(), dark, dynamicType: q.get("dynamicType") ?? "large" });
  }
  present();

  document.getElementById("theme-btn").addEventListener("click", () => {
    const nextDark = !dark;
    applyPageTheme(nextDark);
    try { localStorage.setItem("onesim.theme", nextDark ? "dark" : "light"); } catch {}
    act(() => sim.setAppearance(nextDark ? "dark" : "light"));
  });

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
  const composeLinks = !rn && sim.platform === "android";
  if (composeLinks) stage.tabIndex = 0;
  let drag = null; // { id, x, y: touch-down point, scrolling }
  stage.addEventListener("pointerdown", (e) => {
    if (e.button > 0) return;
    if (composeLinks) stage.focus({ preventScroll: true });
    stage.setPointerCapture(e.pointerId);
    const [x, y] = toPoints(e);
    drag = { id: e.pointerId, cx: e.clientX, cy: e.clientY, x, y, scrolling: false };
    act(() => sim.pointer("down", x, y));
  });
  stage.addEventListener("pointermove", (e) => {
    if (!drag) {
      if (composeLinks && e.pointerType === "mouse") {
        const [x, y] = toPoints(e);
        act(() => sim.pointer("hover", x, y));
      }
      return;
    }
    if (e.pointerId !== drag.id) return;
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
    else if (e.type === "pointerup") act(() => sim.pointer("up", x, y));
    else if (composeLinks) act(() => sim.pointer("cancel", x, y));
  };
  stage.addEventListener("pointerup", lift);
  stage.addEventListener("pointercancel", lift);
  stage.addEventListener("pointerleave", () => {
    if (composeLinks) act(() => sim.pointer("hover", -1, -1));
  });

  // Wheel: CSS pixels -> points; positive deltaY reveals later content, like native.
  stage.addEventListener("wheel", (e) => {
    e.preventDefault();
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? sim.height : 1 / cssScale;
    const [x, y] = toPoints(e);
    act(() => sim.scroll(x, y, e.deltaX * unit, e.deltaY * unit));
  }, { passive: false });

  // Physical keyboard input for Compose links and focused text fields.
  addEventListener("keydown", (e) => {
    if (!rn && sim.platform === "android" && (e.ctrlKey || e.metaKey) && !e.altKey && !e.isComposing) {
      const key = { c: "copy", a: "select_all" }[e.key.toLowerCase()];
      if (key && act(() => sim.key(key))) e.preventDefault();
      return;
    }
    if (e.ctrlKey || e.metaKey || e.altKey || e.isComposing) return;
    if (composeLinks && e.target === stage && !sim.focused) {
      const keys = { Tab: e.shiftKey ? "shift_tab" : "tab", Enter: "enter", Escape: "escape", " ": "space" };
      const key = keys[e.key];
      if (key && act(() => sim.keyIfFocused(key)) && (e.key !== "Tab" || sim.linkFocused)) e.preventDefault();
      return;
    }
    if (!sim.focused) return;
    if (e.key === "Backspace") act(() => sim.key("backspace"));
    else if (e.key === "Enter") act(() => sim.key("enter"));
    else if ([...e.key].length === 1) act(() => sim.typeByUser(e.key));
    else return;
    e.preventDefault();
  });

  // Debug/test hooks.
  window.onesim = window.simcore = {
    ready: true,
    get trees() { return trees; },
    lastTreeText: () => sim.lastTreeText(),
    inspect: () => sim.inspect(),
    setViewport: (w, h) => act(() => {
      const changed = sim.setViewport(w, h);
      const scale = fit();
      if (scale !== sim.scale) sim.setScale(scale);
      return changed;
    }),
    setAppearance: (mode) => { applyPageTheme(mode === "dark"); return act(() => sim.setAppearance(mode)); },
    setDark: (d) => { applyPageTheme(Boolean(d)); return act(() => sim.setAppearance(d ? "dark" : "light")); },
    pointer: (kind, x, y) => act(() => sim.pointer(["down", "up", "click"][kind] ?? kind, x, y)), // points
    scroll: (x, y, dx, dy) => act(() => sim.scroll(x, y, dx, dy)),
    drag: (phase, x, y, t) => act(() => sim.drag(phase, x, y, t)),
    type: (text) => act(() => sim.type(text)),
    key: (name) => act(() => sim.key(name)),
    click: (label) => act(() => sim.clickLabel(label)),
    find: (label) => sim.find(label),
    present,
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
    fallback: (reason = "forced") => { presenter.fallback(reason); syncRendererUi(); },
  };
}

main().catch((e) => {
  if (loader) loader.hidden = true;
  document.getElementById("err").textContent = String(e.stack || e);
  console.error(e);
});
