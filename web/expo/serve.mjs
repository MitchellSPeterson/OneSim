#!/usr/bin/env node
// Dev server for an Expo app in the browser (Node >= 22, no dependencies):
//   node web/expo/serve.mjs [--dir target/web-expo] [--port 8000] [--metro http://localhost:8081]
// Serves the `scripts/build-web.sh --expo` site and proxies Metro on the same origin (Metro sends no CORS headers):
// `/metro/*` -> Metro with the prefix stripped (the Expo manifest is Metro's `/`), and the paths the bundle itself uses
// at the origin root: `*.bundle` / `*.map`, `/assets/*`, `/hot` (WebSocket, Fast Refresh), `/symbolicate`, `/status`,
// `/message`, `/logs`, `/inspector`, `/json*`. Open `http://localhost:<port>/?metro=/metro`.
import { createServer, request } from "node:http";
import { connect } from "node:net";
import { existsSync, readFileSync, statSync } from "node:fs";
import { extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const TYPES = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".wasm": "application/wasm", ".json": "application/json", ".png": "image/png", ".css": "text/css" };
const METRO_PATHS = /^\/(?:assets\/|hot\b|symbolicate\b|status\b|message\b|logs\b|inspector\b|json\b|open-stack-frame\b|[^?]*\.(?:bundle|map)(?:\?|$))/;

/** Start the server; resolves to { url, server } (port 0 = any free port). */
export function serve({ dir = "target/web-expo", port = 8000, host = "127.0.0.1", metro = "http://localhost:8081" } = {}) {
  dir = resolve(dir);
  const m = new URL(metro);
  const target = (u) => (u.startsWith("/metro/") || u === "/metro" ? u.slice(6) || "/" : METRO_PATHS.test(u) ? u : null);
  const server = createServer((req, res) => {
    const path = target(req.url);
    if (path) {
      const up = request({ host: m.hostname, port: m.port || 80, path, method: req.method, headers: { ...req.headers, host: m.host } }, (r) => { res.writeHead(r.statusCode, r.headers); r.pipe(res); });
      up.on("error", (e) => { if (!res.headersSent) res.writeHead(502, { "content-type": "text/plain" }); res.end(`Metro at ${metro}: ${e.message}`); });
      return void req.pipe(up);
    }
    let file = join(dir, decodeURIComponent(new URL(req.url, "http://x").pathname));
    if (!file.startsWith(dir)) return void res.writeHead(403).end();
    if (existsSync(file) && statSync(file).isDirectory()) file = join(file, "index.html");
    if (!existsSync(file)) return void res.writeHead(404).end();
    res.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream", "cache-control": "no-store" }).end(readFileSync(file));
  });
  // WebSockets (Metro's /hot, /message): pipe the raw upgrade through.
  server.on("upgrade", (req, sock, head) => {
    const path = target(req.url);
    if (!path) return void sock.destroy();
    const up = connect(Number(m.port) || 80, m.hostname, () => {
      const headers = Object.entries({ ...req.headers, host: m.host }).map(([k, v]) => `${k}: ${v}`).join("\r\n");
      up.write(`${req.method} ${path} HTTP/1.1\r\n${headers}\r\n\r\n`);
      up.write(head);
      sock.pipe(up).pipe(sock);
    });
    up.on("error", () => sock.destroy());
    sock.on("error", () => up.destroy());
  });
  return new Promise((ok, no) => {
    server.once("error", no);
    server.listen(port, host, () => ok({ server, url: `http://${host === "0.0.0.0" ? "localhost" : host}:${server.address().port}` }));
  });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
  const metro = arg("--metro", "http://localhost:8081");
  const { url } = await serve({ dir: arg("--dir", "target/web-expo"), port: Number(arg("--port", 8000)), metro });
  console.log(`OneSim Expo: ${url}/?metro=/metro  (Metro ${metro})`);
}
