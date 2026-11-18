// Milestone 1 check (core components): node frameworks/react-native/guest/run-components.mjs [port=8214]
// Metro serves examples/expo-components: "key:value" Text rows plus the TextInputs/ScrollView/FlatList/Switch/Image/Modal
// driven below. The driver runs a tiny HTTP server on :8097 (/net.png 7x5, /hello, /echo) that the app talks to.
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import zlib from 'node:zlib';
import { onCleanup, root, run, startHost, startMetro, walk, textOf } from './lib.mjs';

const port = Number(process.argv[2]) || 8214;
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

  await until(() => { const r = rows(); return find('RCTSwitch')[0] && focusReq() && scrollTo() && r.net && r.post && r.xhr && r.sizes && r.l1 && r.l2 && r.shown; }, 'static rows, switch, focus_request, scroll_to');
  const r = rows(), plain = find('RCTSinglelineTextInputView')[2];
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

  const [input, upper] = find('RCTSinglelineTextInputView'), [sv, fl] = find('RCTScrollView'), sw = find('RCTSwitch')[0];
  const tag = +input.id;
  if (focusReq().tag !== tag) throw new Error('focus_request tag ' + JSON.stringify(focusReq()));
  ok('ref.focus() -> focus_request');
  h.send({ type: 'focus', tag });
  await until(() => rows().foc === 'focus', 'onFocus'); ok('focus -> onFocus');
  h.send({ type: 'text_input', field: tag, version: 1, text: 'Hello' }); h.send({ type: 'text_input', field: +upper.id, version: 1, text: 'abc' });
  const cur = (t) => find('RCTSinglelineTextInputView').find((v) => v.id === t.id);
  await until(() => rows().text === 'Hello' && cur(input).props.__simcoreText === 'Hello', 'onChangeText'); ok('text_input -> onChangeText + __simcoreText');
  await until(() => rows().up === 'ABC' && cur(upper).props.__simcoreText === 'ABC', 'controlled rewrite'); ok('controlled value rewrite via setTextAndSelection');
  h.send({ type: 'submit', tag });
  await until(() => rows().sub === 'Hello' && rows().foc === 'blur', 'submit'); ok('submit -> onSubmitEditing + blur');
  h.send({ type: 'layout', frames: { [sv.id]: [0, 300, 393, 200], [fl.id]: [0, 500, 393, 200] } });
  await until(() => rows().fl === '393x200', 'FlatList onLayout'); ok('layout -> FlatList onLayout');
  for (const [phase, y] of [['begin', 0], ['scroll', 120], ['end', 120]]) h.send({ type: 'scroll', tag: +sv.id, phase, x: 0, y, contentWidth: 393, contentHeight: 1000, width: 393, height: 200 });
  for (const [phase, y] of [['begin', 0], ['scroll', 340], ['end', 340]]) h.send({ type: 'scroll', tag: +fl.id, phase, x: 0, y, contentWidth: 393, contentHeight: 2000, width: 393, height: 200 });
  await until(() => rows().y === '120' && rows().fy === '340', 'onScroll'); ok('scroll -> onScroll (ScrollView + FlatList)');
  h.send({ type: 'touch', phase: 'start', tag: +sw.id, x: 10, y: 10 }); h.send({ type: 'touch', phase: 'end', tag: +sw.id, x: 10, y: 10 });
  await until(() => rows().sw === 'true', 'Switch'); ok('Switch tap -> onValueChange');
  console.log('OK: milestone 1 checks passed');
}, 90000);
