// Standalone check: node frameworks/react-native/guest/run-standalone.mjs [metroUrl]
// Spawns host.js against Metro (default :8081), taps the Pressable, asserts the counter text incremented.
import { run, startHost } from './lib.mjs';

const metro = process.argv[2]?.startsWith('http') ? process.argv[2] : 'http://localhost:8081';
run(async () => {
  const h = startHost({ SIMCORE_METRO_URL: metro, SIMCORE_VIEWPORT: '393x852', SIMCORE_APPEARANCE: 'light' });
  const noErr = () => { if (h.errors.length) throw new Error(h.errors[0]); };
  await h.until(() => h.errors.length || /Count: ?0/.test(h.text), 'initial tree', 60000); noErr();
  h.tap('Increment');
  await h.until(() => h.errors.length || /Count: ?1/.test(h.text), 'Count: 1', 60000); noErr();
  console.log('OK: counter incremented');
}, 60000);
