// What DemoKit would do through OneSim's public API only. Run: node demo.ts
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { OneSimClient, type UiNode } from "../../sdk/typescript/src/index.ts";

const out = process.env.OUT_DIR ?? process.cwd();
const onesim = new OneSimClient({ url: process.env.SIMCORE_URL ?? "http://localhost:3030" });
const texts = (n: UiNode): string[] => [n.text ?? "", ...n.children.flatMap(texts)];
const counter = async (s: { tree(): Promise<{ root: UiNode }> }) => texts((await s.tree()).root).join("").match(/Count:\s*(\d+)/)?.[1];
const save = async (s: { screenshot(): Promise<{ png: Uint8Array }> }, file: string) =>
  writeFileSync(join(out, file), (await s.screenshot()).png);

const session = await onesim.createSession({
  framework: "expo",
  device: "iPhone 17 Pro",
  metroUrl: process.env.METRO_URL ?? "http://localhost:8081",
});
try {
  // Log lifecycle events (replay=true also shows the ones emitted while loading).
  session.on("*", (e) => {
    if (/^(runtime\.|bundle\.|navigation\.changed)/.test(e.name)) console.log(`event #${e.seq} ${e.name}`, JSON.stringify(e.data));
  }, { replay: true });

  await session.waitFor("runtime.ready", undefined, 30_000, true);
  await save(session, "before.png");
  console.log("counter before:", await counter(session));

  const updated = session.waitFor("render.updated"); // register before the tap
  await session.tap({ label: "Increment" });
  await updated;
  console.log("counter after: ", await counter(session));
  await save(session, "after.png");

  await session.setAppearance("dark");
  await save(session, "dark.png");
  console.log(`wrote before.png after.png dark.png to ${out}`);
} finally {
  await session.destroy();
  await onesim.close();
}
