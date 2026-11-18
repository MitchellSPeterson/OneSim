// react-native-gesture-handler check (issue #10): node frameworks/react-native/guest/run-gestures.mjs [port=8230]
// Metro serves examples/expo-gestures; host.js gets raw `touch` start/move/end sequences (what the Rust host sends for a
// click / drag) and the driver asserts Gesture.Tap/Pan/LongPress (JS + Reanimated worklet callbacks), the legacy
// TapGestureHandler/PanGestureHandler, RectButton, RNGH Pressable and ReanimatedSwipeable.
import { join } from 'node:path';
import { root, run, sleep, startHost, startMetro, walk } from './lib.mjs';

const port = Number(process.argv[2]) || 8230;
const W = 393, H = 852;
const ok = (m) => console.log('ok:', m);

run(async () => {
  await startMetro(join(root, 'examples/expo-gestures'), port);
  const laidOut = new Set();
  const h = startHost({ SIMCORE_METRO_URL: `http://localhost:${port}`, SIMCORE_VIEWPORT: `${W}x${H}`, SIMCORE_HMR: '0' }, (s, m) => {
    const frames = {}; // the Rust host's layout stand-in: every new view gets the full screen (enough for onLayout / hit slop)
    walk(m.root, (v) => { if (!laidOut.has(v.id)) { laidOut.add(v.id); frames[v.id] = [0, 0, W, H]; } });
    if (Object.keys(frames).length) s.send({ type: 'layout', frames });
  });
  const until = async (pred, what, ms) => { await h.until(() => h.errors.length || pred(), what, ms); if (h.errors.length) throw new Error(h.errors[0]); };
  const byId = (id) => { let r; walk(h.tree, (v) => { if (v.props.testID === id) r = v; }); if (!r) throw new Error(`no testID ${id}`); return +r.id; };
  let ts = 0;
  const touch = (phase, tag, x, y) => h.send({ type: 'touch', phase, tag, x, y, lx: x, ly: y, timestamp: (ts = Math.max(ts + 16, Date.now())) }); // real time, like the host
  const tap = (id) => { const t = byId(id); touch('start', t, 20, 20); touch('end', t, 20, 20); };
  const drag = (id, dx, dy, steps = 8) => {
    const t = byId(id);
    touch('start', t, 200, 20);
    for (let i = 1; i <= steps; i++) touch('move', t, 200 + (dx * i) / steps, 20 + (dy * i) / steps);
    touch('end', t, 200 + dx, 20 + dy);
  };

  await until(() => /Taps: 0/.test(h.text) && /Swipe: closed/.test(h.text), 'first render', 120000);
  tap('tap'); tap('tap');
  await until(() => /Taps: 2/.test(h.text), 'Gesture.Tap x2'); ok('Gesture.Tap onEnd (runOnJS) counted 2 taps');

  drag('pan', 60, 30);
  await until(() => /Pan: end 60,30/.test(h.text), 'Gesture.Pan end'); ok('Gesture.Pan onBegin/onUpdate/onEnd: translation 60,30');
  { const t = byId('pan'); touch('start', t, 20, 20); touch('move', t, 24, 22); touch('end', t, 24, 22); }
  await sleep(300);
  if (!/Pan: began/.test(h.text)) throw new Error('a 4pt move should begin but not activate the pan: ' + h.text);
  ok('Gesture.Pan below minDist 10: began, never active');

  drag('uipan', 80, 0);
  const tx = () => { let v; walk(h.tree, (n) => { if (n.props.testID === 'uipan') v = n.props.transform; }); return v && v[0] && v[0].translateX; };
  await until(() => tx() === 80, 'worklet translateX 80'); ok('Gesture.Pan worklet onUpdate -> shared value -> transform translateX 80');

  { const t = byId('long'); touch('start', t, 20, 20); await sleep(300); if (/Long: yes/.test(h.text)) throw new Error('long press fired before 500ms'); await until(() => /Long: yes/.test(h.text), 'long press'); touch('end', t, 20, 20); }
  ok('Gesture.LongPress activates after minDuration 500ms');
  { const t = byId('tap'); touch('start', t, 20, 20); await sleep(600); touch('end', t, 20, 20); await sleep(300); }
  if (!/Taps: 2/.test(h.text)) throw new Error('a 600ms press must not count as a tap: ' + h.text);
  ok('Gesture.Tap fails past maxDuration 500ms');

  tap('legacytap');
  await until(() => /Legacy: tapped/.test(h.text), 'TapGestureHandler'); ok('legacy TapGestureHandler onHandlerStateChange END');
  drag('legacypan', 40, 0);
  await until(() => /Legacy: pan 40/.test(h.text), 'PanGestureHandler'); ok('legacy PanGestureHandler onGestureEvent translationX 40');

  tap('rect');
  await until(() => /Buttons: rect/.test(h.text), 'RectButton'); ok('RectButton onPress');
  tap('ghpress');
  await until(() => /Buttons: pressable/.test(h.text), 'RNGH Pressable'); ok('RNGH Pressable onPress');

  // every view is laid out full width here, so the action is 393pt wide: past half of it (with the toss) opens
  drag('row', -300, 0);
  await until(() => /Swipe: open left/.test(h.text), 'swipeable open');
  let rowX; walk(h.tree, (v) => { if (v.props.transform && /Swipe:/.test(JSON.stringify(v)) && !/Drag me/.test(JSON.stringify(v))) rowX = v.props.transform[0]?.translateX; });
  if (!(rowX <= -W + 1) || !/Delete/.test(h.text)) throw new Error(`row not slid open over "Delete": translateX ${rowX}`);
  ok(`ReanimatedSwipeable: swipe left opens the "Delete" action (onSwipeableOpen left, row translateX ${Math.round(rowX)})`);
  if (h.errors.length) throw new Error('unexpected errors: ' + h.errors.join('\n---\n').slice(0, 1500));
  h.send({ type: 'shutdown' });
  console.log('PASS');
}, 240000);
