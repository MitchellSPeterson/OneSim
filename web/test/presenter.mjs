#!/usr/bin/env node
// Browser test of the GPU-first presenter (web/renderer.js + target/web/gpu), Node >= 22, no dependencies.
// Needs Chrome/Chromium (CHROME=/path, default macOS Google Chrome) and a built GPU module:
//   cargo build -p onesim-web-gpu --target wasm32-unknown-unknown --release && wasm-bindgen --target web --no-typescript \
//     --out-dir target/web/gpu --out-name onesim_web_gpu target/wasm32-unknown-unknown/release/onesim_web_gpu.wasm
// (scripts/build-web.sh does this too). It writes fixtures with `cargo test -p onesim-web`, serves them with the
// page from web/test/, drives headless Chrome over CDP and checks:
//   gpu       WebGPU renders a showcase frame, non-blank, close to the CPU pixels (needs a WebGPU adapter; SKIP otherwise)
//   cpu       ?renderer=cpu          -> CPU, same pixels as the runtime's
//   nogpu     navigator.gpu undefined -> CPU, reason says so
//   badlist   GPU render throws       -> falls back to CPU, canvas replaced, frame still correct
//   oldwasm   runtime without display-list output -> CPU
//   noadapter adapter request returns null -> CPU with the same pixels
//   resize    several GPU canvas backing sizes and DPRs
//   lost      destroy the WebGPU device -> CPU with the latest runtime state
//   --app     repeat lifecycle and idle checks with the built SwiftUI showcase
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, readFileSync, existsSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve, extname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const chrome = process.env.CHROME ?? ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "/usr/bin/chromium", "/usr/bin/google-chrome"].find(existsSync);
if (!chrome) { console.log("SKIP: no Chrome/Chromium (set CHROME=)"); process.exit(0); }
const appDir = join(process.env.CARGO_TARGET_DIR ?? join(root, "target"), "web");
const gpuDir = join(appDir, "gpu");
const testApp = process.argv.includes("--app");
if (testApp && !existsSync(join(appDir, "app.wasm"))) {
  console.error(`missing ${appDir}/app.wasm: build examples/swiftui-showcase first`);
  process.exit(2);
}
if (!existsSync(join(gpuDir, "onesim_web_gpu_bg.wasm"))) { console.error(`missing ${gpuDir}: build the GPU module first (see header)`); process.exit(2); }

const fx = mkdtempSync(join(tmpdir(), "onesim-fx-"));
execFileSync("cargo", ["test", "-q", "-p", "onesim-web", "write_browser_fixtures"], { cwd: root, env: { ...process.env, SIMCORE_WEB_FIXTURE_DIR: fx }, stdio: "inherit" });

const types = { ".js": "text/javascript", ".mjs": "text/javascript", ".html": "text/html", ".wasm": "application/wasm", ".json": "application/json" };
const server = createServer((req, res) => {
  const p = new URL(req.url, "http://x").pathname;
  const file = p.startsWith("/fx/") ? join(fx, p.slice(4)) : p.startsWith("/gpu/") ? join(gpuDir, p.slice(5))
    : p.startsWith("/app/") ? join(appDir, p.slice(5))
    : ["/renderer.js", "/onesim.js"].includes(p) ? join(root, "web", p) : join(root, "web/test", p);
  if (!existsSync(file) || !statSync(file).isFile()) { res.writeHead(404).end(); return; }
  res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" }).end(readFileSync(file));
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}`;

const profile = mkdtempSync(join(tmpdir(), "onesim-chrome-"));
const proc = spawn(chrome, ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--no-first-run", "--enable-unsafe-webgpu",
  "--enable-features=Vulkan,WebGPU", "--ignore-gpu-blocklist", "--window-size=500,900", "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
const wsUrl = await new Promise((ok, no) => {
  let buf = "";
  proc.stderr.on("data", (d) => { buf += d; const m = buf.match(/ws:\/\/\S+/); if (m) ok(m[0]); });
  setTimeout(() => no(new Error(`Chrome did not start:\n${buf}`)), 20000);
});

// Minimal CDP client over the browser endpoint.
const ws = new WebSocket(wsUrl);
await new Promise((r) => ws.addEventListener("open", r));
let id = 0; const pending = new Map();
ws.addEventListener("message", (e) => { const m = JSON.parse(e.data); pending.get(m.id)?.(m); });
const send = (method, params = {}, sessionId) => new Promise((ok, no) => {
  const n = ++id;
  pending.set(n, (m) => (m.error ? no(new Error(`${method}: ${m.error.message}`)) : ok(m.result)));
  ws.send(JSON.stringify({ id: n, method, params, sessionId }));
});

async function run(query, { preload = "", page = "presenter.html" } = {}) {
  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
  await send("Page.enable", {}, sessionId);
  if (preload) await send("Page.addScriptToEvaluateOnNewDocument", { source: preload }, sessionId);
  await send("Page.navigate", { url: `${base}/${page}?${query}` }, sessionId);
  const t0 = Date.now();
  for (;;) {
    const r = await send("Runtime.evaluate", { expression: "JSON.stringify(window.__result ?? null)", returnByValue: true }, sessionId);
    if (r.result.value && r.result.value !== "null") { await send("Target.closeTarget", { targetId }); return JSON.parse(r.result.value); }
    if (Date.now() - t0 > 60000) throw new Error(`timeout: ${query}`);
    await new Promise((r) => setTimeout(r, 200));
  }
}

let failed = 0;
const check = (ok, msg) => { if (!ok) { failed++; console.error(`  FAIL ${msg}`); } };
const show = (n, r) => console.log(`${n}: mode=${r.mode} diff=${r.diffPct.toFixed(3)}% nonblank=${r.nonBlankPct.toFixed(1)}% size=${r.size} swapped=${r.canvasSwapped}${r.reason ? ` reason="${r.reason}"` : ""}${r.info ? ` adapter="${r.info.adapter}/${r.info.backend}"` : ""}`);
const trackDevice = `
  const requestAdapter = navigator.gpu.requestAdapter.bind(navigator.gpu);
  navigator.gpu.requestAdapter = async (...options) => {
    const adapter = await requestAdapter(...options);
    if (!adapter) return adapter;
    const requestDevice = adapter.requestDevice.bind(adapter);
    adapter.requestDevice = async (...options) => {
      window.__testDevice = await requestDevice(...options);
      return window.__testDevice;
    };
    return adapter;
  };
`;
async function probeApp() {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  while (!window.onesim?.ready) {
    const error = document.getElementById("err")?.textContent;
    if (error) throw new Error(error);
    await sleep(25);
  }
  const s = window.onesim, r = window.onesimRenderer;
  const mode = r.mode, initialFrames = r.frames;
  const size = () => [r.canvas.width, r.canvas.height].join("x");
  const initialSize = size();
  const idleStart = performance.now();
  await sleep(1200);
  const idleMs = performance.now() - idleStart, idleFrames = r.frames - initialFrames;
  s.click("Todos");
  s.click("New todo");
  s.type("Buy milk");
  const beforeLoss = s.lastTreeText();
  const canvas = r.canvas;
  if (window.__testDevice) {
    window.__testDevice.destroy();
    const deadline = Date.now() + 4000;
    while (r.mode === "gpu" && Date.now() < deadline) await sleep(25);
  }
  const stateRetained = beforeLoss === s.lastTreeText() && s.sim.focused;
  s.click("Add");
  const beforeResize = r.frames;
  s.setViewport(320, 550);
  const resizeFrames = r.frames - beforeResize;
  const resized = size();
  s.setAppearance("dark");
  return {
    mode, finalMode: r.mode, reason: r.reason, initialSize, resized, resizeFrames, idleMs, idleFrames,
    stateRetained, canvasSwapped: r.canvas !== canvas, todoAdded: s.lastTreeText().includes("Buy milk"),
  };
}
async function liveApp(device, scale, gpu) {
  const preload = gpu ? trackDevice : "";
  const probe = `(${probeApp})().then(result => { window.__result = result; }, error => { window.__result = { error: String(error) }; });`;
  return run(`device=${device}&scale=${scale}&renderer=${gpu ? "gpu" : "cpu"}`, { preload: `${preload}\n${probe}`, page: "app/index.html" });
}

const cpuOk = (r) => { check(r.mode === "cpu" && r.output === 0, "mode is cpu"); check(r.diffPct === 0, `CPU pixels exact (diff ${r.diffPct})`); check(r.nonBlankPct > 5, "not blank"); check(r.canvasConnected, "canvas attached"); };

try {
  const gpu = await run("renderer=gpu&fixture=home");
  if (gpu.mode === "gpu") {
    show("gpu", gpu);
    check(gpu.diffPct < 2.5, `GPU vs CPU diff ${gpu.diffPct}% < 2.5%`);
    check(gpu.nonBlankPct > 5, "GPU frame not blank");
    for (const f of ["controls", "home-tabbar"]) { const r = await run(`renderer=auto&fixture=${f}`); show(`gpu ${f}`, r); check(r.mode === "gpu" && r.diffPct < 2.5 && r.nonBlankPct > 5, `${f} on GPU (${r.reason})`); }
    const bad = await run("renderer=gpu&fixture=home&fail=render");
    show("badlist", bad); cpuOk(bad); check(bad.canvasSwapped && /render failed/.test(bad.reason), "render error swaps canvas and reports why");
    check(bad.frames === 1, "render error presents CPU pixels once");
    const resized = await run("renderer=gpu&fixture=home&scenario=resize");
    show("resize", resized);
    const [large, small, largeAgain, smallAgain] = resized.sizes;
    check(resized.mode === "gpu" && resized.frames === 4 && resized.diffPct < 2.5, "GPU pixels survive repeated resize");
    check(small.join("x") === "640x1136" && large.join("x") === largeAgain.join("x") && small.join("x") === smallAgain.join("x"), "canvas follows changing fixture size");
    const lost = await run("renderer=gpu&fixture=home&scenario=loss", { preload: trackDevice });
    show("lost", lost); cpuOk(lost);
    check(lost.canvasSwapped && /device lost/i.test(lost.reason) && lost.fixture === "controls" && lost.frames === 3, "device loss redraws the latest runtime state");
    const error = await run("renderer=gpu&fixture=home&scenario=error", { preload: trackDevice });
    show("device-error", error); cpuOk(error);
    check(error.canvasSwapped && /device error/i.test(error.reason) && error.frames === 3, "uncaptured device error switches to CPU");
  } else {
    const unavailable = /navigator\.gpu missing|no WebGPU adapter|canvas WebGPU context.*(unsupported|not supported|webgpu not available)/i.test(gpu.reason);
    if (unavailable) console.log(`SKIP gpu: ${gpu.reason}`);
    else { show("gpu", gpu); check(false, "GPU initialization failed unexpectedly"); }
    cpuOk(gpu);
  }
  const cpu = await run("renderer=cpu&fixture=home"); show("cpu", cpu); cpuOk(cpu); check(!cpu.canvasSwapped, "no canvas swap when GPU never tried");
  const nogpu = await run("fixture=home", { preload: "Object.defineProperty(navigator, 'gpu', { value: undefined, configurable: true });" });
  show("nogpu", nogpu); cpuOk(nogpu); check(/navigator\.gpu/.test(nogpu.reason), "reason mentions navigator.gpu");
  if (!/navigator\.gpu missing/.test(gpu.reason)) {
    const noadapter = await run("fixture=home", { preload: "if (navigator.gpu) navigator.gpu.requestAdapter = async () => null;" });
    show("noadapter", noadapter); cpuOk(noadapter);
    check(/no WebGPU adapter|canvas WebGPU context/.test(noadapter.reason), "unavailable WebGPU reports why");
  }
  if (gpu.mode === "gpu") {
    const noCanvas = await run("fixture=home", { preload: `
      const getContext = HTMLCanvasElement.prototype.getContext;
      HTMLCanvasElement.prototype.getContext = function (type, ...options) {
        return type === "webgpu" ? null : getContext.call(this, type, ...options);
      };
    ` });
    show("no-canvas", noCanvas); cpuOk(noCanvas);
    check(/canvas WebGPU context.*webgpu not available/i.test(noCanvas.reason), "missing canvas WebGPU context reports why");
  }
  const old = await run("fixture=home&old=1"); show("oldwasm", old); cpuOk(old); check(/display-list/.test(old.reason) || old.reason.includes("GPU"), "old runtime reason");
  if (testApp) {
    for (const [device, scale, expected] of [["iphone-se-3", 2, "750x1334"], ["iphone-17-pro", 3, "1206x2622"]]) {
      for (const useGpu of gpu.mode === "gpu" ? [false, true] : [false]) {
        const live = await liveApp(device, scale, useGpu);
        console.log(`app ${device} @${scale}x ${JSON.stringify(live)}`);
        check(live.mode === (useGpu ? "gpu" : "cpu") && live.finalMode === "cpu", "live app selects and recovers its presenter");
        check(live.initialSize === expected && live.resized === `${320 * scale}x${550 * scale}` && live.resizeFrames === 1, "live resize presents one correctly sized frame");
        check(live.idleFrames === 0, "static live app presents no idle frames");
        check(live.stateRetained && live.todoAdded, "live app retains edited text and focus and accepts input");
        if (useGpu) check(live.canvasSwapped && /device lost/i.test(live.reason), "live device loss swaps the canvas");
      }
    }
  }
} finally {
  proc.kill(); server.close(); ws.close();
}
console.log(failed ? `${failed} check(s) failed` : "ok");
process.exit(failed ? 1 : 0);
