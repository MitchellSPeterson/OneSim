// Shared plumbing for the run-*.mjs drivers: Metro, host.js sessions, tree helpers, one cleanup registry.
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

export const root = fileURLToPath(new URL('../../..', import.meta.url));
export const hostJs = fileURLToPath(new URL('./host.js', import.meta.url));

const cleanups = [];
let over = false;
export const onCleanup = (f) => cleanups.push(f);
export const cleanup = () => { if (over) return; over = true; for (const f of cleanups.reverse()) try { f(); } catch {} };
process.on('exit', cleanup);
export const fail = (m) => { if (over) return; console.error('FAIL:', m); cleanup(); process.exit(1); };
/** Runs the driver body with an overall timeout; exits 0 on success, FAIL + 1 on a throw or timeout. */
export const run = (body, ms) => {
  setTimeout(() => fail('overall timeout'), ms).unref();
  body().then(() => { cleanup(); process.exit(0); }, (e) => fail(e.message));
};
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export const walk = (n, f) => { f(n); n.children.forEach((c) => walk(c, f)); };
export const textOf = (n) => { const t = []; walk(n, (v) => v.component === 'RCTRawText' && t.push(v.props.text ?? v.props.children)); return t.join(''); };

const metroUp = (port) => fetch(`http://localhost:${port}/status`).then((r) => r.text()).then((t) => t.includes('running'), () => false);
/** Starts `expo start` in appDir on port (reusing one already listening). CI=1 (no file watching) unless watch. */
export async function startMetro(appDir, port, { watch = false } = {}) {
  if (await metroUp(port)) return console.log(`metro :${port} already running`);
  const env = { ...process.env, EXPO_NO_TELEMETRY: '1', EXPO_OFFLINE: '1' };
  if (watch) delete env.CI; else env.CI = '1';
  const mp = spawn('npx', ['expo', 'start', '--port', String(port)], { cwd: appDir, env, stdio: 'ignore', detached: true });
  onCleanup(() => { try { process.kill(-mp.pid); } catch {} mp.kill(); });
  for (let i = 0; i < 120; i++) { if (await metroUp(port)) return console.log(`metro :${port} started (${appDir})`); await sleep(500); }
  throw new Error(`metro for ${appDir} did not start`);
}

/**
 * Spawns host.js with env; fails the run if it exits non-zero. onTree(h, msg) runs on every tree message.
 * h: tree (root), text, versions[], errors[], events[] (`event` messages), modules[] (`modules` payloads), other[] (any other non-tree message), stderr,
 * send(m), until(pred, what, ms), tap(label) (exact text match, deepest tappable), kill().
 */
export function startHost(env, onTree) {
  const p = spawn('node', [hostJs], { stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env, ...env } });
  onCleanup(() => p.kill());
  const h = { tree: null, text: '', versions: [], errors: [], events: [], modules: [], other: [], stderr: '' };
  let waiters = [];
  const check = () => { waiters = waiters.filter((w) => !w.pred() || (clearTimeout(w.t), w.res(), false)); };
  h.send = (m) => p.stdin.write(JSON.stringify(m) + '\n');
  h.kill = () => p.kill();
  h.until = (pred, what, ms = 30000) => new Promise((res, rej) => {
    const w = { pred, res, t: setTimeout(() => { waiters = waiters.filter((x) => x !== w); rej(new Error(`timeout waiting for ${what}; text="${h.text.slice(0, 300)}" errors=${h.errors.map((e) => e.slice(0, 300))} events=${h.events.map((e) => e.name)}`)); }, ms) };
    waiters.push(w); check();
  });
  h.tap = (label) => {
    let hit; walk(h.tree, (v) => { if (v.component === 'RCTView' && v.props.onClick && textOf(v) === label) hit = v; });
    if (!hit) throw new Error(`no tappable "${label}"; text="${h.text}"`);
    const tag = +hit.id, ts = Date.now();
    h.send({ type: 'touch', phase: 'start', tag, x: 100, y: 100, lx: 5, ly: 5, timestamp: ts });
    setTimeout(() => h.send({ type: 'touch', phase: 'end', tag, x: 100, y: 100, lx: 5, ly: 5, timestamp: ts + 60 }), 60);
  };
  p.stderr.on('data', (d) => { h.stderr += d; process.stderr.write(d); });
  p.on('exit', (c) => c && fail(`host exited ${c}`));
  createInterface({ input: p.stdout }).on('line', (line) => {
    const m = JSON.parse(line);
    if (m.type === 'tree') { h.layers = m.root.layers || []; if (m.root.kind === 'presentation') m.root = m.root.content; h.tree = m.root; h.text = textOf(m.root); h.versions.push(m.version); onTree?.(h, m); }
    else if (m.type === 'error') h.errors.push(m.message);
    else if (m.type === 'event') h.events.push(m);
    else if (m.type === 'modules') h.modules.push(m.modules);
    else h.other.push(m);
    check();
  });
  return h;
}
