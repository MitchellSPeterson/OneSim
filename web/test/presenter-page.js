// Page half of web/test/presenter.mjs: drives the real Presenter (web/renderer.js) with a fake runtime that
// replays fixtures written by onesim-web (NAME.dl = display list, NAME.rgba = CPU pixels of the same frame),
// then reads the visible canvas back and compares it to the CPU pixels.
// Query options select the initial fixture, renderer, and lifecycle scenario.
import { Presenter, preferredRenderer } from "/renderer.js";

const q = new URLSearchParams(location.search);
const name = q.get("fixture") ?? "home";
const bytes = async (f) => new Uint8Array(await (await fetch(`/fx/${f}`)).arrayBuffer());
const manifest = await (await fetch("/fx/manifest.json")).json();
async function load(name) {
  const [dl, rgba] = await Promise.all([bytes(`${name}.dl`), bytes(`${name}.rgba`)]);
  return { ...manifest.find((m) => m.name === name), dl, rgba };
}
let fixture = await load(name);
const scenario = q.get("scenario");
const next = scenario === "resize" ? await load("home-compact") : ["loss", "error"].includes(scenario) ? await load("controls") : null;

const sim = {
  supportsDisplayList: q.get("old") !== "1", output: 0, width: 402, height: 874,
  setOutput(m) { this.output = m; },
  displayList() { return q.get("fail") === "render" ? new Uint8Array([1, 2, 3]) : fixture.dl; },
  frameView() {
    const { width, height, rgba } = fixture;
    return { width, height, data: new Uint8ClampedArray(rgba.buffer.slice(0)) };
  },
};
const original = document.getElementById("screen");
const logs = [];
const log = { info: (...a) => logs.push(a.join(" ")), warn: (...a) => logs.push(a.join(" ")), error: (...a) => logs.push(a.join(" ")) };
const p = new Presenter({ sim, stage: document.getElementById("stage"), canvas: document.getElementById("screen"), preference: preferredRenderer(q), diag: document.getElementById("diag"), log });
await p.start();
p.present();
const sizes = [[p.canvas.width, p.canvas.height]];
if (scenario === "resize") {
  for (const f of [next, fixture, next]) {
    fixture = f;
    p.present();
    sizes.push([p.canvas.width, p.canvas.height]);
  }
}
if (scenario === "loss" || scenario === "error") {
  fixture = next;
  p.present();
  if (scenario === "loss") window.__testDevice.destroy();
  else window.__testDevice.createBuffer({ size: 8, usage: 0 });
  const deadline = Date.now() + 4000;
  while (p.mode === "gpu" && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
}
const { width, height, rgba } = fixture;
// Same task as the present: a WebGPU canvas keeps its frame only until the task ends.
const rb = document.createElement("canvas");
rb.width = width; rb.height = height;
const c2 = rb.getContext("2d", { willReadFrequently: true });
c2.drawImage(p.canvas, 0, 0);
const got = c2.getImageData(0, 0, width, height).data;
let differing = 0, nonBg = 0;
for (let i = 0; i < got.length; i += 4) {
  if (Math.abs(got[i] - rgba[i]) > 8 || Math.abs(got[i + 1] - rgba[i + 1]) > 8 || Math.abs(got[i + 2] - rgba[i + 2]) > 8) differing++;
  if (got[i] !== got[0] || got[i + 1] !== got[1] || got[i + 2] !== got[2]) nonBg++;
}
const px = width * height;
window.__result = {
  fixture: fixture.name, sizes, frames: p.frames, mode: p.mode, reason: p.reason, output: sim.output, canvasSwapped: p.canvas !== original,
  canvasConnected: p.canvas.isConnected, size: [p.canvas.width, p.canvas.height], diffPct: (100 * differing) / px, nonBlankPct: (100 * nonBg) / px, info: p.info, diag: document.getElementById("diag").textContent, logs,
};
