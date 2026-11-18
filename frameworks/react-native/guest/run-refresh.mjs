// Fast Refresh check: node frameworks/react-native/guest/run-refresh.mjs [port=8100]
// Copies examples/expo-counter to a temp dir (node_modules symlinked), runs Metro in watch mode + host.js, then edits
// App.js and asserts: state-preserving refresh, syntax-error handling + recovery, render-error recovery,
// full reload when Refresh bails out (non-component export), manual {"type":"reload"}. Prints refresh latency.
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { cpSync, mkdtempSync, readFileSync, symlinkSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const port = Number(process.argv[2]) || 8100;
const metro = `http://localhost:${port}`;
const root = fileURLToPath(new URL('../../..', import.meta.url));
const dir = mkdtempSync(join(tmpdir(), 'simcore-refresh-'));
for (const f of ['App.js', 'app.json', 'index.js', 'package.json']) cpSync(join(root, 'examples/expo-counter', f), join(dir, f));
symlinkSync(join(root, 'examples/expo-counter/node_modules'), join(dir, 'node_modules'));
const appFile = join(dir, 'App.js');
const original = readFileSync(appFile, 'utf8');

const env = { ...process.env, EXPO_NO_TELEMETRY: '1', EXPO_OFFLINE: '1' };
delete env.CI; // CI=1 disables Metro's file watching
const mp = spawn('npx', ['expo', 'start', '--port', String(port)], { cwd: dir, env, stdio: ['ignore', 'ignore', 'ignore'] });
let host;
let finished = false;
const cleanup = () => { try { host && host.kill(); } catch {} try { process.kill(-mp.pid); } catch {} try { mp.kill(); } catch {} rmSync(dir, { recursive: true, force: true }); };
const fail = (m) => { if (finished) return; finished = true; console.error('FAIL:', m); cleanup(); process.exit(1); };
process.on('exit', cleanup);
setTimeout(() => fail('overall timeout'), 240000);

const send = (m) => host.stdin.write(JSON.stringify(m) + '\n');
const walk = (n, f) => { f(n); n.children.forEach((c) => walk(c, f)); };
const textOf = (n) => { const t = []; walk(n, (v) => v.component === 'RCTRawText' && t.push(v.props.text ?? v.props.children)); return t.join(''); };
let tree = null, treeText = '', versions = [], errors = [], events = [], waiters = [], seq = 0;
const check = () => { waiters = waiters.filter((w) => { if (w.pred()) { clearTimeout(w.t); w.res(); return false; } return true; }); };
const until = (pred, what, ms = 30000) => new Promise((res, rej) => {
  const w = { pred, res, t: setTimeout(() => { waiters = waiters.filter((x) => x !== w); rej(new Error(`timeout waiting for ${what}; text="${treeText}" errors=${errors.length} last="${(errors[errors.length - 1] || "").slice(0, 400)}" events=${events.map((e) => e.name)}`)); }, ms) };
  waiters.push(w); check();
});
const startHost = () => {
  host = spawn('node', [join(root, 'frameworks/react-native/guest/host.js')], { stdio: ['pipe', 'pipe', 'inherit'], env: { ...env, SIMCORE_METRO_URL: metro, SIMCORE_VIEWPORT: '393x852' }, detached: false });
  createInterface({ input: host.stdout }).on('line', (line) => {
    const m = JSON.parse(line);
    if (m.type === 'tree') { tree = m.root; treeText = textOf(tree); versions.push(m.version); seq++; }
    else if (m.type === 'error') errors.push(m.message);
    else if (m.type === 'event') events.push(m);
    check();
  });
  host.on('exit', (c) => !finished && fail(`host exited ${c}`));
};
const tap = (label) => {
  let target; walk(tree, (v) => { if (v.component === 'RCTView' && v.props.onClick && textOf(v).includes(label)) target = v; });
  if (!target) throw new Error('no button with label ' + label);
  const tag = +target.id, ts = Date.now();
  send({ type: 'touch', phase: 'start', tag, x: 100, y: 100, lx: 5, ly: 5, timestamp: ts });
  setTimeout(() => send({ type: 'touch', phase: 'end', tag, x: 100, y: 100, lx: 5, ly: 5, timestamp: ts + 60 }), 60);
};
const ok = (m) => console.log('ok:', m);
const has = (name) => events.some((e) => e.name === name);

try {
  for (let i = 0; i < 120; i++) { try { if ((await (await fetch(metro + '/status')).text()) === 'packager-status:running') break; } catch {} await new Promise((r) => setTimeout(r, 500)); }
  startHost();
  await until(() => /Count: ?0/.test(treeText), 'initial tree', 90000);
  if (!has('bundle.loaded')) throw new Error('no bundle.loaded event');
  ok('initial tree + bundle.loaded');
  for (const n of [1, 2]) { tap('Increment'); await until(() => treeText.includes(`Count: ${n}`), `Count: ${n}`); }
  ok('counter at 2');

  // 1. state-preserving refresh, 5 edits -> latency
  const lat = [];
  let label = 'Increment';
  for (let i = 0; i < 5; i++) {
    const next = i === 0 ? 'Add one' : `Add ${i + 1}`;
    const t0 = performance.now();
    writeFileSync(appFile, original.replace('>Increment<', `>${next}<`));
    await until(() => treeText.includes(next), `label "${next}"`);
    lat.push(Math.round(performance.now() - t0));
    if (!treeText.includes('Count: 2')) throw new Error('state lost on refresh: ' + treeText);
    label = next;
  }
  if (!has('refresh.applied')) throw new Error('no refresh.applied event');
  lat.sort((a, b) => a - b);
  ok(`refresh keeps state (label + Count: 2); latency ms sorted=${lat} median=${lat[2]}`);
  const good = original.replace('>Increment<', `>${label}<`);

  // 2. syntax error: error reported, tree unchanged; fix -> recovery
  let before = treeText, nerr = errors.length, nver = versions.length;
  writeFileSync(appFile, good.replace('<Text style={styles.title}>', '<Text style={styles.title'));
  await until(() => errors.length > nerr, 'syntax error message');
  if (!has('bundle.error')) throw new Error('no bundle.error event');
  await new Promise((r) => setTimeout(r, 1500));
  if (treeText !== before) throw new Error('tree changed after syntax error: ' + treeText);
  ok('syntax error -> error: ' + errors[errors.length - 1].split('\n')[0].slice(0, 100));
  writeFileSync(appFile, good.replace('Count: {count}', 'Count: {count}')); // fixed
  await until(() => treeText.includes('Count: 2') && errors.length >= nerr, 'recovery after syntax fix');
  tap(label); await until(() => treeText.includes('Count: 3'), 'taps work after recovery');
  ok('recovered after syntax fix (state preserved, taps work)');

  // 3. render error, then fix
  nerr = errors.length;
  writeFileSync(appFile, good.replace('return (', "if (count >= 3) throw new Error('boom from render');\n  return ("));
  await until(() => errors.some((e, i) => i >= nerr && /boom from render/.test(e)), 'render error message');
  ok('render error reported');
  await new Promise((r) => setTimeout(r, 1000)); // let Metro finish the in-flight update before the next edit
  await until(() => treeText === '', 'root unmounted by render error', 10000).catch(() => {});
  console.log('   (after render error tree is ' + JSON.stringify(treeText) + ')');
  writeFileSync(appFile, good);
  await until(() => treeText.includes(label) && /Count: ?\d/.test(treeText), 'recovery after render error', 15000);
  ok('recovered after render error: ' + treeText);

  // 4. Refresh bail-out (non-component export next to a component) -> full reload, state reset
  const gen = events.filter((e) => e.name === 'bundle.reloaded').length;
  if (!/Count: ?[1-9]/.test(treeText)) { tap(label); await until(() => /Count: ?[1-9]/.test(treeText), 'count > 0'); }
  writeFileSync(appFile, good + '\nexport const NOT_A_COMPONENT = 1;\n');
  await until(() => events.filter((e) => e.name === 'bundle.reloaded').length > gen && /Count: ?0/.test(treeText), 'full reload after bail-out');
  ok('non-component export -> full reload (Count: 0, bundle.reloaded)');
  writeFileSync(appFile, good);
  await until(() => treeText.includes(label), 'refresh works after reload');
  ok('Fast Refresh still works after a full reload');

  // 5. manual reload
  tap(label); await until(() => /Count: ?1/.test(treeText), 'Count: 1');
  const gen2 = events.filter((e) => e.name === 'bundle.reloaded').length, lastV = versions[versions.length - 1];
  send({ type: 'reload' });
  await until(() => events.filter((e) => e.name === 'bundle.reloaded').length > gen2 && /Count: ?0/.test(treeText) && versions[versions.length - 1] > lastV, 'manual reload');
  if (versions.some((v, i) => i && v <= versions[i - 1])) throw new Error('tree versions not monotonic');
  ok('{"type":"reload"} -> Count: 0, versions monotonic');
  console.log(`OK: fast refresh checks passed; refresh latency median ${lat[2]} ms (${lat.join(', ')})`);
  finished = true; send({ type: 'shutdown' }); cleanup(); process.exit(0);
} catch (e) { fail(e.message); }
