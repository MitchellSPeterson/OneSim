// Standalone check: node frameworks/react-native/guest/run-standalone.mjs [metroUrl] [--text "Count: 0"]
// Spawns host.js against Metro, taps the Pressable, asserts the counter text incremented.
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

const metro = process.argv[2]?.startsWith('http') ? process.argv[2] : 'http://localhost:8081';
const host = fileURLToPath(new URL('./host.js', import.meta.url));
const p = spawn('node', [host], {
  env: { ...process.env, SIMCORE_METRO_URL: metro, SIMCORE_VIEWPORT: '393x852', SIMCORE_APPEARANCE: 'light' },
  stdio: ['pipe', 'pipe', 'inherit'],
});
const send = (m) => p.stdin.write(JSON.stringify(m) + '\n');
const walk = (n, f) => { f(n); n.children.forEach((c) => walk(c, f)); };
const texts = (root) => { const t = []; walk(root, (n) => n.component === 'RCTRawText' && t.push(n.props.text ?? n.props.children)); return t.join(''); };
const fail = (m) => { console.error('FAIL:', m); p.kill(); process.exit(1); };
const timer = setTimeout(() => fail('timeout'), 60000);

let n = 0, tapped = false;
createInterface({ input: p.stdout }).on('line', (line) => {
  const m = JSON.parse(line);
  if (m.type === 'error') return fail(m.message);
  if (m.type !== 'tree') return;
  const t = texts(m.root);
  console.log(`tree v${m.version}: ${t}`);
  if (!tapped) {
    if (!/Count: ?0/.test(t)) return; // wait for the real tree
    // deepest view whose props mark it as touch-responsive and whose subtree has "Increment"
    let target; walk(m.root, (v) => { if (v.component === 'RCTView' && v.props.onClick && texts(v).includes('Increment')) target = v; });
    if (!target) return fail('no Pressable view found');
    tapped = true; n = m.version;
    const tag = +target.id, ts = Date.now();
    send({ type: 'touch', phase: 'start', tag, x: 100, y: 100, lx: 5, ly: 5, timestamp: ts });
    setTimeout(() => send({ type: 'touch', phase: 'end', tag, x: 100, y: 100, lx: 5, ly: 5, timestamp: ts + 80 }), 100);
  } else if (/Count: ?1/.test(t)) {
    console.log('OK: counter incremented');
    clearTimeout(timer); send({ type: 'shutdown' }); p.on('exit', () => process.exit(0));
  }
});
p.on('exit', (c) => c && fail(`host exited ${c}`));
