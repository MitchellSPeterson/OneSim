# @simcore/sdk

Zero-dependency TypeScript client for the SimCore JSON-RPC API ([docs/api.md](../../docs/api.md)).
Needs a global `WebSocket` (Node >= 22 or a browser). No build step: Node 22.18+/26 runs the `.ts` directly.

```ts
import { SimCoreClient } from "@simcore/sdk";

const simcore = new SimCoreClient({ url: "http://localhost:3030" }); // http(s):// is mapped to ws(s)://
const session = await simcore.createSession({ framework: "expo", device: "iPhone 18 Pro", metroUrl: "http://localhost:8081" });
await session.tap({ label: "Increment" });
const { png } = await session.screenshot();           // Uint8Array
session.on("navigation.changed", (e) => console.log(e.data.route));
await session.destroy();
await simcore.close();
```

Server: `simcore serve --listen 127.0.0.1:3030`.

- `createSession` loads by default (`load: false` to only create).
- Errors are `SimCoreError` (`code` = SimCore `data.code`, plus `rpcCode`, `sessionId`, `details`). Client-side codes: `timeout`, `disconnected`, `closed`, `connect_failed`.
- If the socket drops, the client reconnects with backoff (immediately for sessions with listeners, otherwise on the next call), repeats `initialize`, and re-subscribes with `since` = last seen `seq`; events are deduped by `seq`. In-flight requests fail with `disconnected`.
- `on()` is live-only; pass `{ replay: true }` (first listener of a session) to also get the server's buffered history. `waitFor(name, predicate?, timeoutMs?, replay?)`: register it **before** the action that causes the event.
