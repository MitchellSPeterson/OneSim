// Issue #129 check: node frameworks/react-native/guest/run-export.mjs [app=expo-prism]
// Exports the app the way scripts/serve-web.sh does (`expo export --no-bytecode`, minified, per platform), serves the
// bundle over HTTP and boots host.js on it (SIMCORE_BUNDLE_URL, no Metro). expo-prism mounts Animated views
// (useNativeDriver: false) on its first screen, so the minified Animated.flushQueue must not throw without
// NativeAnimatedModule; before the fix React unmounted the whole tree and the host got an empty RootView.
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { onCleanup, root, run, startHost } from './lib.mjs';

const app = process.argv[2] || 'expo-prism';
const expect = (c, m) => { if (!c) throw new Error(m); };

run(async () => {
  const out = mkdtempSync(join(tmpdir(), 'onesim-export-'));
  onCleanup(() => rmSync(out, { recursive: true, force: true }));
  for (const platform of ['ios', 'android']) {
    const dir = join(out, platform);
    const r = spawnSync('npx', ['expo', 'export', '--no-bytecode', '-p', platform, '--output-dir', dir], {
      cwd: join(root, 'examples', app), env: { ...process.env, CI: '1', EXPO_NO_TELEMETRY: '1' }, encoding: 'utf8',
    });
    expect(r.status === 0, `expo export -p ${platform} failed:\n${r.stderr || r.stdout}`);
    const jsDir = join(dir, '_expo/static/js', platform);
    const name = readdirSync(jsDir).find((f) => f.endsWith('.js'));
    const bundle = readFileSync(join(jsDir, name), 'utf8');
    // The case this test is for: identifiers mangled (`(0,u.default)(c,'Native animated ...')`, not `invariant.default`).
    expect(/\(0,[\w$]+\.default\)\([\w$]+,'Native animated module is not available'\)/.test(bundle), `${platform} bundle is not minified`);

    const server = createServer((_, res) => res.end(bundle));
    await new Promise((r) => server.listen(0, '127.0.0.1', r));
    onCleanup(() => server.close());
    const h = startHost({ SIMCORE_BUNDLE_URL: `http://127.0.0.1:${server.address().port}/index.js`, SIMCORE_PLATFORM: platform, SIMCORE_VIEWPORT: '402x874', SIMCORE_HMR: '0' });
    await h.until(() => h.errors.length || /PRISM STUDIO/.test(h.text), `${platform} first screen`, 60000);
    expect(!h.errors.length, `${platform}: ${h.errors[0]}`);
    expect(!/Animated\.flushQueue patch found no match/.test(h.stderr), `${platform}: flushQueue patch did not apply`);
    // The Animated orbs keep committing (JS driver): give them a few frames, then the tree must still be there.
    await new Promise((r) => setTimeout(r, 1500));
    expect(!h.errors.length && /PRISM STUDIO/.test(h.text), `${platform}: tree lost after animating: ${h.errors[0] || h.text.slice(0, 200)}`);
    console.log(`ok: ${platform} minified export renders with Animated (${h.versions.length} trees)`);
    h.kill();
    server.close();
  }
  console.log('OK: exported bundles render');
}, 600000);
