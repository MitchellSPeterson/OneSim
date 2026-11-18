#!/usr/bin/env node
// Headless harness for the browser backend (Node >= 20, no dependencies). Loads app.wasm with the
// same driver as the browser (web/simcore.js) and drives it like `simcore run`.
//
//   node web/test/run.mjs [--wasm target/web/app.wasm] [--width W] [--height H] [--scale S] [--dark] [--dynamic-type SIZE]
//        [--click LABEL|X,Y] [--type TEXT] [--key backspace|enter] [--scroll X,Y,DX,DY] [--drag X,Y,DX,DY] ...   (in order)
//        [--screenshot out.png] [--inspect] [--expect TEXT]...
//   node web/test/run.mjs --suite counter|showcase     assertions against the built app
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
import { load } from "../simcore.js";
import { encodePng } from "./png.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const nums = (s, n) => { const v = s.split(",").map((x) => (x.trim() === "" ? NaN : Number(x))); return v.length === n && v.every(Number.isFinite) ? v : null; };

async function boot(wasmPath, opts) {
  const log = (level, t) => { if (level === "error") console.error(`[guest] ${t}`); };
  return (await load(readFileSync(wasmPath), { log })).init(opts);
}

function click(sim, c) {
  const xy = nums(c, 2);
  const [x, y] = xy ?? sim.find(c);
  sim.click(x, y);
}

/** One CLI-style step: [flag, value]. */
function step(sim, flag, v) {
  switch (flag) {
    case "--click": return click(sim, v);
    case "--type": return sim.type(v);
    case "--key": return sim.key(v);
    case "--scroll": {
      const a = nums(v, 4);
      if (!a) throw new Error(`--scroll wants X,Y,DX,DY, got ${JSON.stringify(v)}`);
      const [x, y, dx, dy] = a;
      sim.scroll(x, y, dx, dy);
      if (!sim.inspect()) throw new Error("no tree");
      return;
    }
    case "--drag": {
      // One finger gesture, ending long after the last move (no fling), like the native CLI.
      const a = nums(v, 4);
      if (!a) throw new Error(`--drag wants X,Y,DX,DY, got ${JSON.stringify(v)}`);
      const [x, y, dx, dy] = a;
      sim.drag("begin", x, y, 0);
      sim.drag("move", x + dx, y + dy, 16);
      sim.drag("end", x + dx, y + dy, 1000);
      if (!sim.inspect()) throw new Error("no tree");
      return;
    }
    default: throw new Error(`unknown step ${flag}`);
  }
}

function savePng(sim, path) {
  const fb = sim.framebuffer();
  writeFileSync(path, encodePng(fb));
  console.error(`wrote ${path} (${fb.width}x${fb.height})`);
}

async function cli(argv) {
  const o = { wasm: resolve(here, "../../target/web/app.wasm"), width: 402, height: 874, scale: 3, dark: false, dynamicType: "large" };
  const steps = [], expects = [];
  let shot, inspect = false;
  for (let i = 0; i < argv.length; i++) {
    const f = argv[i], val = () => { if (i + 1 >= argv.length) throw new Error(`${f} needs a value`); return argv[++i]; };
    if (f === "--wasm") o.wasm = val();
    else if (f === "--width" || f === "--height" || f === "--scale") o[f.slice(2)] = Number(val());
    else if (f === "--dark") o.dark = true;
    else if (f === "--dynamic-type") o.dynamicType = val();
    else if (f === "--screenshot") shot = val();
    else if (f === "--inspect") inspect = true;
    else if (f === "--expect") expects.push(val());
    else if (["--click", "--type", "--key", "--scroll", "--drag"].includes(f)) steps.push([f, val()]);
    else throw new Error(`unknown argument ${f}`);
  }
  const sim = await boot(o.wasm, o);
  for (const [f, v] of steps) step(sim, f, v);
  const texts = sim.texts();
  for (const e of expects) if (!texts.some((t) => t.includes(e))) throw new Error(`expected text ${JSON.stringify(e)} not in tree: ${JSON.stringify(texts)}`);
  if (shot) savePng(sim, shot);
  if (inspect) console.log(JSON.stringify(sim.inspect(), null, 2));
}

// ---- suites -----------------------------------------------------------------------------------
const find = (n, p) => (p(n) ? n : n.children.map((c) => find(c, p)).find(Boolean));
const byRole = (sim, role, label) => find(sim.inspect(), (n) => n.role === role && n.label === label);
const field = (sim) => find(sim.inspect(), (n) => n.role === "textfield");
const hasText = (sim, t) => sim.texts().some((x) => x.includes(t));
const px = (sim, x, y) => { const f = sim.framebuffer(); const i = (Math.round(y * sim.scale) * f.width + Math.round(x * sim.scale)) * 4; return [...f.data.subarray(i, i + 3)]; };

const suites = {
  async counter(wasm) {
    const sim = await boot(wasm, {});
    assert.ok(hasText(sim, "Count: 0"));
    sim.clickLabel("Increment"); sim.clickLabel("Increment");
    assert.ok(hasText(sim, "Count: 2"), "two clicks");
    assert.ok(sim.lastTreeText().includes("Count: 2"));
    assert.throws(() => sim.clickLabel("Nope"), /no button/);
    const fb = sim.framebuffer();
    assert.deepEqual([fb.width, fb.height], [1206, 2622]);
    const light = px(sim, 5, 300);
    sim.setAppearance("dark");
    assert.ok(px(sim, 5, 300)[0] < light[0], "dark background is darker");
    assert.ok(hasText(sim, "Count: 2"), "state survives the environment change");
    sim.setViewport(300, 500);
    assert.deepEqual([sim.framebuffer().width, sim.framebuffer().height], [900, 1500]);
    sim.setAppearance("light");
    assert.deepEqual(sim.framebuffer().width, 900);
  },

  async showcase(wasm) {
    const sim = await boot(wasm, {});
    assert.ok(hasText(sim, "Open todos"));
    // Typography: scroll like the wheel does, then dark mode and back.
    sim.clickLabel("Typography");
    assert.ok(hasText(sim, "Large Title") && hasText(sim, "Light appearance"));
    assert.equal(sim.scroll(200, 400, 0, 120), false, "content fits the 874pt screen: nothing to scroll");
    sim.setViewport(402, 500);
    const before = Buffer.from(sim.framebuffer().data).toString("base64");
    assert.equal(sim.scroll(200, 300, 0, 120), true, "scrolls");
    assert.notEqual(Buffer.from(sim.framebuffer().data).toString("base64"), before, "scroll moves pixels");
    assert.equal(sim.scroll(200, 300, 0, -1e6), true, "scroll back to top");
    // Touch: pull down past the top (rubber band); a still release bounces back to the same frame.
    const fb = () => Buffer.from(sim.framebuffer().data).toString("base64");
    const top = fb();
    sim.drag("begin", 200, 200, 0);
    assert.equal(sim.drag("move", 200, 300, 50), true, "rubber band moves content");
    sim.drag("end", 200, 300, 400); // finger rested 350 ms: no fling
    let frames = 0, t0 = performance.now();
    while (sim.tick(1000 / 60)) frames++;
    const ms = (performance.now() - t0) / frames;
    assert.ok(frames > 15 && frames < 60, `bounce back took ${frames} frames`);
    assert.equal(fb(), top, "rests exactly at the top");
    // Flick up: momentum keeps going after the finger lifts, then comes to rest.
    sim.drag("begin", 200, 400, 1000);
    sim.drag("move", 200, 360, 1016);
    sim.drag("move", 200, 320, 1032);
    const lifted = (sim.drag("end", 200, 320, 1040), fb());
    assert.equal(sim.tick(1000 / 60), true, "momentum after lift");
    assert.notEqual(fb(), lifted);
    frames = 0;
    while (sim.tick(1000 / 60)) assert.ok(++frames < 600, "comes to rest");
    // A tap on moving content stops it and is not a tap.
    sim.scroll(200, 300, 0, -1e6);
    sim.drag("begin", 200, 400, 2000);
    sim.drag("move", 200, 360, 2016);
    sim.drag("end", 200, 360, 2020);
    sim.tick(1000 / 60);
    assert.equal(sim.pointer("down", 200, 300), true, "caught");
    assert.equal(sim.tick(1000 / 60), false, "held content stays put");
    sim.pointer("up", 200, 300);
    while (sim.tick(1000 / 60)); // released past an edge: settles
    console.log(`    scroll frame: ${ms.toFixed(1)} ms at ${sim.scale}x`);
    sim.setViewport(402, 874);
    sim.setAppearance("dark");
    assert.ok(hasText(sim, "Dark appearance"));
    sim.setAppearance("light");
    sim.clickLabel("Showcase"); // back button
    assert.ok(hasText(sim, "Open todos"));

    // Todos: toggle a row, focus the field, type with the physical-keyboard path and the on-canvas keys.
    sim.clickLabel("Todos");
    assert.equal(sim.focused, false);
    assert.throws(() => sim.type("x"), /no focused text field/);
    sim.clickLabel("Ship the SwiftUI subset");
    sim.clickLabel("New todo");
    assert.equal(sim.focused, true, "tapping the field focuses it");
    assert.ok(byRole(sim, "key", "space"), "software keyboard is showing");
    sim.type("Buy milk");
    assert.equal(field(sim).value, "Buy milk");
    sim.clickLabel("s"); // on-canvas key (shift is still up: programmatic typing leaves it, as native)
    assert.equal(field(sim).value, "Buy milkS");
    sim.clickLabel("delete");
    assert.equal(field(sim).value, "Buy milk");
    sim.key("backspace");
    assert.equal(field(sim).value, "Buy mil");
    sim.type("k");
    sim.clickLabel("Add");
    assert.equal(sim.focused, false, "tapping a button unfocuses");
    assert.ok(hasText(sim, "Buy milk"), "todo added");
    // A person typing on a physical keyboard: shift drops after the first letter, so a following
    // on-canvas key tap is lowercase (regression: was "BuyS").
    sim.clickLabel("New todo");
    sim.typeByUser("B"); sim.typeByUser("u"); sim.typeByUser("y");
    sim.clickLabel("s");
    assert.equal(field(sim).value, "Buys");
    sim.key("backspace"); sim.key("backspace"); sim.key("backspace"); sim.key("backspace");
    sim.clickLabel("New todo"); sim.key("enter");
    assert.equal(sim.focused, false, "enter dismisses the keyboard");
    sim.clickLabel("Showcase");
    assert.ok(hasText(sim, "Open todos"), "back on the home list");

    // Controls: toggle and stepper.
    sim.clickLabel("Controls");
    assert.ok(hasText(sim, "You will be notified"));
    sim.clickLabel("Notifications");
    assert.ok(!hasText(sim, "You will be notified"));
    sim.clickLabel("More"); sim.clickLabel("More");
    assert.ok(hasText(sim, "Volume: 5"));
    sim.clickLabel("Volume: 5#decrement");
    assert.ok(hasText(sim, "Volume: 4"));
  },
};

try {
  const i = process.argv.indexOf("--suite");
  if (i > 0) {
    const name = process.argv[i + 1], wasm = resolve(here, "../../target/web/app.wasm");
    if (!suites[name]) throw new Error(`unknown suite ${JSON.stringify(name)} (${Object.keys(suites).join(", ")})`);
    await suites[name](wasm);
    console.log(`suite ${name}: ok`);
  } else {
    await cli(process.argv.slice(2));
  }
} catch (e) {
  console.error(e.stack || String(e));
  process.exit(1);
}
