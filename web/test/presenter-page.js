// Page half of web/test/presenter.mjs: drives the real Presenter (web/renderer.js) with a fake runtime that
// replays fixtures written by simcore-web (NAME.dl = display list, NAME.rgba = CPU pixels of the same frame),
// then reads the visible canvas back and compares it to the CPU pixels.
//   ?fixture=home  ?renderer=cpu|gpu|auto  ?fail=render (display list turns to garbage after start)
import { Presenter, preferredRenderer } from "/renderer.js";

const q = new URLSearchParams(location.search);
const name = q.get("fixture") ?? "home";
const bytes = async (f) => new Uint8Array(await (await fetch(`/fx/${f}`)).arrayBuffer());
const manifest = await (await fetch("/fx/manifest.json")).json();
const { width, height } = manifest.find((m) => m.name === name);
const dl = await bytes(`${name}.dl`), rgba = await bytes(`${name}.rgba`);

const sim = {
  supportsDisplayList: q.get("old") !== "1", output: 0, width: 402, height: 874,
  setOutput(m) { this.output = m; },
  displayList() { return q.get("fail") === "render" ? new Uint8Array([1, 2, 3]) : dl; },
  frameView() { return { width, height, data: new Uint8ClampedArray(rgba.buffer.slice(0)) }; },
};
const original = document.getElementById("screen");
const logs = [];
const log = { info: (...a) => logs.push(a.join(" ")), warn: (...a) => logs.push(a.join(" ")), error: (...a) => logs.push(a.join(" ")) };
const p = new Presenter({ sim, stage: document.getElementById("stage"), canvas: document.getElementById("screen"), preference: preferredRenderer(q), diag: document.getElementById("diag"), log });
await p.start();
p.present();
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
  fixture: name, mode: p.mode, reason: p.reason, output: sim.output, canvasSwapped: p.canvas !== original,
  canvasConnected: p.canvas.isConnected, size: [p.canvas.width, p.canvas.height], diffPct: (100 * differing) / px, nonBlankPct: (100 * nonBg) / px, info: p.info, diag: document.getElementById("diag").textContent, logs,
};
