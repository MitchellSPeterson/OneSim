// Milestone 4 check (native-module compatibility): node frameworks/react-native/guest/run-modules.mjs [port=8180]
// Boots examples/expo-modules (clipboard, haptics, location, camera) and asserts the `modules` line, the simulated clipboard /
// haptics / location / permissions, host `simulate`, state surviving a reload, and the unsupported-module diagnostic.
// A Metro already listening on the port is reused (and left running).
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';

const root = fileURLToPath(new URL('../../..', import.meta.url));
const port = Number(process.argv[2]) || 8180;
const children = [];
const cleanup = () => { for (const c of children) { try { process.kill(-c.pid); } catch {} try { c.kill(); } catch {} } };
process.on('exit', cleanup);
const fail = (m) => { console.error('FAIL:', m); cleanup(); process.exit(1); };
setTimeout(() => fail('overall timeout'), 240000).unref();
const ok = (m) => console.log('ok:', m);

const metroUp = () => fetch(`http://localhost:${port}/status`).then((r) => r.text()).then((t) => t.includes('running'), () => false);
if (await metroUp()) console.log(`metro :${port} already running`);
else {
  const mp = spawn('npx', ['expo', 'start', '--port', String(port)], { cwd: join(root, 'examples/expo-modules'), env: { ...process.env, CI: '1', EXPO_NO_TELEMETRY: '1', EXPO_OFFLINE: '1' }, stdio: 'ignore', detached: true });
  children.push(mp);
  for (let i = 0; ; i++) { if (await metroUp()) break; if (i > 120) fail('metro did not start'); await new Promise((r) => setTimeout(r, 500)); }
}

const walk = (n, f) => { f(n); n.children.forEach((c) => walk(c, f)); };
const textOf = (n) => { const t = []; walk(n, (v) => v.component === 'RCTRawText' && t.push(v.props.text ?? v.props.children)); return t.join(''); };
const host = spawn('node', [join(root, 'frameworks/react-native/guest/host.js')], { stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env, SIMCORE_METRO_URL: `http://localhost:${port}`, SIMCORE_VIEWPORT: '402x874', SIMCORE_HMR: '0' } });
children.push(host);
host.on('exit', (c) => c && fail(`host exited ${c}`));
let tree = null, text = '', stderr = '', modulesLines = [], events = [], errors = [], waiters = [];
const send = (m) => host.stdin.write(JSON.stringify(m) + '\n');
const check = () => { waiters = waiters.filter((w) => !w.pred() || (clearTimeout(w.t), w.res(), false)); };
const until = (pred, what, ms = 20000) => new Promise((res, rej) => {
  const w = { pred, res, t: setTimeout(() => rej(new Error(`timeout waiting for ${what}; text="${text}" errors=${errors.map((e) => e.slice(0, 200))}`)), ms) };
  waiters.push(w); check();
});
host.stderr.on('data', (d) => { stderr += d; });
createInterface({ input: host.stdout }).on('line', (line) => {
  const m = JSON.parse(line);
  if (m.type === 'tree') { tree = m.root; text = textOf(m.root); }
  else if (m.type === 'modules') modulesLines.push(m.modules);
  else if (m.type === 'event') events.push(m);
  else if (m.type === 'error') errors.push(m.message);
  check();
});
const tap = (label) => {
  let hit; walk(tree, (v) => { if (v.component === 'RCTView' && v.props.onClick && textOf(v) === label) hit = v; });
  if (!hit) throw new Error(`no tappable "${label}"; text="${text}"`);
  const tag = +hit.id, ts = Date.now();
  send({ type: 'touch', phase: 'start', tag, x: 100, y: 100, lx: 5, ly: 5, timestamp: ts });
  setTimeout(() => send({ type: 'touch', phase: 'end', tag, x: 100, y: 100, lx: 5, ly: 5, timestamp: ts + 60 }), 60);
};
const out = () => (/Output: (.*?)Watch:/.exec(text) || [])[1];
const press = async (label, re, what) => { tap(label); await until(() => re.test(out() || ''), what || `${label} -> ${re}`); ok(`${label}: ${out()}`); };

try {
  await until(() => /Output: ready/.test(text) && modulesLines.length, 'first tree + modules line');
  const mods = Object.fromEntries(modulesLines[0].map((m) => [m.name, m]));
  const want = { ExpoClipboard: 'simulated', ExpoHaptics: 'simulated', ExpoLocation: 'simulated', ExpoCamera: 'unsupported', ExpoDevice: 'simulated', DeviceInfo: 'simulated', Appearance: 'simulated', RNCSafeAreaContext: 'simulated', Clipboard: 'simulated' };
  for (const [n, s] of Object.entries(want)) if (!mods[n] || mods[n].status !== s) throw new Error(`modules: ${n} should be ${s}, got ${mods[n] && mods[n].status}`);
  if (!mods.ExpoLocation.methods.includes('watchPositionImplAsync') || mods.ExpoLocation.package !== 'expo-location' || !mods.ExpoLocation.events.includes('Expo.locationChanged')) throw new Error('modules: ExpoLocation entry ' + JSON.stringify(mods.ExpoLocation));
  ok(`modules line: ${modulesLines[0].length} modules; clipboard/haptics/location simulated, camera unsupported`);

  await press('Copy', /copied/);
  if (!events.some((e) => e.name === 'module.called' && e.detail.module === 'ExpoClipboard' && e.detail.method === 'setStringAsync' && e.detail.args[0] === 'hello')) throw new Error('no module.called for setStringAsync');
  await press('Paste', /pasted hello/);
  send({ type: 'simulate', module: 'clipboard', state: { text: 'from host' } });
  await press('Paste', /pasted from host/, 'Paste shows the simulated clipboard');

  const n0 = events.length;
  await press('Buzz', /buzzed/);
  if (!events.slice(n0).some((e) => e.name === 'module.called' && e.detail.module === 'ExpoHaptics' && e.detail.method === 'impactAsync')) throw new Error('no module.called for impactAsync');
  ok('haptics: module.called impactAsync ' + JSON.stringify(events.slice(n0).find((e) => e.name === 'module.called').detail));

  await press('Locate', /permission granted lat 37\.3349 lon -122\.009/, 'Locate: granted + Apple Park');
  await press('Watch', /watching/);
  await until(() => /Watch: lat 37\.3349 lon -122\.009/.test(text), 'first watch fix');
  send({ type: 'simulate', module: 'location', state: { latitude: 48.8584, longitude: 2.2945 } });
  await until(() => /Watch: lat 48\.8584 lon 2\.2945/.test(text), 'watch update from simulate location');
  ok('watchPositionAsync: simulate location pushed to the watcher (' + /Watch: (lat \S+ lon [\d.-]+)/.exec(text)[1] + ')');

  // state set through `simulate` survives a reload (the supervisor replays it)
  send({ type: 'simulate', module: 'permissions', state: { location: 'denied' } });
  const nm = modulesLines.length;
  send({ type: 'reload' });
  await until(() => modulesLines.length > nm && /Output: ready/.test(text), 'reload + modules line again');
  await press('Locate', /permission denied/, 'Locate after denied + reload');

  const n1 = events.length, e1 = errors.length;
  tap('Camera');
  await until(() => /Output: error: SimCore: ExpoCamera is not supported\./.test(text), 'Camera error in the UI');
  ok('camera: UI shows ' + out());
  const diag = 'SimCore: ExpoCamera is not supported.\n\nModule: expo-camera\nMethod: requestCameraPermissionsAsync';
  if (!stderr.includes(diag)) throw new Error('stderr lacks the diagnostic:\n' + stderr.slice(-800));
  const un = events.slice(n1).filter((e) => e.name === 'module.unsupported');
  if (un.length !== 1 || JSON.stringify(un[0].detail) !== JSON.stringify({ module: 'ExpoCamera', method: 'requestCameraPermissionsAsync', package: 'expo-camera' })) throw new Error('module.unsupported: ' + JSON.stringify(un));
  if (errors.slice(e1).length !== 1 || !errors[e1].startsWith(diag)) throw new Error('error lines: ' + JSON.stringify(errors.slice(e1)));
  ok('camera: stderr diagnostic, one module.unsupported, one error line');
  const stray = errors.slice(0, e1);
  if (stray.length) throw new Error('unexpected errors:\n' + stray.map((e) => e.slice(0, 400)).join('\n---\n'));
  ok('no other errors');
  send({ type: 'shutdown' });
  console.log('PASS');
  process.exit(0);
} catch (e) { fail(e.message); }
