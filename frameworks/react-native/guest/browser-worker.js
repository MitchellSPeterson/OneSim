// OneSim React Native guest in a browser Web Worker (classic worker script, Chrome / Safari 16.4+).
// Runs the same host.js + siblings as Node: this file provides the Node built-ins they use (a CommonJS `require`,
// in-memory fs, path, os, crypto, util, url, vm, worker_threads, process, Buffer) and nothing else.
// Page side: web/expo/guest.js (startGuest). First message is
//   {type:'__init', env:{SIMCORE_...}, pageOrigin, workerData:{generation, dataDir}, files: Map(path -> entry)}
// then protocol messages (docs/protocol.md §H). fs writes are mirrored to the page as {type:'__fs', path, entry}
// (entry null = deleted) so the session's files survive reloads, like the Node guest's per-session data dir.
'use strict';
(() => {
  // Natives captured before RN's polyfills replace the globals of the same name.
  const NativeURL = URL, Decoder = TextDecoder, Encoder = TextEncoder;
  const out = console.log.bind(console), err = console.error.bind(console);
  const post = self.postMessage.bind(self);

  // ---- Buffer: the subset the guest files use ----
  const b64clean = (s) => String(s).replace(/-/g, '+').replace(/_/g, '/').replace(/[^A-Za-z0-9+/]/g, ''); // atob accepts missing padding
  const latin1 = (u8) => { let s = ''; for (let i = 0; i < u8.length; i += 0x8000) s += String.fromCharCode.apply(null, u8.subarray(i, i + 0x8000)); return s; };
  class Buffer extends Uint8Array {
    static from(x, enc, len) {
      if (typeof x === 'string') {
        if (enc === 'base64' || enc === 'base64url') { const s = atob(b64clean(x)), b = new Buffer(s.length); for (let i = 0; i < s.length; i++) b[i] = s.charCodeAt(i); return b; }
        if (enc === 'latin1' || enc === 'binary' || enc === 'ascii') { const b = new Buffer(x.length); for (let i = 0; i < x.length; i++) b[i] = x.charCodeAt(i) & 0xff; return b; }
        if (enc === 'hex') { const b = new Buffer(x.length >> 1); for (let i = 0; i < b.length; i++) b[i] = parseInt(x.substr(i * 2, 2), 16); return b; }
        const u = new Encoder().encode(x); return new Buffer(u.buffer, u.byteOffset, u.byteLength);
      }
      if (x instanceof ArrayBuffer || (typeof SharedArrayBuffer !== 'undefined' && x instanceof SharedArrayBuffer)) return new Buffer(x, enc || 0, len ?? x.byteLength - (enc || 0)); // a view, like Node
      return new Buffer(x); // copy of an array / typed array
    }
    static alloc(n) { return new Buffer(n); }
    static isBuffer(b) { return b instanceof Buffer; }
    static byteLength(s) { return typeof s === 'string' ? new Encoder().encode(s).length : s.byteLength; }
    static concat(list, total) {
      const b = new Buffer(total ?? list.reduce((n, x) => n + x.length, 0));
      let o = 0; for (const x of list) { if (o >= b.length) break; b.set(x.subarray(0, b.length - o), o); o += x.length; }
      return b;
    }
    toString(enc = 'utf8', s = 0, e = this.length) {
      const v = this.subarray(s, e);
      if (enc === 'base64') return btoa(latin1(v));
      if (enc === 'base64url') return btoa(latin1(v)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
      if (enc === 'latin1' || enc === 'binary' || enc === 'ascii') return latin1(v);
      if (enc === 'hex') return Array.from(v, (x) => x.toString(16).padStart(2, '0')).join('');
      return new Decoder().decode(v);
    }
    get _dv() { return new DataView(this.buffer, this.byteOffset, this.byteLength); }
    readUInt32BE(o) { return this._dv.getUint32(o); }
    readUInt32LE(o) { return this._dv.getUint32(o, true); }
    readUInt16BE(o) { return this._dv.getUint16(o); }
    readUInt16LE(o) { return this._dv.getUint16(o, true); }
    readUIntLE(o, n) { let v = 0; for (let i = n - 1; i >= 0; i--) v = v * 256 + this[o + i]; return v; }
  }

  // ---- crypto: randomUUID / randomFillSync / createHash('sha1') ----
  function sha1(m) {
    const l = m.length, w = new Uint32Array(80), n = ((l + 8) >> 6) + 1, p = new Uint8Array(n * 64);
    p.set(m); p[l] = 0x80; const dv = new DataView(p.buffer); dv.setUint32(p.length - 4, l * 8); dv.setUint32(p.length - 8, Math.floor(l / 0x20000000));
    let [a, b, c, d, e] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476, 0xc3d2e1f0];
    const rol = (x, k) => (x << k) | (x >>> (32 - k));
    for (let i = 0; i < p.length; i += 64) {
      for (let t = 0; t < 80; t++) w[t] = t < 16 ? dv.getUint32(i + t * 4) : rol(w[t - 3] ^ w[t - 8] ^ w[t - 14] ^ w[t - 16], 1);
      let [A, B, C, D, E] = [a, b, c, d, e];
      for (let t = 0; t < 80; t++) {
        const f = t < 20 ? (B & C) | (~B & D) : t < 40 ? B ^ C ^ D : t < 60 ? (B & C) | (B & D) | (C & D) : B ^ C ^ D;
        const k = t < 20 ? 0x5a827999 : t < 40 ? 0x6ed9eba1 : t < 60 ? 0x8f1bbcdc : 0xca62c1d6;
        const T = (rol(A, 5) + f + E + k + w[t]) | 0; E = D; D = C; C = rol(B, 30); B = A; A = T;
      }
      a = (a + A) | 0; b = (b + B) | 0; c = (c + C) | 0; d = (d + D) | 0; e = (e + E) | 0;
    }
    const r = new Buffer(20), rv = new DataView(r.buffer); [a, b, c, d, e].forEach((x, i) => rv.setUint32(i * 4, x)); return r;
  }
  const crypto = {
    randomUUID: () => self.crypto.randomUUID(),
    randomFillSync: (a) => { const u = new Uint8Array(a.buffer, a.byteOffset, a.byteLength); for (let i = 0; i < u.length; i += 65536) self.crypto.getRandomValues(u.subarray(i, i + 65536)); return a; },
    // ponytail: SHA-1 only (the image cache's file names); md5/sha256 throw. Add digests here if an app needs expo-crypto sync hashes.
    createHash(alg) {
      if (alg !== 'sha1') throw new Error(`OneSim (browser): crypto hash ${alg} is not available`);
      const parts = [];
      const h = { update: (x) => { parts.push(typeof x === 'string' ? Buffer.from(x) : x); return h; }, digest: (enc) => { const r = sha1(Buffer.concat(parts)); return enc ? r.toString(enc) : r; } };
      return h;
    },
  };

  // ---- path (posix subset) ----
  const normalize = (p) => {
    const abs = p.startsWith('/'), o = [];
    for (const s of p.split('/')) { if (s === '..') { if (o.length && o[o.length - 1] !== '..') o.pop(); else if (!abs) o.push(s); } else if (s && s !== '.') o.push(s); }
    return (abs ? '/' : '') + o.join('/') || (abs ? '/' : '.');
  };
  const path = {
    sep: '/', normalize,
    join: (...a) => normalize(a.filter(Boolean).join('/')),
    resolve: (...a) => { let r = '/'; for (const s of a) if (s) r = s.startsWith('/') ? s : r + '/' + s; return normalize(r); },
    dirname: (p) => { const i = p.replace(/\/+$/, '').lastIndexOf('/'); return i < 0 ? '.' : i === 0 ? '/' : p.slice(0, i); },
    basename: (p, ext) => { const b = p.replace(/\/+$/, '').split('/').pop(); return ext && b.endsWith(ext) ? b.slice(0, -ext.length) : b; },
    extname: (p) => { const b = path.basename(p), i = b.lastIndexOf('.'); return i > 0 ? b.slice(i) : ''; },
  };
  path.posix = path;

  // ---- fs: in-memory, mirrored to the page (see header) ----
  const files = new Map(); // abs path -> {dir: true} | {data: Uint8Array}, plus mtimeMs / birthtimeMs
  const fds = new Map();
  let nextFd = 10;
  const ERRS = { ENOENT: 'no such file or directory', EEXIST: 'file already exists', EISDIR: 'illegal operation on a directory', ENOTDIR: 'not a directory', ENOTEMPTY: 'directory not empty' };
  const fsErr = (code, p) => Object.assign(new Error(`${code}: ${ERRS[code]}, '${p}'`), { code, path: p });
  const abs = (p) => path.resolve(String(p instanceof NativeURL ? url.fileURLToPath(p) : p));
  const put = (p, e) => { const old = files.get(p); e.birthtimeMs = old ? old.birthtimeMs : Date.now(); e.mtimeMs = Date.now(); files.set(p, e); post({ type: '__fs', path: p, entry: e }); };
  const del = (p) => { files.delete(p); post({ type: '__fs', path: p, entry: null }); };
  const get = (p) => { const e = files.get(p); if (!e) throw fsErr('ENOENT', p); return e; };
  const file = (p) => { const e = get(p); if (e.dir) throw fsErr('EISDIR', p); return e; };
  const parentOk = (p) => { const d = files.get(path.dirname(p)); if (!d) throw fsErr('ENOENT', p); if (!d.dir) throw fsErr('ENOTDIR', p); };
  const under = (p) => [...files.keys()].filter((k) => k.startsWith(p === '/' ? '/' : p + '/'));
  const toBytes = (d, enc) => (typeof d === 'string' ? Buffer.from(d, enc) : new Uint8Array(d.buffer ? d.buffer.slice(d.byteOffset, d.byteOffset + d.byteLength) : d));
  const stat = (e) => ({ size: e.dir ? 0 : e.data.length, mtimeMs: e.mtimeMs, birthtimeMs: e.birthtimeMs, isFile: () => !e.dir, isDirectory: () => !!e.dir });
  const fs = {
    existsSync: (p) => files.has(abs(p)),
    statSync: (p) => stat(get(abs(p))),
    lstatSync: (p) => stat(get(abs(p))),
    readFileSync(p, o) { const b = Buffer.from(file(abs(p)).data); const enc = typeof o === 'string' ? o : o && o.encoding; return enc ? b.toString(enc) : b; },
    writeFileSync(p, d, o) { p = abs(p); parentOk(p); if (files.get(p)?.dir) throw fsErr('EISDIR', p); put(p, { data: toBytes(d, typeof o === 'string' ? o : o && o.encoding) }); },
    appendFileSync(p, d, o) { p = abs(p); const old = files.get(p); if (!old) return fs.writeFileSync(p, d, o); file(p); put(p, { data: Buffer.concat([old.data, toBytes(d, typeof o === 'string' ? o : o && o.encoding)]) }); },
    mkdirSync(p, o) {
      p = abs(p);
      if (files.has(p)) { if (o && o.recursive && files.get(p).dir) return; throw fsErr('EEXIST', p); }
      if (o && o.recursive) { if (p !== '/') fs.mkdirSync(path.dirname(p), o); } else parentOk(p);
      put(p, { dir: true });
    },
    rmSync(p, o = {}) {
      p = abs(p);
      const e = files.get(p);
      if (!e) { if (o.force) return; throw fsErr('ENOENT', p); }
      const kids = under(p);
      if (e.dir && kids.length && !o.recursive) throw fsErr('ENOTEMPTY', p);
      for (const k of kids) del(k);
      del(p);
    },
    renameSync(a, b) {
      a = abs(a); b = abs(b); get(a); parentOk(b);
      for (const k of [a, ...under(a)]) { const x = files.get(k); del(k); files.set(b + k.slice(a.length), x); post({ type: '__fs', path: b + k.slice(a.length), entry: x }); }
    },
    cpSync(a, b) { a = abs(a); b = abs(b); get(a); for (const k of [a, ...under(a)]) put(b + k.slice(a.length), files.get(k).dir ? { dir: true } : { data: files.get(k).data.slice() }); },
    readdirSync(p, o = {}) {
      p = abs(p); if (!get(p).dir) throw fsErr('ENOTDIR', p);
      const rel = under(p).map((k) => k.slice(p === '/' ? 1 : p.length + 1)).filter((r) => o.recursive || !r.includes('/')).sort();
      return o.withFileTypes ? rel.map((name) => { const e = files.get(path.join(p, name)); return { name, isDirectory: () => !!e.dir, isFile: () => !e.dir }; }) : rel;
    },
    openSync(p, flags = 'r') {
      p = abs(p);
      if (!files.has(p)) { if (/^[wa]/.test(flags)) fs.writeFileSync(p, ''); else throw fsErr('ENOENT', p); } else if (flags[0] === 'w') fs.writeFileSync(p, '');
      file(p); fds.set(nextFd, p); return nextFd++;
    },
    closeSync: (fd) => { fds.delete(fd); },
    fstatSync: (fd) => fs.statSync(fds.get(fd)),
    ftruncateSync: (fd, n = 0) => { const p = fds.get(fd), d = file(p).data, r = new Uint8Array(n); r.set(d.subarray(0, n)); put(p, { data: r }); },
    readSync(fd, b, off, len, pos) { const d = file(fds.get(fd)).data.subarray(pos, pos + len); b.set(d, off); return d.length; },
    writeSync(fd, b, off = 0, len, pos) {
      if (fd === 1 || fd === 2) { const s = typeof b === 'string' ? b : Buffer.from(b).toString(); (fd === 2 ? err : out)(s.replace(/\n$/, '')); return s.length; }
      const p = fds.get(fd), d = file(p).data, src = typeof b === 'string' ? Buffer.from(b) : b.subarray(off, off + (len ?? b.length - off));
      const at = pos ?? d.length, r = new Uint8Array(Math.max(d.length, at + src.length)); r.set(d); r.set(src, at); put(p, { data: r }); return src.length;
    },
    statfsSync: () => { throw new Error('statfs is not available in the browser'); },
  };

  // ---- url, util, os, vm ----
  const url = {
    URL: NativeURL,
    fileURLToPath: (u) => { u = new NativeURL(String(u)); if (u.protocol !== 'file:') throw new TypeError('The URL must be of scheme file'); return decodeURIComponent(u.pathname); },
    pathToFileURL: (p) => { const u = new NativeURL('file:///'); u.pathname = path.resolve(p); return u; },
  };
  const inspect = (v) => { if (v instanceof Error) return v.stack || String(v); try { return JSON.stringify(v) ?? String(v); } catch (_) { return String(v); } };
  const os = { tmpdir: () => '/tmp', homedir: () => '/home', platform: () => 'browser', EOL: '\n' };
  const geval = eval; // indirect: global scope, like vm.runInThisContext
  const src = (code, o) => `${code}\n//# sourceURL=${String((o && o.filename) || o || 'eval').replace(/\s/g, '%20')}`;
  // ponytail: a context is a with()-scope over the sandbox (falls back to this global's builtins), not a separate realm:
  // top-level declarations stay local to one run. Enough for react-native-worklets' UI runtime basics; a real realm needs an iframe (not in workers).
  const vm = {
    runInThisContext: (code, o) => geval(src(code, o)),
    createContext(sb = {}) {
      const scope = new Proxy(sb, { has: (_, k) => k !== Symbol.unscopables && k !== '__s', get: (t, k) => (k === 'globalThis' || k === 'global' || k === 'self' ? scope : k in t ? t[k] : self[k]), set: (t, k, v) => { t[k] = v; return true; } });
      Object.defineProperty(sb, '__onesimScope', { value: scope });
      return sb;
    },
    runInContext: (code, sb, o) => new Function('__s', `with (__s) { return eval(__s.__code); }`)(Object.assign(sb.__onesimScope, { __code: src(code, o) })),
  };

  // ---- worker_threads / process ----
  const listeners = [], handlers = { uncaughtException: [], unhandledRejection: [] };
  const parentPort = { postMessage: (m) => post(m), on: (ev, f) => { if (ev === 'message') listeners.push(f); }, once: () => {} };
  const process = {
    env: {}, argv: ['node', 'host.js'], platform: 'browser', versions: {}, cwd: () => '/',
    on(ev, f) { (handlers[ev] || (handlers[ev] = [])).push(f); return process; },
    nextTick: (f, ...a) => queueMicrotask(() => f(...a)),
    exit: () => self.close(),
    stdout: { write: (s) => { out(String(s).replace(/\n$/, '')); return true; } },
    stderr: { write: (s) => { err(String(s).replace(/\n$/, '')); return true; } },
  };
  self.addEventListener('error', (e) => { if (handlers.uncaughtException.length) { e.preventDefault(); handlers.uncaughtException.forEach((f) => f(e.error || e.message)); } });
  self.addEventListener('unhandledrejection', (e) => { if (handlers.unhandledRejection.length) { e.preventDefault(); handlers.unhandledRejection.forEach((f) => f(e.reason)); } });

  // ---- CommonJS require over the guest files next to this script (sync XHR: workers allow it) ----
  const builtins = { fs, path, os, crypto, url, vm, util: { inspect }, buffer: { Buffer }, process, readline: {}, worker_threads: { isMainThread: false, parentPort, workerData: null, Worker: null } };
  const cache = {};
  function require(id) {
    const b = builtins[id.replace(/^node:/, '')];
    if (b) return b;
    const u = new NativeURL(id, self.location.href).href;
    if (cache[u]) return cache[u].exports;
    const x = new XMLHttpRequest();
    x.open('GET', u, false); x.send();
    if (x.status !== 200) throw new Error(`OneSim: cannot load ${u} (HTTP ${x.status})`);
    const module = (cache[u] = { exports: {} });
    new Function('exports', 'require', 'module', '__filename', '__dirname', src(x.responseText.replace(/^#!.*/, ''), u))(module.exports, require, module, u, '/');
    return module.exports;
  }

  // Globals host.js reads by their bare names (it captures them before the bundle runs).
  // It calls them as methods of its `node` object: Web APIs throw "Illegal invocation" unless bound to the global.
  for (const k of ['fetch', 'setTimeout', 'clearTimeout', 'setInterval', 'clearInterval', 'queueMicrotask']) self[k] = self[k].bind(self);
  const imm = [], ch = new MessageChannel();
  ch.port1.onmessage = () => imm.shift()();
  Object.assign(self, { Buffer, process, setImmediate: (f, ...a) => { imm.push(() => f(...a)); ch.port2.postMessage(0); } });
  console.Console = function Console() { return console; }; // host.js: console = new console.Console(process.stderr)

  self.addEventListener('message', function init(e) {
    const m = e.data;
    if (!m || m.type !== '__init') return;
    self.removeEventListener('message', init);
    const base = m.pageOrigin || self.location.href;
    for (const [k, v] of Object.entries(m.env || {})) process.env[k] = /^SIMCORE_(METRO|BUNDLE)_URL$/.test(k) && v ? new NativeURL(v, base).href : String(v);
    builtins.worker_threads.workerData = m.workerData || { generation: 0, dataDir: '/session' };
    if (m.files) for (const [p, x] of m.files) files.set(p, x);
    fs.mkdirSync(builtins.worker_threads.workerData.dataDir || '/session', { recursive: true });
    fs.mkdirSync('/tmp', { recursive: true });
    self.addEventListener('message', (ev) => listeners.forEach((f) => f(ev.data)));
    require('./host.js');
  });
})();
