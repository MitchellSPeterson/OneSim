// Fast Refresh check: node frameworks/react-native/guest/run-refresh.mjs [port=8210]
// Copies examples/expo-counter to a temp dir (node_modules symlinked), runs Metro in watch mode + host.js, then edits
// App.js and asserts: state-preserving refresh, syntax-error handling + recovery, render-error recovery,
// full reload when Refresh bails out (non-component export), manual {"type":"reload"}. Prints refresh latency.
import { cpSync, mkdtempSync, readFileSync, symlinkSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { onCleanup, root, run, sleep, startHost, startMetro } from './lib.mjs';

const port = Number(process.argv[2]) || 8210;
const dir = mkdtempSync(join(tmpdir(), 'simcore-refresh-'));
onCleanup(() => rmSync(dir, { recursive: true, force: true }));
for (const f of ['App.js', 'app.json', 'index.js', 'package.json']) cpSync(join(root, 'examples/expo-counter', f), join(dir, f));
symlinkSync(join(root, 'examples/expo-counter/node_modules'), join(dir, 'node_modules'));
const appFile = join(dir, 'App.js');
const original = readFileSync(appFile, 'utf8');
const ok = (m) => console.log('ok:', m);

run(async () => {
  await startMetro(dir, port, { watch: true });
  const h = startHost({ SIMCORE_METRO_URL: `http://localhost:${port}`, SIMCORE_VIEWPORT: '393x852' });
  const has = (name) => h.events.some((e) => e.name === name);
  await h.until(() => /Count: ?0/.test(h.text), 'initial tree', 90000);
  if (!has('bundle.loaded')) throw new Error('no bundle.loaded event');
  ok('initial tree + bundle.loaded');
  for (const n of [1, 2]) { h.tap('Increment'); await h.until(() => h.text.includes(`Count: ${n}`), `Count: ${n}`); }
  ok('counter at 2');

  // 1. state-preserving refresh, 5 edits -> latency
  const lat = [];
  let label = 'Increment';
  for (let i = 0; i < 5; i++) {
    const next = i === 0 ? 'Add one' : `Add ${i + 1}`;
    const t0 = performance.now();
    writeFileSync(appFile, original.replace('>Increment<', `>${next}<`));
    await h.until(() => h.text.includes(next), `label "${next}"`);
    lat.push(Math.round(performance.now() - t0));
    if (!h.text.includes('Count: 2')) throw new Error('state lost on refresh: ' + h.text);
    label = next;
  }
  if (!has('refresh.applied')) throw new Error('no refresh.applied event');
  lat.sort((a, b) => a - b);
  ok(`refresh keeps state (label + Count: 2); latency ms sorted=${lat} median=${lat[2]}`);
  const good = original.replace('>Increment<', `>${label}<`);

  // 2. syntax error: error reported, tree unchanged; fix -> recovery
  let before = h.text, nerr = h.errors.length, nver = h.versions.length;
  writeFileSync(appFile, good.replace('<Text style={styles.title}>', '<Text style={styles.title'));
  await h.until(() => h.errors.length > nerr, 'syntax error message');
  if (!has('bundle.error')) throw new Error('no bundle.error event');
  await sleep(1500);
  if (h.text !== before) throw new Error('tree changed after syntax error: ' + h.text);
  ok('syntax error -> error: ' + h.errors[h.errors.length - 1].split('\n')[0].slice(0, 100));
  writeFileSync(appFile, good.replace('Count: {count}', 'Count: {count}')); // fixed
  await h.until(() => h.text.includes('Count: 2') && h.errors.length >= nerr, 'recovery after syntax fix');
  h.tap(label); await h.until(() => h.text.includes('Count: 3'), 'taps work after recovery');
  ok('recovered after syntax fix (state preserved, taps work)');

  // 3. render error, then fix
  nerr = h.errors.length;
  writeFileSync(appFile, good.replace('return (', "if (count >= 3) throw new Error('boom from render');\n  return ("));
  await h.until(() => h.errors.some((e, i) => i >= nerr && /boom from render/.test(e)), 'render error message');
  ok('render error reported');
  await sleep(1000); // let Metro finish the in-flight update before the next edit
  await h.until(() => h.text === '', 'root unmounted by render error', 10000).catch(() => {});
  console.log('   (after render error tree is ' + JSON.stringify(h.text) + ')');
  writeFileSync(appFile, good);
  await h.until(() => h.text.includes(label) && /Count: ?\d/.test(h.text), 'recovery after render error', 15000);
  ok('recovered after render error: ' + h.text);

  // 4. Refresh bail-out (non-component export next to a component) -> full reload, state reset
  const gen = h.events.filter((e) => e.name === 'bundle.reloaded').length;
  if (!/Count: ?[1-9]/.test(h.text)) { h.tap(label); await h.until(() => /Count: ?[1-9]/.test(h.text), 'count > 0'); }
  writeFileSync(appFile, good + '\nexport const NOT_A_COMPONENT = 1;\n');
  await h.until(() => h.events.filter((e) => e.name === 'bundle.reloaded').length > gen && /Count: ?0/.test(h.text), 'full reload after bail-out');
  ok('non-component export -> full reload (Count: 0, bundle.reloaded)');
  writeFileSync(appFile, good);
  await h.until(() => h.text.includes(label), 'refresh works after reload');
  ok('Fast Refresh still works after a full reload');

  // 5. manual reload
  h.tap(label); await h.until(() => /Count: ?1/.test(h.text), 'Count: 1');
  const gen2 = h.events.filter((e) => e.name === 'bundle.reloaded').length, lastV = h.versions[h.versions.length - 1];
  h.send({ type: 'reload' });
  await h.until(() => h.events.filter((e) => e.name === 'bundle.reloaded').length > gen2 && /Count: ?0/.test(h.text) && h.versions[h.versions.length - 1] > lastV, 'manual reload');
  if (h.versions.some((v, i) => i && v <= h.versions[i - 1])) throw new Error('tree versions not monotonic');
  ok('{"type":"reload"} -> Count: 0, versions monotonic');
  console.log(`OK: fast refresh checks passed; refresh latency median ${lat[2]} ms (${lat.join(', ')})`);
}, 240000);
