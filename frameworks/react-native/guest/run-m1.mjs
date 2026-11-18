// Milestone 1 check: node frameworks/react-native/guest/run-m1.mjs [metroUrl]
// Metro must serve a test app that renders "key:value" Text rows plus the TextInputs/ScrollView/FlatList/Switch/
// Image/Modal the checks below drive (the app used lives outside the repo). The driver starts a tiny HTTP server on
// :8097 (/net.png 7x5, /hello, /echo) that the app talks to.
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import zlib from 'node:zlib';

const metro = process.argv[2] || 'http://localhost:8090';
const host = fileURLToPath(new URL('./host.js', import.meta.url));
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

const p = spawn('node', [host], {
  env: { ...process.env, SIMCORE_METRO_URL: metro, SIMCORE_VIEWPORT: '393x852', SIMCORE_APPEARANCE: 'light' },
  stdio: ['pipe', 'pipe', 'inherit'],
});
const send = (m) => p.stdin.write(JSON.stringify(m) + '\n');
const walk = (n, f) => { f(n); n.children.forEach((c) => walk(c, f)); };
const texts = (n) => { const t = []; walk(n, (v) => v.component === 'RCTRawText' && t.push(v.props.text ?? v.props.children)); return t.join(''); };
const done = (code) => { srv.close(); p.kill(); process.exit(code); };
const fail = (m) => { console.error('FAIL:', m); done(1); };
const timer = setTimeout(() => fail('timeout; last rows: ' + JSON.stringify(rows)), 90000);

let tree, rows = {}, step = 0, focusReq = null, scrollTo = null, checked = new Set();
const find = (comp) => { const r = []; walk(tree, (v) => v.component === comp && r.push(v)); return r; };
const ok = (name) => { if (!checked.has(name)) { checked.add(name); console.log('ok:', name); } };
const expect = (cond, name) => (cond ? ok(name) : false);

function advance() {
  const [input, upper, plain] = find('RCTSinglelineTextInputView');
  const [sv, fl] = find('RCTScrollView');
  const sw = find('RCTSwitch')[0];
  const imgs = find('RCTImageView');
  // static checks (no input needed)
  if (plain) expect(plain.props.__simcoreText === 'abc', 'defaultValue -> __simcoreText');
  expect(rows.net === 'packager-status:running', 'fetch(metro/status)');
  expect(rows.post === '{"got":{"a":1},"method":"POST"}', 'fetch POST json');
  expect(rows.xhr === '200:hello world', 'XMLHttpRequest');
  expect(rows.sizes === '7x5', 'Image.getSize');
  expect(rows.l2 === '7x5' && imgs.some((i) => i.props.__simcoreImage?.width === 7 && i.props.__simcoreImage?.height === 5), 'network Image __simcoreImage 7x5 + onLoad');
  expect(rows.l1 === '20' && imgs.some((i) => i.props.__simcoreImage?.width === 20 && i.props.__simcoreImage?.height === 10), 'require()d PNG __simcoreImage 20x10 + onLoad');
  for (const i of imgs) if (i.props.__simcoreImage && readFileSync(i.props.__simcoreImage.path).readUInt32BE(0) !== 0x89504e47) fail('cached file is not a PNG');
  expect(rows.shown === 'yes', 'Modal topShow');
  expect(scrollTo && scrollTo.y === 50 && scrollTo.x === 0, 'scrollTo command -> scroll_to');
  if (!input || !sv || !fl || !sw || !focusReq || checked.size < 9) return;
  const tag = +input.id;
  if (focusReq.tag !== tag) return fail('focus_request tag ' + JSON.stringify(focusReq));
  ok('ref.focus() -> focus_request');
  if (step === 0) { step = 1; send({ type: 'focus', tag }); }
  else if (step === 1 && rows.foc === 'focus') { ok('focus -> onFocus'); step = 2; send({ type: 'text_input', field: tag, version: 1, text: 'Hello' }); send({ type: 'text_input', field: +upper.id, version: 1, text: 'abc' }); }
  else if (step === 2 && rows.text === 'Hello' && input.props.__simcoreText === 'Hello') {
    ok('text_input -> onChangeText + __simcoreText');
    if (rows.up !== 'ABC' || upper.props.__simcoreText !== 'ABC') return;
    ok('controlled value rewrite via setTextAndSelection');
    step = 3; send({ type: 'submit', tag });
  } else if (step === 3 && rows.sub === 'Hello' && rows.foc === 'blur') {
    ok('submit -> onSubmitEditing + blur'); step = 4;
    send({ type: 'layout', frames: { [sv.id]: [0, 300, 393, 200], [fl.id]: [0, 500, 393, 200] } });
  } else if (step === 4 && rows.fl === '393x200') {
    ok('layout -> FlatList onLayout'); step = 5;
    for (const [phase, y] of [['begin', 0], ['scroll', 120], ['end', 120]]) send({ type: 'scroll', tag: +sv.id, phase, x: 0, y, contentWidth: 393, contentHeight: 1000, width: 393, height: 200 });
    for (const [phase, y] of [['begin', 0], ['scroll', 340], ['end', 340]]) send({ type: 'scroll', tag: +fl.id, phase, x: 0, y, contentWidth: 393, contentHeight: 2000, width: 393, height: 200 });
  } else if (step === 5 && rows.y === '120' && rows.fy === '340') { ok('scroll -> onScroll (ScrollView + FlatList)'); step = 6; send({ type: 'touch', phase: 'start', tag: +sw.id, x: 10, y: 10 }); send({ type: 'touch', phase: 'end', tag: +sw.id, x: 10, y: 10 }); }
  else if (step === 6 && rows.sw === 'true') { ok('Switch tap -> onValueChange'); console.log('OK: milestone 1 checks passed'); clearTimeout(timer); send({ type: 'shutdown' }); done(0); }
}

createInterface({ input: p.stdout }).on('line', (line) => {
  const m = JSON.parse(line);
  if (m.type === 'error') return fail(m.message);
  if (m.type === 'scroll_to') { scrollTo = m; return advance(); }
  if (m.type === 'focus_request') { if (!focusReq && m.tag != null) focusReq = m; return advance(); }
  if (m.type !== 'tree') return;
  tree = m.root;
  rows = {};
  walk(tree, (v) => { if (v.component === 'RCTText') { const t = texts(v), i = t.indexOf(':'); if (i > 0) rows[t.slice(0, i)] = t.slice(i + 1); } });
  advance();
});
p.on('exit', (c) => c && fail(`host exited ${c}`));
