// Milestone 4 check (native-module compatibility): node frameworks/react-native/guest/run-modules.mjs [port=8220]
// Boots examples/expo-modules (clipboard, haptics, location, camera, file system, secure store) and asserts the `modules` line, the simulated clipboard /
// haptics / location / permissions, host `simulate`, state surviving a reload, and the unsupported-module diagnostic.
// A Metro already listening on the port is reused (and left running).
import { join } from 'node:path';
import { root, run, startHost, startMetro } from './lib.mjs';

const port = Number(process.argv[2]) || 8220;
const ok = (m) => console.log('ok:', m);
run(async () => {
  await startMetro(join(root, 'examples/expo-modules'), port);
  const h = startHost({ SIMCORE_METRO_URL: `http://localhost:${port}`, SIMCORE_VIEWPORT: '402x874', SIMCORE_HMR: '0' });
  const out = () => (/Output: (.*?)Watch:/.exec(h.text) || [])[1];
  const press = async (label, re, what) => { h.tap(label); await h.until(() => re.test(out() || ''), what || `${label} -> ${re}`); ok(`${label}: ${out()}`); };
  await h.until(() => /Output: ready/.test(h.text) && h.modules.length, 'first tree + modules line');
  const mods = Object.fromEntries(h.modules[0].map((m) => [m.name, m]));
  const want = { ExpoClipboard: 'simulated', ExpoHaptics: 'simulated', ExpoLocation: 'simulated', ExpoCamera: 'unsupported', FileSystem: 'implemented', ExponentFileSystem: 'implemented', ExpoSecureStore: 'simulated', ExpoDevice: 'simulated', DeviceInfo: 'simulated', Appearance: 'simulated', RNCSafeAreaContext: 'simulated', Clipboard: 'simulated' };
  for (const [n, s] of Object.entries(want)) if (!mods[n] || mods[n].status !== s) throw new Error(`modules: ${n} should be ${s}, got ${mods[n] && mods[n].status}`);
  if (!mods.ExpoLocation.methods.includes('watchPositionImplAsync') || mods.ExpoLocation.package !== 'expo-location' || !mods.ExpoLocation.events.includes('Expo.locationChanged')) throw new Error('modules: ExpoLocation entry ' + JSON.stringify(mods.ExpoLocation));
  ok(`modules line: ${h.modules[0].length} modules; clipboard/haptics/location simulated, camera unsupported`);

  await press('Copy', /copied/);
  if (!h.events.some((e) => e.name === 'module.called' && e.detail.module === 'ExpoClipboard' && e.detail.method === 'setStringAsync' && e.detail.args[0] === 'hello')) throw new Error('no module.called for setStringAsync');
  await press('Paste', /pasted hello/);
  h.send({ type: 'simulate', module: 'clipboard', state: { text: 'from host' } });
  await press('Paste', /pasted from host/, 'Paste shows the simulated clipboard');

  const n0 = h.events.length;
  await press('Buzz', /buzzed/);
  if (!h.events.slice(n0).some((e) => e.name === 'module.called' && e.detail.module === 'ExpoHaptics' && e.detail.method === 'impactAsync')) throw new Error('no module.called for impactAsync');
  ok('haptics: module.called impactAsync ' + JSON.stringify(h.events.slice(n0).find((e) => e.name === 'module.called').detail));

  await press('Write file', /file hi/, 'Write file: FileSystem File create + write + text');
  await press('List docs', /docs note\.txt/);
  await press('Read secret', /secret null/, 'Read secret: nothing stored yet');
  await press('Save secret', /saved/);
  await press('Read secret', /secret abc/);
  h.send({ type: 'simulate', module: 'securestore', state: { token: 'seeded' } });
  await press('Read secret', /secret seeded/, 'Read secret shows the simulated value');

  await press('Locate', /permission granted lat 37\.3349 lon -122\.009/, 'Locate: granted + Apple Park');
  await press('Watch', /watching/);
  await h.until(() => /Watch: lat 37\.3349 lon -122\.009/.test(h.text), 'first watch fix');
  h.send({ type: 'simulate', module: 'location', state: { latitude: 48.8584, longitude: 2.2945 } });
  await h.until(() => /Watch: lat 48\.8584 lon 2\.2945/.test(h.text), 'watch update from simulate location');
  ok('watchPositionAsync: simulate location pushed to the watcher (' + /Watch: (lat \S+ lon [\d.-]+)/.exec(h.text)[1] + ')');

  // state set through `simulate` survives a reload (the supervisor replays it)
  h.send({ type: 'simulate', module: 'permissions', state: { location: 'denied' } });
  const nm = h.modules.length;
  h.send({ type: 'reload' });
  await h.until(() => h.modules.length > nm && /Output: ready/.test(h.text), 'reload + modules line again');
  await press('Locate', /permission denied/, 'Locate after denied + reload');
  // the keychain and the Documents folder outlive a reload; the replayed simulate state must not overwrite what the app saved since
  await press('Save secret', /saved/);
  const nm2 = h.modules.length;
  h.send({ type: 'reload' });
  await h.until(() => h.modules.length > nm2 && /Output: ready/.test(h.text), 'second reload');
  await press('Read secret', /secret abc/, 'Read secret after reload (replay did not overwrite it)');
  await press('List docs', /docs note\.txt/, 'Documents file survives the reload');

  const n1 = h.events.length, e1 = h.errors.length;
  h.tap('Camera');
  await h.until(() => /Output: error: SimCore: ExpoCamera is not supported\./.test(h.text), 'Camera error in the UI');
  ok('camera: UI shows ' + out());
  const diag = 'SimCore: ExpoCamera is not supported.\n\nModule: expo-camera\nMethod: requestCameraPermissionsAsync';
  if (!h.stderr.includes(diag)) throw new Error('stderr lacks the diagnostic:\n' + h.stderr.slice(-800));
  const un = h.events.slice(n1).filter((e) => e.name === 'module.unsupported');
  if (un.length !== 1 || JSON.stringify(un[0].detail) !== JSON.stringify({ module: 'ExpoCamera', method: 'requestCameraPermissionsAsync', package: 'expo-camera' })) throw new Error('module.unsupported: ' + JSON.stringify(un));
  if (h.errors.slice(e1).length !== 1 || !h.errors[e1].startsWith(diag)) throw new Error('error lines: ' + JSON.stringify(h.errors.slice(e1)));
  ok('camera: stderr diagnostic, one module.unsupported, one error line');
  const stray = h.errors.slice(0, e1);
  if (stray.length) throw new Error('unexpected errors:\n' + stray.map((e) => e.slice(0, 400)).join('\n---\n'));
  ok('no other errors');
  h.send({ type: 'shutdown' });
  console.log('PASS');
  }, 240000);
