// Milestone 3 check (Expo Router): node frameworks/react-native/guest/run-router.mjs [tabsPort=8120] [stackPort=8121]
// Starts Metro for examples/expo-default (NativeTabs, reanimated) and examples/expo-router-stack (Stack), runs host.js on each
// at 402x874 and asserts: no unexpected `error`, the splash overlay animates away, select_tab / push / pop / native_back work.
// Metro already listening on a port is reused (and left running).
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';

const root = fileURLToPath(new URL('../../..', import.meta.url));
const hostJs = join(root, 'frameworks/react-native/guest/host.js');
const ports = { tabs: Number(process.argv[2]) || 8120, stack: Number(process.argv[3]) || 8121 };
const W = 402, H = 874;
// `error` messages that are known diagnostics (SimCore: X is not supported); none today.
const ALLOWED_ERRORS = [];

const children = [];
const cleanup = () => { for (const c of children) { try { process.kill(-c.pid); } catch {} try { c.kill(); } catch {} } };
process.on('exit', cleanup);
const fail = (m) => { console.error('FAIL:', m); cleanup(); process.exit(1); };
setTimeout(() => fail('overall timeout'), 300000).unref();

const metroUp = (port) => fetch(`http://localhost:${port}/status`).then((r) => r.text()).then((t) => t.includes('running'), () => false);
async function startMetro(dir, port) {
  if (await metroUp(port)) return console.log(`metro :${port} already running`);
  const env = { ...process.env, CI: '1', EXPO_NO_TELEMETRY: '1', EXPO_OFFLINE: '1' };
  const mp = spawn('npx', ['expo', 'start', '--port', String(port)], { cwd: join(root, 'examples', dir), env, stdio: 'ignore', detached: true });
  children.push(mp);
  for (let i = 0; i < 120; i++) { if (await metroUp(port)) return console.log(`metro :${port} started (${dir})`); await new Promise((r) => setTimeout(r, 500)); }
  fail(`metro for ${dir} did not start`);
}

const walk = (n, f) => { f(n); n.children.forEach((c) => walk(c, f)); };
const textOf = (n) => { const t = []; walk(n, (v) => v.component === 'RCTRawText' && t.push(v.props.text ?? v.props.children)); return t.join(''); };
const find = (n, pred) => { let r; walk(n, (v) => { if (!r && pred(v)) r = v; }); return r; };
const all = (n, pred) => { const r = []; walk(n, (v) => pred(v) && r.push(v)); return r; };

// One guest session against a Metro: tracks the latest tree, errors, and answers layout so onLayout fires.
function session(port) {
  const host = spawn('node', [hostJs], { stdio: ['pipe', 'pipe', 'inherit'], env: { ...process.env, SIMCORE_METRO_URL: `http://localhost:${port}`, SIMCORE_VIEWPORT: `${W}x${H}`, SIMCORE_APPEARANCE: 'light', SIMCORE_HMR: '0' } });
  children.push(host);
  const s = { tree: null, text: '', errors: [], navs: [], versions: 0, laidOut: new Set(), waiters: [], host };
  s.send = (m) => host.stdin.write(JSON.stringify(m) + '\n');
  const check = () => { s.waiters = s.waiters.filter((w) => { if (w.pred()) { clearTimeout(w.t); w.res(); return false; } return true; }); };
  s.until = (pred, what, ms = 30000) => new Promise((res, rej) => {
    const w = { pred, res, t: setTimeout(() => { s.waiters = s.waiters.filter((x) => x !== w); rej(new Error(`timeout waiting for ${what}; text="${s.text.slice(0, 300)}" errors=${s.errors.map((e) => e.slice(0, 300))} navs=${JSON.stringify(s.navs)}`)); }, ms) };
    s.waiters.push(w); check();
  });
  createInterface({ input: host.stdout }).on('line', (line) => {
    const m = JSON.parse(line);
    if (m.type === 'error') s.errors.push(m.message);
    else if (m.type === 'event' && m.name === 'navigation.changed') s.navs.push(m.detail);
    else if (m.type === 'tree') {
      s.tree = m.root; s.text = textOf(m.root); s.versions++;
      walk(m.root, (v) => { if (v.props.nativeID != null && v.props.opacity > 0 && v.props.opacity < 1) s.midFade = true; }); // an entering animation in flight
      // the Rust host's layout stand-in: every new view gets the full screen as its frame (enough for onLayout)
      const frames = {};
      walk(m.root, (v) => { if (!s.laidOut.has(v.id)) { s.laidOut.add(v.id); frames[v.id] = [0, 0, W, H]; } });
      if (Object.keys(frames).length) s.send({ type: 'layout', frames });
    }
    check();
  });
  host.on('exit', (c) => c && fail(`host exited ${c}`));
  s.tap = (label) => {
    const hit = all(s.tree, (v) => v.component === 'RCTView' && v.props.onClick && textOf(v) === label).pop();
    if (!hit) throw new Error(`no tappable "${label}"; text="${s.text}"`);
    const tag = +hit.id, ts = Date.now();
    s.send({ type: 'touch', phase: 'start', tag, x: 100, y: 100, lx: 5, ly: 5, timestamp: ts });
    setTimeout(() => s.send({ type: 'touch', phase: 'end', tag, x: 100, y: 100, lx: 5, ly: 5, timestamp: ts + 60 }), 60);
  };
  s.close = () => { s.send({ type: 'shutdown' }); };
  return s;
}
const ok = (m) => console.log('ok:', m);
const noErrors = (s, app) => {
  const bad = s.errors.filter((e) => !ALLOWED_ERRORS.some((re) => re.test(e)));
  if (bad.length) throw new Error(`${app}: unexpected errors:\n${bad.map((e) => e.slice(0, 600)).join('\n---\n')}`);
};
const lastNav = (s) => s.navs[s.navs.length - 1];
const navIs = (s, route, stack) => lastNav(s) && lastNav(s).route === route && (!stack || lastNav(s).stack.join() === stack.join());
const SPLASH = 4280322799; // #208AEF, the splash overlay background

async function tabsApp() {
  const s = session(ports.tabs);
  await s.until(() => /Welcome to.Expo/.test(s.text), 'Home tab text');
  ok('tabs: Home tab rendered');
  const host = find(s.tree, (v) => v.component === 'RNSTabsHostIOS');
  if (!host) throw new Error('no RNSTabsHostIOS');
  const screens = all(host, (v) => v.component === 'RNSTabsScreenIOS').map((v) => v.props.screenKey);
  if (screens.join() !== 'index,explore') throw new Error('tab screens: ' + screens);
  // splash: a plain View with onLayout -> hideAsync -> Reanimated Keyframe entering (600 ms) -> scheduleOnRN(setVisible, false)
  const splash = () => find(s.tree, (v) => v.props.backgroundColor === SPLASH && v.props.zIndex === 1000);
  await s.until(() => !splash(), 'splash overlay to disappear', 6000);
  ok('tabs: splash overlay removed after its entering animation');
  // the glow's endless Keyframe rotation reaches the tree through animated props
  const rot = () => { const g = find(s.tree, (v) => v.props.nativeID === '0'); return g && JSON.stringify(g.props.transform); };
  const r0 = rot();
  await s.until(() => rot() !== r0 && rot() !== undefined, 'animated transform to change');
  ok(`tabs: reanimated animation updates the tree (${r0} -> ${rot()})`);
  s.send({ type: 'select_tab', tag: +host.id, key: 'explore' });
  const selected = () => find(s.tree, (v) => v.component === 'RNSTabsHostIOS').props.navStateRequest;
  await s.until(() => /File-based routing/.test(s.text) && selected().selectedScreenKey === 'explore', 'Explore tab text + navStateRequest');
  await s.until(() => navIs(s, '/explore'), 'navigation.changed /explore');
  ok(`tabs: navigation.changed ${JSON.stringify(lastNav(s))}`);
  ok(`tabs: select_tab explore shows Explore (navStateRequest ${JSON.stringify(selected())})`);
  // FadeIn entering on content mounted later (Collapsible): runs through Reanimated's layout-animation path too
  s.midFade = false;
  s.tap('File-based routing');
  await s.until(() => /This app has two screens/.test(s.text), 'Collapsible content');
  await s.until(() => s.midFade, 'FadeIn to be seen mid-way');
  ok('tabs: Collapsible FadeIn entering animates (opacity strictly between 0 and 1 seen)');
  s.send({ type: 'select_tab', tag: +host.id, index: 0 });
  await s.until(() => selected().selectedScreenKey === 'index' && /Welcome to.Expo/.test(s.text), 'back to Home');
  await s.until(() => navIs(s, '/'), 'navigation.changed /');
  ok('tabs: select_tab by index goes back to Home (navigation.changed /)');
  noErrors(s, 'tabs');
  s.close();
}

async function stackApp() {
  const s = session(ports.stack);
  const list = () => /Apple.*Banana.*Cherry.*Mango.*Peach/.test(s.text) && !s.text.includes('You picked');
  await s.until(list, 'fruit list');
  const header = () => all(s.tree, (v) => v.component === 'RNSScreenStackHeaderConfig').map((v) => v.props.title);
  if (!header().includes('Fruits')) throw new Error('header title: ' + header());
  await s.until(() => navIs(s, '/', ['/']), 'navigation.changed / [/]');
  ok('stack: Fruits list, header title "Fruits", navigation.changed ' + JSON.stringify(lastNav(s)));
  s.tap('Apple');
  await s.until(() => s.text.includes('You picked Apple') && header().includes('Apple'), 'detail Apple + header title');
  await s.until(() => navIs(s, '/item/Apple', ['/', '/item/Apple']), 'navigation.changed /item/Apple');
  ok('stack: tap Apple pushes detail, header title "Apple", navigation.changed ' + JSON.stringify(lastNav(s)));
  s.tap('Go back');
  await s.until(() => list() && all(s.tree, (v) => v.component === 'RNSScreen').length === 1, 'list after Go back');
  await s.until(() => navIs(s, '/', ['/']), 'navigation.changed back to /');
  ok('stack: Go back pops to the list, navigation.changed ' + JSON.stringify(lastNav(s)));
  s.tap('Banana');
  await s.until(() => s.text.includes('You picked Banana') && header().includes('Banana'), 'detail Banana');
  await new Promise((r) => setTimeout(r, 600)); // let the push transition finish (appear events)
  const stack = find(s.tree, (v) => v.component === 'RNSScreenStack');
  s.send({ type: 'native_back', tag: +stack.id });
  await s.until(() => list() && all(s.tree, (v) => v.component === 'RNSScreen').length === 1, 'list after native_back');
  ok('stack: native_back pops (topDismissed reached react-navigation)');
  await new Promise((r) => setTimeout(r, 800));
  s.tap('Cherry');
  await s.until(() => s.text.includes('You picked Cherry'), 'push again after native pop');
  ok('stack: navigation still works after native_back');
  noErrors(s, 'stack');
  s.close();
}

try {
  await startMetro('expo-default', ports.tabs);
  await startMetro('expo-router-stack', ports.stack);
  await tabsApp();
  await stackApp();
  console.log('OK: router drivers passed');
  cleanup();
  process.exit(0);
} catch (e) { fail(e.message); }
