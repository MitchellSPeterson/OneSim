// Milestone 1 check (core components): node frameworks/react-native/guest/run-components.mjs [port=8214]
// Metro serves examples/expo-components: "key:value" Text rows plus the TextInputs/ScrollView/FlatList/Switch/Image/Modal
// driven below. The driver runs a tiny HTTP server on :8097 (/net.png 7x5, /hello, /echo) that the app talks to.
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import zlib from 'node:zlib';
import { onCleanup, root, run, startHost, startMetro, walk, textOf } from './lib.mjs';

const port = Number(process.argv[2]) || 8214;
const android = process.env.SIMCORE_PLATFORM === 'android';
const inputName = android ? 'AndroidTextInput' : 'RCTSinglelineTextInputView';
const switchName = android ? 'AndroidSwitch' : 'RCTSwitch';
const png = (w, h) => { // 8-bit RGB, all-grey; enough for a header + valid zlib stream
  const crcT = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
  const crc = (b) => { let c = 0xffffffff; for (const x of b) c = crcT[(c ^ x) & 255] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
  const chunk = (t, d) => { const l = Buffer.alloc(4), c = Buffer.alloc(4), td = Buffer.concat([Buffer.from(t), d]); l.writeUInt32BE(d.length); c.writeUInt32BE(crc(td)); return Buffer.concat([l, td, c]); };
  const ih = Buffer.alloc(13); ih.writeUInt32BE(w, 0); ih.writeUInt32BE(h, 4); ih[8] = 8; ih[9] = 2;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ih), chunk('IDAT', zlib.deflateSync(Buffer.alloc((w * 3 + 1) * h, 0x80))), chunk('IEND', Buffer.alloc(0))]);
};
const srv = createServer((req, res) => {
  if (req.url === '/net.png') return res.writeHead(200, { 'content-type': 'image/png' }).end(png(7, 5));
  if (req.url === '/hello') return res.writeHead(200, { 'content-type': 'text/plain' }).end('hello world');
  if (req.url === '/echo') { let b = ''; req.on('data', (d) => (b += d)); return req.on('end', () => res.writeHead(200, { 'content-type': 'application/json' }).end(JSON.stringify({ got: JSON.parse(b), method: req.method }))); }
  res.writeHead(404).end();
}).listen(8097, '127.0.0.1');
onCleanup(() => srv.close());

const ok = (m) => console.log('ok:', m);
run(async () => {
  await startMetro(join(root, 'examples/expo-components'), port);
  const h = startHost({ SIMCORE_METRO_URL: `http://localhost:${port}`, SIMCORE_VIEWPORT: '393x852', SIMCORE_APPEARANCE: 'light' });
  const find = (comp) => { const r = []; if (h.tree) walk(h.tree, (v) => v.component === comp && r.push(v)); return r; };
  const rows = () => { const r = {}; find('RCTText').forEach((v) => { const t = textOf(v), i = t.indexOf(':'); if (i > 0) r[t.slice(0, i)] = t.slice(i + 1); }); return r; };
  const focusReq = () => h.other.find((m) => m.type === 'focus_request' && m.tag != null);
  const scrollTo = () => h.other.filter((m) => m.type === 'scroll_to').pop();
  // wait for pred; any host `error` message fails immediately
  const until = async (pred, what) => { await h.until(() => h.errors.length || pred(), what, 90000); if (h.errors.length) throw new Error(h.errors[0]); };
  const imgs = () => find('RCTImageView');
  const img = (w, ht) => imgs().some((i) => i.props.__simcoreImage?.width === w && i.props.__simcoreImage?.height === ht);

  await until(() => { const r = rows(); return find(switchName)[0] && focusReq() && scrollTo() && r.net && r.post && r.xhr && r.sizes && r.l1 && r.l2 && r.shown; }, 'static rows, switch, focus_request, scroll_to');
  const r = rows(), plain = find(inputName)[2];
  const must = (cond, name) => { if (!cond) throw new Error('check failed: ' + name + '; rows=' + JSON.stringify(rows())); ok(name); };
  if (plain) must(plain.props.__simcoreText === 'abc', 'defaultValue -> __simcoreText');
  must(r.net === 'packager-status:running', 'fetch(metro/status)');
  must(r.post === '{"got":{"a":1},"method":"POST"}', 'fetch POST json');
  must(r.xhr === '200:hello world', 'XMLHttpRequest');
  must(r.sizes === '7x5', 'Image.getSize');
  must(r.l2 === '7x5' && img(7, 5), 'network Image __simcoreImage 7x5 + onLoad');
  must(r.l1 === '20' && img(20, 10), 'require()d PNG __simcoreImage 20x10 + onLoad');
  for (const i of imgs()) if (i.props.__simcoreImage && readFileSync(i.props.__simcoreImage.path).readUInt32BE(0) !== 0x89504e47) throw new Error('cached file is not a PNG');
  must(r.shown === 'yes', 'Modal topShow');
  must(scrollTo().y === 50 && scrollTo().x === 0, 'scrollTo command -> scroll_to');

  const [input, upper] = find(inputName), [sv, fl] = find('RCTScrollView'), sw = find(switchName)[0];
  const tag = +input.id;
  if (focusReq().tag !== tag) throw new Error('focus_request tag ' + JSON.stringify(focusReq()));
  ok('ref.focus() -> focus_request');
  h.send({ type: 'focus', tag });
  await until(() => rows().foc === 'focus', 'onFocus'); ok('focus -> onFocus');
  h.send({ type: 'keyboard', height: 335 }); // 852 - 335 = 517
  const didShow = 'DidShow:335@517:true,', didHide = 'DidHide:335@852:false,';
  const show = android ? didShow : 'WillShow:335@517:false,' + didShow;
  const hide = android ? didHide : 'WillHide:335@852:true,' + didHide;
  await until(() => rows().kb === show, 'keyboard show events'); ok('keyboard show events (height, screenY, Keyboard.isVisible)');
  h.tap('dismiss');
  await until(() => h.other.some((m) => m.type === 'focus_request' && m.tag === null), 'Keyboard.dismiss'); ok('Keyboard.dismiss() -> focus_request null');
  h.send({ type: 'text_input', field: tag, version: 1, text: 'Hello' }); h.send({ type: 'text_input', field: +upper.id, version: 1, text: 'abc' });
  const cur = (t) => find(inputName).find((v) => v.id === t.id);
  await until(() => rows().text === 'Hello' && cur(input).props.__simcoreText === 'Hello', 'onChangeText'); ok('text_input -> onChangeText + __simcoreText');
  await until(() => rows().up === 'ABC' && cur(upper).props.__simcoreText === 'ABC', 'controlled rewrite'); ok('controlled value rewrite via setTextAndSelection');
  h.send({ type: 'submit', tag });
  await until(() => rows().sub === 'Hello' && rows().foc === 'blur', 'submit'); ok('submit -> onSubmitEditing + blur');
  h.send({ type: 'keyboard', height: 0 });
  await until(() => rows().kb === show + hide, 'keyboard hide events'); ok('keyboard hide events');
  h.send({ type: 'layout', frames: { [sv.id]: [0, 300, 393, 200], [fl.id]: [0, 500, 393, 200] } });
  await until(() => rows().fl === '393x200', 'FlatList onLayout'); ok('layout -> FlatList onLayout');
  for (const [phase, y] of [['begin', 0], ['scroll', 120], ['end', 120]]) h.send({ type: 'scroll', tag: +sv.id, phase, x: 0, y, contentWidth: 393, contentHeight: 1000, width: 393, height: 200 });
  for (const [phase, y] of [['begin', 0], ['scroll', 340], ['end', 340]]) h.send({ type: 'scroll', tag: +fl.id, phase, x: 0, y, contentWidth: 393, contentHeight: 2000, width: 393, height: 200 });
  await until(() => rows().y === '120' && rows().fy === '340', 'onScroll'); ok('scroll -> onScroll (ScrollView + FlatList)');
  h.send({ type: 'touch', phase: 'start', tag: +sw.id, x: 10, y: 10 }); h.send({ type: 'touch', phase: 'end', tag: +sw.id, x: 10, y: 10 });
  await until(() => rows().sw === 'true', 'Switch'); ok('Switch tap -> onValueChange');

  if (android) {
    must(rows().platform === 'android 37 Pixel 10a', 'Android Platform constants');
    must(rows().dimensions === '393x852@2.625', 'Android dimensions and pixel density');
    h.send({ type: 'environment', appearance: 'light', viewport: { width: 400, height: 850 } });
    await until(() => rows().dimensions === '400x850@2.625', 'Android dimensions change');
    must(find('AndroidProgressBar').length === 1, 'Android ActivityIndicator');
    const horizontal = find('AndroidHorizontalScrollView')[0];
    must(horizontal && find('AndroidHorizontalScrollContentView').length === 1, 'Android horizontal ScrollView');
    h.tap('Jump horizontal');
    await until(() => scrollTo()?.tag === +horizontal.id, 'horizontal scroll_to');
    must(scrollTo().x === 60 && scrollTo().y === 0, 'horizontal scrollTo offsets');
    h.send({ type: 'scroll', tag: +horizontal.id, phase: 'end', x: 60, y: 0, contentWidth: 1000, contentHeight: 40, width: 400, height: 40 });
    await until(() => rows().x === '60', 'horizontal onScroll');
    console.log('OK: Android core component checks passed');
    return;
  }

  // Alert.alert (examples/expo-showcase, Controls): an `alert` presentation layer; its button's action runs the callback.
  await startMetro(join(root, 'examples/expo-showcase'), port + 1);
  const s = startHost({ SIMCORE_METRO_URL: `http://localhost:${port + 1}`, SIMCORE_VIEWPORT: '402x874', SIMCORE_APPEARANCE: 'light' });
  await s.until(() => /Controls/.test(s.text), 'showcase home', 90000); s.tap('Controls>');
  await s.until(() => /Count: 0/.test(s.text), 'Controls'); s.tap('Increment');
  await s.until(() => /Count: 1/.test(s.text), 'Count: 1'); s.tap('Count: 1');
  await s.until(() => s.layers.length === 1, 'alert layer');
  const al = s.layers[0], roles = al.actions.map((b) => `${b.label.text}:${b.role}`).join();
  if (al.kind !== 'alert' || al.title !== 'Reset count?' || al.message?.text !== 'The count goes back to zero.' || roles !== 'Cancel:cancel,Reset:destructive') throw new Error('alert layer ' + JSON.stringify(al));
  ok('Alert.alert -> alert layer (title, message, cancel/destructive roles)');
  s.send({ type: 'action', id: al.actions[1].action, version: 0 });
  await s.until(() => !s.layers.length && /Count: 0/.test(s.text), 'Reset callback'); ok('alert button action -> onPress callback, layer dropped');

  // KeyboardAvoidingView (showcase Form, opened by tapping the home title): keyboard -> bottom padding = keyboard overlap.
  s.tap('Back');
  await s.until(() => /Showcase/.test(s.text), 'home');
  s.tap('Showcase');
  await s.until(() => /Message/.test(s.text), 'Form');
  const kav = () => { let r; walk(s.tree, (v) => { if (v.props.justifyContent === 'flex-end' && v.props.padding === 16) r = v; }); return r; };
  const parentOf = (t) => { let p; walk(s.tree, (v) => { if (v.children?.some((c) => c.id === t)) p = v; }); return p; };
  const k = kav(), msg = (() => { let r; walk(s.tree, (v) => { if (v.component === 'RCTSinglelineTextInputView') r = v; }); return r; })();
  s.send({ type: 'layout', frames: { [parentOf(k.id).id]: [0, 62, 402, 812], [k.id]: [0, 106, 402, 768] } });
  s.send({ type: 'focus', tag: +msg.id }); s.send({ type: 'keyboard', height: 335 });
  await s.until(() => kav()?.props.paddingBottom === 335, 'KeyboardAvoidingView padding'); ok('keyboard -> KeyboardAvoidingView paddingBottom 335 (behavior padding)');
  s.send({ type: 'blur', tag: +msg.id }); s.send({ type: 'keyboard', height: 0 });
  await s.until(() => kav()?.props.paddingBottom === 0, 'KeyboardAvoidingView padding 0'); ok('keyboard hide -> paddingBottom 0');
  if (s.errors.length) throw new Error(s.errors[0]);
  console.log('OK: milestone 1 checks passed');
}, 90000);
