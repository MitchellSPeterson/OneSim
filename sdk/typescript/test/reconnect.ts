// Needs a running `simcore serve --listen` and Metro. Env: SIMCORE_URL, METRO_URL.
// Kills the client socket mid-stream, keeps tapping, asserts the seq stream has no gaps or duplicates.
import assert from "node:assert/strict";
import { SimCoreClient, type SimEvent } from "../src/index.ts";

const client = new SimCoreClient({ url: process.env.SIMCORE_URL ?? "http://localhost:3030" });
const session = await client.createSession({
  framework: "expo",
  device: "iPhone 17 Pro",
  metroUrl: process.env.METRO_URL ?? "http://localhost:8081",
});
const seen: SimEvent[] = [];
session.on("*", (e) => seen.push(e), { replay: true }); // all events, from seq 1
await session.waitFor("runtime.ready", undefined, 30_000, true);

for (let i = 0; i < 6; i++) {
  await session.tap({ label: "Increment" });
  if (i === 1) client._socketForTest!.close(); // drop mid-stream; later taps reconnect lazily
  if (i === 3) await new Promise((r) => setTimeout(r, 500)); // let a background reconnect happen too
}
await session.tree(); // round trip so the server has flushed everything
await new Promise((r) => setTimeout(r, 500));

const seqs = seen.map((e) => e.seq);
console.log("seqs:", seqs.join(","));
assert.ok(seqs.length > 6, "expected several events");
seqs.forEach((s, i) => assert.equal(s, i === 0 ? seqs[0] : seqs[i - 1] + 1, `gap or duplicate at index ${i}: ${seqs}`));
assert.equal(seqs[0], 1, "replay should start at seq 1");
assert.ok(seen.filter((e) => e.name === "render.updated").length >= 2, "render.updated seen across the drop");

await session.destroy();
await client.close();
console.log("reconnect ok:", seqs.length, "events, seq", seqs[0], "->", seqs.at(-1));
