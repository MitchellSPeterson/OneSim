#!/usr/bin/env node
// Browser test of the GPU-first presenter (web/renderer.js + target/web/gpu), Node >= 22, no dependencies.
// Needs Chrome/Chromium (CHROME=/path, default macOS Google Chrome) and a built GPU module:
//   cargo build -p simcore-web-gpu --target wasm32-unknown-unknown --release && wasm-bindgen --target web --no-typescript \
//     --out-dir target/web/gpu --out-name simcore_web_gpu target/wasm32-unknown-unknown/release/simcore_web_gpu.wasm
// (scripts/build-web.sh does this too). It writes fixtures with `cargo test -p simcore-web`, serves them with the
// page from web/test/, drives headless Chrome over CDP and checks:
//   gpu       WebGPU renders a showcase frame, non-blank, close to the CPU pixels (needs a WebGPU adapter; SKIP otherwise)
//   cpu       ?renderer=cpu          -> CPU, same pixels as the runtime's
//   nogpu     navigator.gpu undefined -> CPU, reason says so
//   badlist   GPU render throws       -> falls back to CPU, canvas replaced, frame still correct
//   oldwasm   runtime without display-list output -> CPU
import { spawn, execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdtempSync, readFileSync, existsSync, statSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve, extname } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const chrome = process.env.CHROME ?? ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "/usr/bin/chromium", "/usr/bin/google-chrome"].find(existsSync);
if (!chrome) { console.log("SKIP: no Chrome/Chromium (set CHROME=)"); process.exit(0); }
const gpuDir = join(process.env.CARGO_TARGET_DIR ?? join(root, "target"), "web/gpu");
if (!existsSync(join(gpuDir, "simcore_web_gpu_bg.wasm"))) { console.error(`missing ${gpuDir}: build the GPU module first (see header)`); process.exit(2); }

const fx = mkdtempSync(join(tmpdir(), "simcore-fx-"));
execFileSync("cargo", ["test", "-q", "-p", "simcore-web", "write_browser_fixtures"], { cwd: root, env: { ...process.env, SIMCORE_WEB_FIXTURE_DIR: fx }, stdio: "inherit" });

const types = { ".js": "text/javascript", ".mjs": "text/javascript", ".html": "text/html", ".wasm": "application/wasm", ".json": "application/json" };
const server = createServer((req, res) => {
  const p = new URL(req.url, "http://x").pathname;
  const file = p.startsWith("/fx/") ? join(fx, p.slice(4)) : p.startsWith("/gpu/") ? join(gpuDir, p.slice(5))
    : ["/renderer.js", "/simcore.js"].includes(p) ? join(root, "web", p) : join(root, "web/test", p);
  if (!existsSync(file) || !statSync(file).isFile()) { res.writeHead(404).end(); return; }
  res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" }).end(readFileSync(file));
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}`;

const profile = mkdtempSync(join(tmpdir(), "simcore-chrome-"));
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

async function run(query, { preload = "" } = {}) {
  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
  await send("Page.enable", {}, sessionId);
  if (preload) await send("Page.addScriptToEvaluateOnNewDocument", { source: preload }, sessionId);
  await send("Page.navigate", { url: `${base}/presenter.html?${query}` }, sessionId);
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
  } else {
    console.log(`SKIP gpu: no WebGPU in this Chrome (${gpu.reason})`);
    cpuOk(gpu); // still must be a correct CPU frame, not blank
  }
  const cpu = await run("renderer=cpu&fixture=home"); show("cpu", cpu); cpuOk(cpu); check(!cpu.canvasSwapped, "no canvas swap when GPU never tried");
  const nogpu = await run("fixture=home", { preload: "Object.defineProperty(navigator, 'gpu', { value: undefined, configurable: true });" });
  show("nogpu", nogpu); cpuOk(nogpu); check(/navigator\.gpu/.test(nogpu.reason), "reason mentions navigator.gpu");
  const old = await run("fixture=home&old=1"); show("oldwasm", old); cpuOk(old); check(/display-list/.test(old.reason) || old.reason.includes("GPU"), "old runtime reason");
} finally {
  proc.kill(); server.close(); ws.close();
}
console.log(failed ? `${failed} check(s) failed` : "ok");
process.exit(failed ? 1 : 0);
