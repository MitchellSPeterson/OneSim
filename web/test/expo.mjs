#!/usr/bin/env node
// Drive an Expo app in the browser build (scripts/build-web.sh --expo -> target/web-expo) in headless Chrome over CDP,
// like the native `onesim expo` CLI. Node >= 22, no dependencies; needs Chrome/Chromium (CHROME=/path) and Metro running.
//   node web/test/expo.mjs --metro URL | --bundle URL [--dir target/web-expo] [--platform android] [--ios 26.5|27.2]
//     [--device NAME] [--scale S] [--dark] [--renderer cpu|gpu] [--click LABEL]... [--type T] [--key K] [--scroll X,Y,DX,DY]
//     [--drag X,Y,DX,DY] [--expect TEXT]... [--screenshot out.png] [--inspect out.json]
// Steps run in command-line order; after the first tree and after every step it waits for the guest to go quiet
// (no tree for 300 ms), like the native host's settle. --expect checks the final tree text. Exit 1 on failure.
// --bundle is a page URL (served from --dir) or absolute; --metro is proxied same-origin by web/expo/serve.mjs.
import { spawn } from "node:child_process";
import { mkdtempSync, existsSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { serve } from "../expo/serve.mjs";
import { encodePng } from "./png.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const argv = process.argv.slice(2);
const opt = (k) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : undefined; };
const all = (k) => argv.flatMap((a, i) => (a === k ? [argv[i + 1]] : []));
const steps = argv.flatMap((a, i) => (["--click", "--type", "--key", "--scroll", "--drag"].includes(a) ? [[a.slice(2), argv[i + 1]]] : []));
const metro = opt("--metro"), bundle = opt("--bundle");
if (!metro && !bundle) { console.error("usage: node web/test/expo.mjs --metro URL | --bundle URL [steps] (see header)"); process.exit(2); }
const dir = resolve(root, opt("--dir") ?? join(process.env.CARGO_TARGET_DIR ?? "target", "web-expo"));
if (!existsSync(join(dir, "app.wasm"))) { console.error(`missing ${dir}/app.wasm: run scripts/build-web.sh --expo`); process.exit(2); }
const chrome = process.env.CHROME ?? ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "/usr/bin/chromium", "/usr/bin/google-chrome"].find(existsSync);
if (!chrome) { console.log("SKIP: no Chrome/Chromium (set CHROME=)"); process.exit(0); }

const { server, url: base } = await serve({ dir, port: 0, metro: metro ?? "http://127.0.0.1:9" });
const q = new URLSearchParams(metro ? { metro: "/metro" } : { bundle });
for (const k of ["platform", "ios", "device", "renderer", "scale"]) if (opt(`--${k}`)) q.set(k, opt(`--${k}`));
q.set("dark", argv.includes("--dark") ? "1" : "0");

const profile = mkdtempSync(join(tmpdir(), "onesim-chrome-"));
const proc = spawn(chrome, ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--no-first-run", "--enable-unsafe-webgpu",
  "--enable-features=Vulkan,WebGPU", "--ignore-gpu-blocklist", "--window-size=500,1000", "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
const cleanup = () => { proc.kill(); server.close(); try { rmSync(profile, { recursive: true, force: true }); } catch {} };
let failed = false;
const fail = (msg) => { console.error(`FAIL ${msg}`); failed = true; };
try {
  const wsUrl = await new Promise((ok, no) => {
    let buf = "";
    proc.stderr.on("data", (d) => { buf += d; const m = buf.match(/ws:\/\/\S+/); if (m) ok(m[0]); });
    setTimeout(() => no(new Error(`Chrome did not start:\n${buf}`)), 20000);
  });
  const ws = new WebSocket(wsUrl);
  await new Promise((r) => ws.addEventListener("open", r));
  let id = 0; const pending = new Map(), logs = [];
  ws.addEventListener("message", (e) => {
    const m = JSON.parse(e.data);
    if (m.id) return pending.get(m.id)?.(m);
    if (m.method === "Runtime.consoleAPICalled" && ["error", "warning"].includes(m.params.type)) logs.push(m.params.args.map((a) => a.value ?? a.description).join(" "));
    if (m.method === "Runtime.exceptionThrown") logs.push(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text);
  });
  const send = (method, params = {}, sessionId) => new Promise((ok, no) => {
    const n = ++id;
    pending.set(n, (m) => (m.error ? no(new Error(`${method}: ${m.error.message}`)) : ok(m.result)));
    ws.send(JSON.stringify({ id: n, method, params, sessionId }));
  });
  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
  await send("Runtime.enable", {}, sessionId);
  const ev = async (expression) => {
    const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true }, sessionId);
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
    return r.result.value;
  };
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  // Wait for a tree after `seen` (at most `first` ms), then until none arrives for 300 ms (at most 5 s more).
  const settle = async (seen, first) => {
    const t0 = Date.now();
    let n = seen;
    while (n === seen && Date.now() - t0 < first) { await sleep(25); n = await ev("window.onesim?.trees ?? 0"); }
    for (let quiet = Date.now(), t1 = Date.now(); Date.now() - quiet < 300 && Date.now() - t1 < 5000; await sleep(25)) {
      const m = await ev("window.onesim.trees");
      if (m !== n) { n = m; quiet = Date.now(); }
    }
    return n;
  };
  await send("Page.navigate", { url: `${base}/?${q}` }, sessionId);
  for (const t0 = Date.now(); !(await ev("(window.onesim?.trees ?? 0) > 0")); await sleep(100)) {
    const err = await ev("document.getElementById('err')?.textContent || ''");
    if (err || Date.now() - t0 > 120000) throw new Error(`no tree from the guest: ${err || "timeout"}\n${logs.slice(-10).join("\n")}`);
  }
  let trees = await settle(0, 0);
  const renderer = await ev("JSON.stringify({ mode: onesimRenderer.mode, reason: onesimRenderer.reason ?? null })");
  console.log(`first tree; renderer ${renderer}`);
  for (const [kind, v] of steps) {
    const nums = () => v.split(",").map(Number);
    const call = {
      click: () => `onesim.click(${JSON.stringify(v)})`,
      type: () => `onesim.type(${JSON.stringify(v)})`,
      key: () => `onesim.key(${JSON.stringify(v)})`,
      scroll: () => `onesim.scroll(${nums().join(",")})`,
      // A touch drag as web/main.js makes it: pointer down, drag begin, moves 16 ms apart in real time (so the guest
      // can answer between them, e.g. claim the touch), lifted at rest: no fling (like the native --drag).
      drag: () => { const [x, y, dx, dy] = nums(); return `(async () => { const t = performance.now(); onesim.pointer("down", ${x}, ${y}); onesim.drag("begin", ${x}, ${y}, t); for (let i = 1; i <= 8; i++) { await new Promise((r) => setTimeout(r, 16)); onesim.drag("move", ${x} + ${dx} * i / 8, ${y} + ${dy} * i / 8, t + 16 * i); } onesim.drag("end", ${x + dx}, ${y + dy}, t + 1144); })()`; },
    }[kind]();
    try { await ev(call); } catch (e) { fail(`--${kind} ${v}: ${e.message}`); break; }
    trees = await settle(trees, 2000);
    console.log(`--${kind} ${v}: ${trees} trees`);
  }
  await sleep(100); // the animation clock's last frame
  // text and label of every inspection node (Android text nodes carry only `label`), like scripts/compare-expo-ui.sh
  const texts = await ev("(() => { const o = []; const w = (n) => { for (const k of ['text', 'label']) if (n[k] != null) o.push(n[k]); n.children.forEach(w); }; w(onesim.inspect()); return o; })()");
  for (const t of all("--expect")) if (!texts.some((x) => x.includes(t))) fail(`--expect ${JSON.stringify(t)} not in ${JSON.stringify(texts).slice(0, 400)}`);
  // The presented canvas (CPU or WebGPU), redrawn and read back in one task, as web/test/presenter-page.js does.
  const fb = await ev(`(() => { onesim.present(); const c = onesimRenderer.canvas, w = c.width, h = c.height, c2 = document.createElement("canvas");
    c2.width = w; c2.height = h; const x = c2.getContext("2d"); x.drawImage(c, 0, 0); const d = x.getImageData(0, 0, w, h).data;
    let s = ""; for (let i = 0; i < d.length; i += 0x8000) s += String.fromCharCode.apply(null, d.subarray(i, i + 0x8000));
    let blank = 0; for (let i = 0; i < d.length; i += 4) if (d[i] === d[0] && d[i + 1] === d[1] && d[i + 2] === d[2]) blank++;
    return { width: w, height: h, b64: btoa(s), nonBlankPct: 100 - (100 * blank * 4) / d.length, mode: onesimRenderer.mode, reason: onesimRenderer.reason }; })()`);
  if (opt("--renderer") === "gpu" && fb.mode !== "gpu") fail(`fell back to ${fb.mode}: ${fb.reason}`);
  console.log(`frame ${fb.width}x${fb.height}, ${fb.nonBlankPct.toFixed(1)}% non-background`);
  if (fb.nonBlankPct < 1) fail("blank frame");
  if (opt("--screenshot")) writeFileSync(opt("--screenshot"), encodePng({ width: fb.width, height: fb.height, data: Buffer.from(fb.b64, "base64") }));
  if (opt("--inspect")) writeFileSync(opt("--inspect"), JSON.stringify(await ev("onesim.inspect()"), null, 1));
  if (failed) console.error(logs.slice(-10).join("\n"));
  ws.close();
} catch (e) { fail(e.message); } finally { cleanup(); }
process.exit(failed ? 1 : 0);
