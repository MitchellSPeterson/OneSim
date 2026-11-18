#!/usr/bin/env node
// SimCore React Native guest. Runs an unmodified Metro bundle in a bare vm context (like Hermes: ES
// builtins only), implements the *native* side RN's JS expects (bridgeless/new-arch: Fabric UIManager,
// TurboModules, timers, callable modules) and streams the host view tree as JSON lines on stdout.
// stdout is protocol-only. Wire contract: JSON lines, see run-standalone.mjs for a client. Root surface tag = 1 (iOS uses 1, 11, ...).
'use strict';
// Why a worker thread: Node's vm contexts break RN/expo lazy-global polyfills (defineProperty inside a
// global getter leaves the property undefined), so the bundle runs in a plain Worker global instead.
// Main thread = stdin/stdout protocol + supervisor (reload = new Worker); worker = the RN "native" side.
const vm = require('node:vm');
const fs = require('node:fs');
const readline = require('node:readline');
const os = require('node:os');
const path = require('node:path');
const crypto = require('node:crypto');
const { Worker, isMainThread, parentPort, workerData } = require('node:worker_threads');

Error.stackTraceLimit = 60;
const ROOT_TAG = 1;
const TEXT_INPUTS = new Set(['RCTSinglelineTextInputView', 'RCTMultilineTextInputView']);
const env = process.env; // `process` is removed from the guest global before the bundle runs
if (isMainThread) return mainThread();

// Node intrinsics captured before RN's polyfills replace the globals of the same name.
const node = {
  Buffer, setTimeout, clearTimeout, setInterval, clearInterval, setImmediate, queueMicrotask, performance, fetch,
  AbortController, TextDecoder, Uint8Array,
  inspect: require('node:util').inspect, randomUUID: require('node:crypto').randomUUID,
};
// HTTP runs on the main thread (see __fetch): once RN's polyfills replace timers/Promise/Buffer in this global,
// Node's own fetch (undici) breaks here. The worker only keeps the plain `node.fetch` for the bundle download.
const fetches = new Map(); // id -> in-flight request state
let fetchSeq = 0;
// nfetch(url, {method, headers, body, redirect, onChunk}) -> Promise<{status,statusText,url,redirected,headers:[[k,v]],get(name),done,abort()}>;
// `done` resolves with all body bytes (Buffer; empty when onChunk consumed them) or rejects on a network error.
function nfetch(url, init = {}) {
  const id = ++fetchSeq;
  const p = new Promise((resolve, reject) => {
    const st = { chunks: [], onChunk: init.onChunk, head: { resolve, reject } };
    st.done = new Promise((res, rej) => { st.finish = res; st.fail = rej; });
    st.done.catch(() => {}); // callers that only want the head must not trip unhandledRejection
    fetches.set(id, st);
    send({ type: '__fetch', id, url, method: init.method, headers: init.headers, redirect: init.redirect, body: init.body });
  });
  p.abort = () => send({ type: '__fetchAbort', id }); // works before the head arrives too (then `p` rejects)
  return p;
}
function fetchEvent(m) {
  const st = fetches.get(m.id);
  if (!st) return;
  if (m.type === '__fetchHead') {
    const res = { status: m.status, statusText: m.statusText, url: m.url, redirected: m.redirected, headers: m.headers, done: st.done, abort: () => send({ type: '__fetchAbort', id: m.id }),
      get: (k) => { const h = m.headers.find(([n]) => n === String(k).toLowerCase()); return h ? h[1] : null; } };
    st.head.resolve(res);
  } else if (m.type === '__fetchChunk') {
    const b = node.Buffer.from(m.data);
    if (st.onChunk) st.onChunk(b); else st.chunks.push(b);
  } else if (m.type === '__fetchEnd') { fetches.delete(m.id); st.finish(node.Buffer.concat(st.chunks)); }
  else if (m.type === '__fetchError') { fetches.delete(m.id); st.head.reject(new Error(m.message)); st.fail(new Error(m.message)); }
}
const send = (m) => parentPort.postMessage(m);
const log = (...a) => fs.writeSync(2, a.join(' ') + '\n');
const arg = (n) => { const i = process.argv.indexOf(n); return i > 0 ? process.argv[i + 1] : undefined; };

const cfg = {
  metro: (arg('--url') || process.env.SIMCORE_METRO_URL || 'http://localhost:8081').replace(/\/+$/, ''),
  appearance: process.env.SIMCORE_APPEARANCE === 'dark' ? 'dark' : 'light',
  scale: Number(process.env.SIMCORE_SCALE) || 3,
  session: process.env.SIMCORE_SESSION_ID || '',
  hmr: process.env.SIMCORE_HMR !== '0', // Fast Refresh via Metro's /hot; SIMCORE_HMR=0 turns it off
  generation: (workerData && workerData.generation) || 0, // 0 = first load, >0 = after a reload
};
{
  const m = /^(\d+(?:\.\d+)?)x(\d+(?:\.\d+)?)$/.exec(process.env.SIMCORE_VIEWPORT || '393x852');
  cfg.width = m ? +m[1] : 393; cfg.height = m ? +m[2] : 852;
}

const sendEvent = (name, detail) => send({ type: 'event', name, detail: detail || {} });
const sendError = (message) => { log('SimCore error:', message); send({ type: 'error', message: String(message) }); };
const reportedModules = new Set();
function unsupported(mod, method) {
  const msg = `SimCore: ${mod} is not supported (method: ${method})`;
  log(msg);
  if (env.SIMCORE_DEBUG) log(new Error().stack);
  if (!reportedModules.has(mod)) { reportedModules.add(mod); send({ type: 'error', message: msg }); }
}

// ---------------------------------------------------------------------------------------------
// Image cache: $TMPDIR/simcore-images/<sha1(uri)>.<ext>. Shared by RCTImageView and ImageLoader.
// ---------------------------------------------------------------------------------------------
const imageCache = new Map(); // uri -> Promise<{path,width,height}> (pixels)
const imageDone = new Map(); // uri -> info, once loaded (queryCache)
// Pixel size + extension from the file header (PNG, JPEG, GIF, WebP); null if unrecognised.
function imageInfo(b) {
  if (b.length > 24 && b.readUInt32BE(0) === 0x89504e47) return { ext: 'png', width: b.readUInt32BE(16), height: b.readUInt32BE(20) };
  if (b.length > 10 && b.toString('latin1', 0, 4) === 'GIF8') return { ext: 'gif', width: b.readUInt16LE(6), height: b.readUInt16LE(8) };
  if (b.length > 30 && b.toString('latin1', 0, 4) === 'RIFF' && b.toString('latin1', 8, 12) === 'WEBP') {
    const k = b.toString('latin1', 12, 16);
    if (k === 'VP8 ') return { ext: 'webp', width: b.readUInt16LE(26) & 0x3fff, height: b.readUInt16LE(28) & 0x3fff };
    if (k === 'VP8L') { const v = b.readUInt32LE(21); return { ext: 'webp', width: (v & 0x3fff) + 1, height: ((v >> 14) & 0x3fff) + 1 }; }
    if (k === 'VP8X') return { ext: 'webp', width: 1 + b.readUIntLE(24, 3), height: 1 + b.readUIntLE(27, 3) };
  }
  if (b.length > 4 && b[0] === 0xff && b[1] === 0xd8) {
    for (let i = 2; i + 9 < b.length;) {
      if (b[i] !== 0xff) { i++; continue; }
      const m = b[i + 1];
      if (m >= 0xc0 && m <= 0xcf && m !== 0xc4 && m !== 0xc8 && m !== 0xcc) return { ext: 'jpg', height: b.readUInt16BE(i + 5), width: b.readUInt16BE(i + 7) };
      i += m === 0xd8 || m === 0x01 || (m >= 0xd0 && m <= 0xd7) || m === 0xff ? (m === 0xff ? 1 : 2) : 2 + b.readUInt16BE(i + 2);
    }
  }
  return null;
}
async function readUri(uri, headers) {
  const d = /^data:[^,]*?(;base64)?,([\s\S]*)$/.exec(uri);
  if (d) return d[1] ? node.Buffer.from(d[2], 'base64') : node.Buffer.from(decodeURIComponent(d[2]), 'latin1');
  if (/^file:/.test(uri)) return fs.readFileSync(require('node:url').fileURLToPath(uri));
  if (uri[0] === '/') return fs.readFileSync(uri);
  const r = await nfetch(uri, { headers });
  if (r.status < 200 || r.status >= 300) throw new Error(`HTTP ${r.status} for ${uri}`);
  return r.done;
}
function loadImage(uri, headers) {
  let p = imageCache.get(uri);
  if (!p) {
    p = readUri(uri, headers).then((buf) => {
      const i = imageInfo(buf);
      if (!i) throw new Error(`not a PNG/JPEG/GIF/WebP image: ${uri.slice(0, 120)}`);
      const dir = path.join(os.tmpdir(), 'simcore-images');
      fs.mkdirSync(dir, { recursive: true });
      const file = path.join(dir, `${crypto.createHash('sha1').update(uri).digest('hex')}.${i.ext}`);
      fs.writeFileSync(file, buf);
      const info = { path: file, width: i.width, height: i.height };
      imageDone.set(uri, info);
      return info;
    });
    imageCache.set(uri, p);
    p.catch(() => imageCache.delete(uri)); // failures are not cached
  }
  return p;
}

// ---------------------------------------------------------------------------------------------
// One Runtime per worker; reload = terminate worker + new worker (fresh global state).
// ---------------------------------------------------------------------------------------------
let version = 0;
let current = null;

class Runtime {
  constructor() {
    this.dead = false;
    this.callable = new Map(); // name -> factory
    this.callableCache = new Map();
    this.nodes = new Map(); // tag -> latest node (for events / measure)
    this.parents = new Map(); // tag -> parent tag (committed tree)
    this.frames = {}; // tag -> [x,y,w,h] page coords from host
    this.lastLaidOut = {}; // tag -> "x,y,w,h" last frame reported through topLayout
    this.rootChildren = [];
    this.rootNode = { reactTag: ROOT_TAG, viewName: 'RootView', props: {}, children: [], instanceHandle: null };
    this.pending = false;
    this.inputs = new Map(); // tag -> {text, eventCount} (TextInput state; the guest owns the text)
    this.focused = null; // tag of the focused TextInput
    this.images = new Map(); // tag -> {uri, info?, err?}
    this.scrolled = new Map(); // tag -> last reported [x,y]
    this.shown = new Set(); // Modal tags that got topShow
    this.blobs = new Map(); // BlobModule store: blobId -> Buffer
    this.eventHandler = null;
    this.eventPriority = 32;
    this.window = { width: cfg.width, height: cfg.height };
    this.appearance = cfg.appearance;
  }

  guard(fn) {
    return (...args) => {
      if (this.dead) return;
      try { return fn(...args); } catch (e) { this.onUncaught(e); }
    };
  }
  onUncaught(e) {
    const g = globalThis.ErrorUtils;
    try { if (g && g.reportFatalError) return g.reportFatalError(e); } catch (_) { /* fall through */ }
    sendError(e && e.stack ? e.stack : e);
  }

  // ---- timers (native in bridgeless mode; numeric ids like Hermes hosts) ----
  makeTimers() {
    const live = new Map();
    let next = 0;
    const mk = (set, clear, repeat) => (fn, ms, ...a) => {
      const id = ++next;
      live.set(id, set(this.guard((...x) => { if (!repeat) live.delete(id); return fn(...x); }), ms, ...a));
      return id;
    };
    const clr = (clear) => (id) => { clear(live.get(id)); live.delete(id); };
    return {
      setTimeout: mk(node.setTimeout, 0, false), setInterval: mk(node.setInterval, 0, true),
      clearTimeout: clr(node.clearTimeout), clearInterval: clr(node.clearInterval),
      requestAnimationFrame: (cb) => {
        const id = ++next;
        live.set(id, node.setTimeout(this.guard(() => { live.delete(id); cb(node.performance.now()); }), 16));
        return id;
      },
      cancelAnimationFrame: clr(node.clearTimeout),
      queueMicrotask: (fn) => node.queueMicrotask(this.guard(fn)),
    };
  }

  // ---- Fabric UIManager ----
  makeFabric() {
    const self = this;
    const mk = (n, props, children) => ({ reactTag: n.reactTag, viewName: n.viewName, rootTag: n.rootTag, props, children, instanceHandle: n.instanceHandle });
    const merge = (base, diff) => {
      const p = Object.assign({}, base);
      for (const k of Object.keys(diff || {})) { if (diff[k] === null) delete p[k]; else p[k] = diff[k]; }
      return p;
    };
    const isInput = (n) => TEXT_INPUTS.has(n.viewName);
    const reg = (n) => { self.nodes.set(n.reactTag, n); return n; };
    const diag = (m, ret) => (...a) => { unsupported('FabricUIManager', m); return ret; };
    const frameOf = (n) => self.frames[n.reactTag];
    const rel = (tag) => { // x,y relative to parent
      const f = self.frames[tag]; if (!f) return [0, 0, 0, 0];
      const pf = self.frames[self.parents.get(tag)];
      return [f[0] - (pf ? pf[0] : 0), f[1] - (pf ? pf[1] : 0), f[2], f[3]];
    };
    return {
      createNode: (reactTag, viewName, rootTag, props, instanceHandle) => {
        if (env.SIMCORE_DEBUG) log('createNode', reactTag, viewName);
        return reg({ reactTag, viewName, rootTag, props: merge({}, props), children: [], instanceHandle });
      },
      cloneNode: (n) => reg(mk(n, n.props, n.children.slice())),
      cloneNodeWithNewChildren: (n) => reg(mk(n, n.props, [])),
      cloneNodeWithNewProps: (n, p) => reg(mk(n, merge(n.props, p), n.children.slice())),
      cloneNodeWithNewChildrenAndProps: (n, p) => reg(mk(n, merge(n.props, p), [])),
      createChildSet: () => [],
      appendChild: (parent, child) => { parent.children.push(child); return parent; },
      appendChildToSet: (set, child) => { set.push(child); },
      completeRoot: (rootTag, childSet) => { env.SIMCORE_DEBUG && log('completeRoot', rootTag, childSet.length); self.rootChildren = childSet; self.scheduleEmit(); },
      registerEventHandler: (h) => { self.eventHandler = h; },
      // Frames come from the Rust host's layout engine; zeros until the first `layout` message.
      measure: (n, cb) => {
        const [x, y, w, h] = rel(n.reactTag), f = frameOf(n) || [0, 0, 0, 0];
        node.queueMicrotask(self.guard(() => cb(x, y, w, h, f[0], f[1])));
      },
      measureInWindow: (n, cb) => {
        const f = frameOf(n) || [0, 0, 0, 0];
        node.queueMicrotask(self.guard(() => cb(f[0], f[1], f[2], f[3])));
      },
      measureLayout: (n, relNode, onFail, onSuccess) => {
        const f = frameOf(n), r = frameOf(relNode);
        if (!f || !r) return node.queueMicrotask(self.guard(() => (onFail && onFail())));
        node.queueMicrotask(self.guard(() => onSuccess(f[0] - r[0], f[1] - r[1], f[2], f[3])));
      },
      configureNextLayoutAnimation: diag('configureNextLayoutAnimation'),
      sendAccessibilityEvent: diag('sendAccessibilityEvent'),
      findShadowNodeByTag_DEPRECATED: (tag) => self.nodes.get(tag) || null,
      setNativeProps: (n, p) => {
        n.props = merge(n.props, p);
        if (isInput(n) && typeof p.text === 'string') self.input(n).text = p.text;
        self.scheduleEmit();
      },
      dispatchCommand: (n, name, args) => self.command(n, name, args || []),
      findNodeAtPoint: (n, x, y, cb) => { unsupported('FabricUIManager', 'findNodeAtPoint'); cb(null); },
      compareDocumentPosition: diag('compareDocumentPosition', 0),
      getBoundingClientRect: diag('getBoundingClientRect', [0, 0, 0, 0]),
      setIsJSResponder: () => {}, // scroll-gesture arbitration is the host's concern
      unstable_DefaultEventPriority: 32,
      unstable_DiscreteEventPriority: 2,
      unstable_ContinuousEventPriority: 8,
      unstable_IdleEventPriority: 268435456,
      unstable_getCurrentEventPriority: () => self.eventPriority,
    };
  }

  // DOM-like node APIs (ReadOnlyNode / ReactNativeElement: parentNode, measure, getBoundingClientRect, ...).
  // Frames come from the host's `layout` message (zeros before it arrives).
  domModule() {
    const f = this.fabric, nodes = this.nodes, frames = this.frames;
    const latest = (n) => (typeof n === 'number' ? this.rootNode : nodes.get(n.reactTag) || n);
    const frame = (n) => frames[n.reactTag] || [0, 0, 0, 0];
    const text = (n) => (n.viewName === 'RCTRawText' ? n.props.text || '' : n.children.map(text).join(''));
    const dom = {
      linkRootNode: () => this.rootNode,
      getParentNode: (n) => { const p = nodes.get(this.parents.get(latest(n).reactTag)); return p ? p.instanceHandle : null; },
      getChildNodes: (n) => latest(n).children.map((c) => c.instanceHandle),
      isConnected: (n) => this.parents.has(latest(n).reactTag),
      getBoundingClientRect: (n) => frame(latest(n)),
      getInnerSize: (n) => frame(latest(n)).slice(2),
      getBorderWidth: (n) => { const p = latest(n).props; return [p.borderTopWidth || 0, p.borderRightWidth || 0, p.borderBottomWidth || 0, p.borderLeftWidth || 0]; },
      getScrollPosition: () => [0, 0], // ScrollView offsets are host state; see known gaps
      getScrollSize: (n) => frame(latest(n)).slice(2),
      getTagName: (n) => latest(n).viewName,
      getTextContent: (n) => text(latest(n)),
      getOffset: () => [null, 0, 0],
      hasPointerCapture: () => false,
      measure: (n, cb) => f.measure(latest(n), cb),
      measureInWindow: (n, cb) => f.measureInWindow(latest(n), cb),
      measureLayout: (n, r, fail, ok) => f.measureLayout(latest(n), latest(r), fail, ok),
      setNativeProps: (n, p) => f.setNativeProps(latest(n), p),
      compareDocumentPosition: (a, b) => f.compareDocumentPosition(a, b),
    };
    return new Proxy(dom, { get: (t, p) => t[p] || (typeof p === 'string' ? () => { unsupported('NativeDOMCxx', p); } : undefined) });
  }

  // ---- tree emission ----
  scheduleEmit() {
    if (this.pending) return;
    this.pending = true;
    node.setImmediate(() => {
      this.pending = false;
      if (this.dead) return;
      this.parents.clear();
      const after = []; // events to dispatch once the tree is out (first emit of an image / visible modal)
      const ser = (n, parent) => {
        this.parents.set(n.reactTag, parent);
        const props = {};
        for (const [k, v] of Object.entries(n.props)) {
          if (typeof v === 'function' || v === undefined) continue;
          props[k] = v;
        }
        if (TEXT_INPUTS.has(n.viewName)) props.__simcoreText = this.input(n).text;
        else if (n.viewName === 'RCTImageView') this.imageProps(n, props, after);
        else if (n.viewName === 'ModalHostView') {
          if (props.visible && !this.shown.has(n.reactTag)) { this.shown.add(n.reactTag); after.push(() => this.dispatch(n, 'topShow', { target: n.reactTag })); }
          else if (!props.visible) this.shown.delete(n.reactTag);
        }
        // RN's Fabric name is ModalHostView; the wire contract says RCTModalHostView
        return { kind: 'rn', id: String(n.reactTag), component: n.viewName === 'ModalHostView' ? 'RCTModalHostView' : n.viewName, props, children: n.children.map((c) => ser(c, n.reactTag)) };
      };
      const root = { kind: 'rn', id: String(ROOT_TAG), component: 'RootView', props: {}, children: this.rootChildren.map((c) => ser(c, ROOT_TAG)) };
      if (env.SIMCORE_DEBUG) log('emit tree', version + 1);
      send({ type: 'tree', version: ++version, root });
      after.forEach((f) => f());
      this.fireLayouts();
    });
  }

  fireLayouts() {
    for (const [tag, f] of Object.entries(this.frames)) {
      const n = this.nodes.get(+tag);
      if (!n || !n.props.onLayout || !this.parents.has(+tag)) continue;
      const key = f.join(',');
      if (this.lastLaidOut[tag] === key) continue;
      this.lastLaidOut[tag] = key;
      const pf = this.frames[this.parents.get(+tag)];
      this.dispatch(n, 'topLayout', { layout: { x: f[0] - (pf ? pf[0] : 0), y: f[1] - (pf ? pf[1] : 0), width: f[2], height: f[3] }, target: +tag });
    }
  }

  dispatch(node, type, nativeEvent, priority = 32) {
    if (!this.eventHandler || this.dead) return;
    this.eventPriority = priority;
    try { this.eventHandler(node.instanceHandle, type, nativeEvent); } catch (e) { this.onUncaught(e); }
    this.eventPriority = 32;
  }

  // ---- host -> guest ----
  onMessage(m) {
    switch (m.type) {
      case '__wsEvent':
        if (m.event === 'websocketMessage' && m.payload.type === 'text' && m.payload.data.includes('"error"')) {
          try { const d = JSON.parse(m.payload.data); if (d.type === 'error') this.hmrError(d.body || {}); } catch (_) { /* not JSON */ }
        }
        this.emit(m.event, m.payload); break;
      case '__fetchHead': case '__fetchChunk': case '__fetchEnd': case '__fetchError': this.guard(fetchEvent)(m); break;
      case 'touch': {
        const n = this.nodes.get(m.tag);
        if (!n) return log(`SimCore: touch for unknown tag ${m.tag}`);
        const ts = m.timestamp ?? Date.now();
        const touch = { identifier: 0, target: m.tag, pageX: m.x, pageY: m.y, locationX: m.lx ?? 0, locationY: m.ly ?? 0, timestamp: ts, force: 0 };
        const ended = m.phase === 'end' || m.phase === 'cancel';
        const type = { start: 'topTouchStart', move: 'topTouchMove', end: 'topTouchEnd', cancel: 'topTouchCancel' }[m.phase];
        if (!type) return;
        this.dispatch(n, type, {
          touches: ended ? [] : [touch], changedTouches: [touch], target: m.tag, identifier: 0,
          pageX: m.x, pageY: m.y, locationX: touch.locationX, locationY: touch.locationY, timestamp: ts,
        }, m.phase === 'move' ? 8 : 2);
        if (m.phase === 'end') {
          const t = this.nodes.get(m.tag); // latest committed props
          if (t && t.viewName === 'RCTSwitch' && !t.props.disabled) this.dispatch(t, 'topChange', { value: !t.props.value, target: m.tag }, 2);
        }
        break;
      }
      case 'focus': this.focus(m.tag); break;
      case 'blur': this.blur(m.tag); break;
      case 'text_input': { // `version` is ignored: the guest owns the text
        const n = this.nodes.get(m.field);
        if (!n || !TEXT_INPUTS.has(n.viewName)) return log(`SimCore: text_input for unknown field ${m.field}`);
        const st = this.input(n), text = String(m.text ?? '');
        st.text = text; st.eventCount++;
        this.dispatch(n, 'topChange', { text, eventCount: st.eventCount, target: m.field }, 2);
        this.dispatch(n, 'topSelectionChange', { selection: { start: text.length, end: text.length }, target: m.field }, 2);
        this.scheduleEmit();
        break;
      }
      case 'submit': {
        const n = this.nodes.get(m.tag);
        if (!n || !TEXT_INPUTS.has(n.viewName)) return;
        const st = this.input(n);
        this.dispatch(n, 'topSubmitEditing', { text: st.text, eventCount: st.eventCount, target: m.tag }, 2);
        if (n.viewName === 'RCTSinglelineTextInputView' && n.props.submitBehavior !== 'submit') { this.blur(m.tag); send({ type: 'focus_request', tag: null }); }
        break;
      }
      case 'scroll': {
        const n = this.nodes.get(m.tag);
        if (!n) return;
        const last = this.scrolled.get(m.tag), cur = [m.x, m.y];
        this.scrolled.set(m.tag, cur);
        const ev = { contentOffset: { x: m.x, y: m.y }, contentSize: { width: m.contentWidth, height: m.contentHeight }, layoutMeasurement: { width: m.width, height: m.height }, contentInset: { top: 0, left: 0, bottom: 0, right: 0 }, zoomScale: 1, target: m.tag };
        if (m.phase === 'begin') this.dispatch(n, 'topScrollBeginDrag', ev, 8);
        else if (m.phase === 'scroll') this.dispatch(n, 'topScroll', ev, 8);
        else if (m.phase === 'end') {
          if (!last || last[0] !== m.x || last[1] !== m.y) this.dispatch(n, 'topScroll', ev, 8); // programmatic scrollTo reply
          this.dispatch(n, 'topScrollEndDrag', ev, 8);
          this.dispatch(n, 'topMomentumScrollEnd', ev, 8);
        }
        break;
      }
      case 'layout': Object.assign(this.frames, m.frames); this.fireLayouts(); break;
      case 'environment': {
        if (m.appearance && m.appearance !== this.appearance) { this.appearance = m.appearance; this.emit('appearanceChanged', { colorScheme: this.appearance }); }
        if (m.viewport) { this.window = { width: m.viewport.width, height: m.viewport.height }; this.emit('didUpdateDimensions', this.dimensions()); }
        break;
      }
    }
  }
  input(n) {
    let st = this.inputs.get(n.reactTag);
    if (!st) this.inputs.set(n.reactTag, (st = { text: String(n.props.text ?? n.props.value ?? n.props.defaultValue ?? ''), eventCount: 0 }));
    return st;
  }
  focus(tag) {
    const n = this.nodes.get(tag);
    if (!n || this.focused === tag) return;
    if (this.focused != null) this.blur(this.focused);
    this.focused = tag;
    this.dispatch(n, 'topFocus', { target: tag }, 2);
  }
  blur(tag) {
    const n = this.nodes.get(tag);
    if (!n || this.focused !== tag) return;
    this.focused = null;
    this.dispatch(n, 'topEndEditing', { text: this.input(n).text, target: tag }, 2);
    this.dispatch(n, 'topBlur', { target: tag }, 2);
  }
  // Fabric view commands (names/args from the *NativeComponent / *Commands specs).
  command(n, name, args) {
    const tag = n.reactTag;
    if (TEXT_INPUTS.has(n.viewName)) {
      if (name === 'focus') return send({ type: 'focus_request', tag });
      if (name === 'blur') return send({ type: 'focus_request', tag: null });
      if (name === 'setTextAndSelection') { // (eventCount, text|null, start, end); stale counts lose, like on iOS
        const [count, text] = args, st = this.input(n);
        if (typeof text === 'string' && count >= st.eventCount) { st.text = text; this.scheduleEmit(); }
        return;
      }
    } else if (n.viewName === 'RCTScrollView') {
      if (name === 'scrollTo') return send({ type: 'scroll_to', tag, x: args[0] || 0, y: args[1] || 0 });
      if (name === 'scrollToEnd') return send({ type: 'scroll_to', tag, x: 'end', y: 'end' });
      if (name === 'flashScrollIndicators' || name === 'zoomToRect') return;
    }
    unsupported('FabricUIManager', `dispatchCommand:${name} on ${n.viewName}`);
  }

  // Image: download once per uri (shared with ImageLoader), expose the cached file to the host.
  imageProps(n, props, after) {
    const tag = n.reactTag, src = [].concat(props.source || [])[0], uri = src && src.uri;
    if (!uri) return;
    let st = this.images.get(tag);
    if (!st || st.uri !== uri) {
      const mine = (st = { uri });
      this.images.set(tag, st);
      const done = (type, ev) => { if (this.images.get(tag) !== mine) return; this.dispatch(this.nodes.get(tag) || n, type, { target: tag, ...ev }); };
      after.push(() => {
        done('topLoadStart', {});
        loadImage(uri, src.headers).then((info) => {
          mine.info = info;
          const s = src.scale || 1;
          done('topLoad', { source: { uri, width: info.width / s, height: info.height / s } });
          done('topLoadEnd', {});
          if (this.images.get(tag) === mine) this.scheduleEmit();
        }, (e) => { mine.err = e; done('topError', { error: String(e && e.message || e) }); done('topLoadEnd', {}); });
      });
    }
    if (st.info) props.__simcoreImage = st.info;
  }

  dimensions() {
    const w = { width: this.window.width, height: this.window.height, scale: cfg.scale, fontScale: 1 };
    return { window: w, screen: w };
  }
  emit(event, payload) {
    const em = this.getCallable('RCTDeviceEventEmitter');
    if (em) this.guard(() => em.emit(event, payload))();
  }
  getCallable(name) {
    if (!this.callableCache.has(name)) {
      const f = this.callable.get(name);
      this.callableCache.set(name, f ? f() : null);
    }
    return this.callableCache.get(name);
  }

  // Metro HMR `error` (syntax / resolution failure): RN's listener throws inside an event-target-shim dispatch where it is
  // swallowed, so report it here. The last good tree stays (no commit happens); fixing the file recovers via the next update.
  hmrError(b) {
    const where = (b.errors || []).map((x) => [x.filename, x.lineNumber, x.column].filter((v) => v != null).join(':') + (x.description ? ` ${x.description}` : '')).filter(Boolean).join('\n');
    sendEvent('bundle.error', { type: b.type, message: b.message, errors: b.errors });
    sendError(`${b.type || 'Error'}: ${b.message || ''}${where ? '\n' + where : ''}`);
    // Metro lost this client's revision (edits raced, Metro restarted): RN would say "Reload to reconnect"; do it.
    if (b.type === 'RevisionNotFoundError' || b.type === 'GraphNotFoundError') this.reload(b.type);
  }

  // Full reload (DevSettings.reload, expo reloadAppAsync, React Refresh bail-out): main thread swaps the worker.
  reload(reason) {
    if (this.dead) return;
    log('SimCore: reload:', reason || '');
    send({ type: '__reload', reason: String(reason || '') });
  }

  dispose() {
    this.dead = true;
  }

  // ---- TurboModules ----
  makeModules(bundleSource) {
    const self = this;
    const ver = /ReactNativeVersion\.major = (\d+);\s*ReactNativeVersion\.minor = (\d+);\s*ReactNativeVersion\.patch = (\d+)/.exec(bundleSource) || [0, 0, 0, 0];
    const noop = () => {};
    const listener = { addListener: noop, removeListeners: noop };
    const stub = (o) => Object.assign({ getConstants: () => ({}) }, o);
    const reportException = (d) => sendError(`${d.message}\n${(d.stack || []).map((f) => `  at ${f.methodName || '?'} (${f.file}:${f.lineNumber}:${f.column})`).join('\n')}`);
    const modules = {
      PlatformConstants: () => ({
        getConstants: () => ({
          isTesting: false, isDisableAnimations: false, reactNativeVersion: { major: +ver[1], minor: +ver[2], patch: +ver[3] },
          forceTouchAvailable: false, osVersion: '26.5', systemName: 'iOS', interfaceIdiom: 'phone',
        }),
      }),
      DeviceInfo: () => ({ getConstants: () => ({ Dimensions: self.dimensions() }) }),
      Appearance: () => ({ getColorScheme: () => self.appearance, ...listener }),
      SourceCode: () => ({ getConstants: () => ({ scriptURL: self.scriptURL }) }),
      I18nManager: () => stub({
        getConstants: () => ({ isRTL: false, doLeftAndRightSwapInRTL: true, localeIdentifier: 'en_US' }),
        allowRTL: noop, forceRTL: noop, swapLeftAndRightInRTL: noop,
      }),
      // Host chrome (status bar) is the Rust host's job; calls are accepted and logged, not rendered.
      StatusBarManager: () => ({
        getConstants: () => ({ HEIGHT: 54, DEFAULT_BACKGROUND_COLOR: 0, STYLE: 'default' }),
        ...listener, getHeight: (cb) => cb({ height: 54 }),
        setStyle: (s) => log('SimCore: StatusBar.setStyle', s), setHidden: noop, setNetworkActivityIndicatorVisible: noop,
      }),
      ExceptionsManager: () => ({
        reportException, reportFatalException: (m, s) => reportException({ message: m, stack: s }),
        reportSoftException: (m) => log('SimCore soft exception:', m), updateExceptionMessage: noop, dismissRedbox: noop,
      }),
      AppState: () => ({ getConstants: () => ({ initialAppState: 'active' }), getCurrentAppState: (cb) => cb({ app_state: 'active' }), ...listener }),
      SettingsManager: () => ({ getConstants: () => ({ settings: { AppleLocale: 'en_US', AppleLanguages: ['en'] } }), setValues: noop, deleteValues: noop }),
      AccessibilityInfo: () => stub({
        isReduceMotionEnabled: (cb) => cb(false), isReduceTransparencyEnabled: (cb) => cb(false), isBoldTextEnabled: (cb) => cb(false),
        isGrayscaleEnabled: (cb) => cb(false), isInvertColorsEnabled: (cb) => cb(false), isDarkerSystemColorsEnabled: (cb) => cb(false),
        isScreenReaderEnabled: (cb) => cb(false), getCurrentReduceMotionState: (cb) => cb(false), getCurrentReduceTransparencyState: (cb) => cb(false),
        getCurrentBoldTextState: (cb) => cb(false), getCurrentGrayscaleState: (cb) => cb(false), getCurrentInvertColorsState: (cb) => cb(false),
        getCurrentDarkerSystemColorsState: (cb) => cb(false), getCurrentPrefersCrossFadeTransitionsState: (cb) => cb(false),
        setAccessibilityFocus: noop, announceForAccessibility: noop, ...listener,
      }),
      KeyboardObserver: () => listener,
      Networking: () => self.networking(),
      BlobModule: () => self.blobModule(),
      FileReaderModule: () => self.fileReaderModule(),
      LogBox: () => ({ show: noop, hide: noop }),
      DevSettings: () => ({ reload: () => self.reload('reload'), reloadWithReason: (r) => self.reload(r), onFastRefresh: () => sendEvent('refresh.applied', {}), ...listener, setHotLoadingEnabled: noop, setIsDebuggingRemotely: noop, toggleElementInspector: noop, addMenuItem: noop, openDebugger: noop, setProfilingEnabled: noop }),
      DevLoadingView: () => ({ showMessage: noop, hide: noop }),
      DevMenu: () => ({ show: noop, reload: noop, setProfilingEnabled: noop, setHotLoadingEnabled: noop }),
      RedBox: () => ({ setExtraData: noop, dismiss: noop }),
      Timing: () => ({ createTimer: noop, deleteTimer: noop, setSendIdleEvents: noop }),
      NativePerformanceCxx: () => ({
        now: () => node.performance.now(), timeOrigin: () => node.performance.timeOrigin, reportMark: noop, reportMeasure: noop,
        getMarkTime: () => undefined, clearMarks: noop, clearMeasures: noop, getEntries: () => [], getEntriesByName: () => [],
        getEntriesByType: () => [], getEventCounts: () => [], getSimpleMemoryInfo: () => ({}), getReactNativeStartupTiming: () => ({}),
        createObserver: () => ({}), getDroppedEntriesCount: () => 0, observe: noop, disconnect: noop, takeRecords: () => [],
        getSupportedPerformanceEntryTypes: () => [], clearEventCountsForTesting: noop,
      }),
      NativeMicrotasksCxx: () => ({ queueMicrotask: (f) => node.queueMicrotask(self.guard(f)) }),
      // Sizes are pixels (scale 1), like iOS for an unscaled image.
      ImageLoader: () => ({
        getConstants: () => ({}),
        getSize: (uri) => loadImage(uri).then((i) => [i.width, i.height]),
        getSizeWithHeaders: (uri, headers) => loadImage(uri, headers).then((i) => ({ width: i.width, height: i.height })),
        prefetchImage: (uri) => loadImage(uri).then(() => true, () => false),
        prefetchImageWithMetadata: (uri) => loadImage(uri).then(() => true, () => false),
        queryCache: (uris) => Promise.resolve(Object.fromEntries(uris.filter((u) => imageDone.has(u)).map((u) => [u, 'disk']))),
        abortRequest: noop,
      }),
      LinkingManager: () => ({ getInitialURL: () => Promise.resolve(null), canOpenURL: () => Promise.resolve(false), openURL: () => { unsupported('LinkingManager', 'openURL'); return Promise.reject(new Error('openURL unsupported')); }, openSettings: () => Promise.reject(new Error('unsupported')), ...listener }),
      WebSocketModule: () => self.webSocketModule(),
      // JS defaults apply for every flag when the native getter returns undefined.
      NativeReactNativeFeatureFlagsCxx: () => new Proxy({}, { get: (t, p) => (typeof p === 'string' ? () => undefined : undefined) }),
      // iPhone 17 Pro portrait insets; the Rust host owns the real chrome.
      RNCSafeAreaContext: () => ({ getConstants: () => ({ initialWindowMetrics: { frame: { x: 0, y: 0, width: self.window.width, height: self.window.height }, insets: { top: 62, left: 0, right: 0, bottom: 34 } } }) }),
      NativeDOMCxx: () => self.domModule(),
      EventDispatcher: () => ({}),
    };
    // Optional modules RN feature-detects: absent => JS falls back (JS Animated driver, no perf API, ...).
    const absent = new Set(['NativeAnimatedModule', 'NativeAnimatedTurboModule', 'NativeCPUTime', 'ReactDevToolsRuntimeSettingsModule', 'NativeReactDevToolsSettingsManager', 'NativeAnimatedModule', 'NativeIdleCallbacksCxx', 'NativeIdleCallbacks', 'JSCExecutor', 'IntentAndroid', 'SoundManager', 'Performance', 'HeadlessJsTaskSupport', 'PerformanceObserver', 'IntersectionObserver', 'MutationObserver', 'ResizeObserver', 'FrameRateLogger', 'UIManager', 'I18nManagerAndroid']);
    // Real iOS RN modules with platform behaviour we do not implement: every call is a loud diagnostic.
    // Anything not listed anywhere is simply absent (null), so getEnforcing throws and the error is reported.
    const known = new Set(['ActionSheetManager', 'AlertManager', 'Clipboard', 'ShareModule', 'PushNotificationManager', 'ImageStoreManager', 'AccessibilityManager', 'DevToolsSettingsManager', 'Vibration', 'ImageEditor']);
    const cache = new Map();
    return {
      absent,
      explicit: (name) => (modules[name] ? modules[name]() : undefined), // legacy NativeModules.X: absent = undefined
      get(name) {
        if (cache.has(name)) return cache.get(name);
        let m = null;
        if (modules[name]) m = modules[name]();
        else if (known.has(name)) {
          m = new Proxy({}, {
            get: (t, p) => {
              if (typeof p === 'symbol' || p === 'then' || p === 'toJSON') return undefined;
              if (p === 'getConstants') return () => { unsupported(name, 'getConstants'); return {}; };
              return () => { unsupported(name, String(p)); };
            },
          });
        } else if (!absent.has(name)) log(`SimCore: TurboModule "${name}" is not available (null, as on a binary without it)`);
        cache.set(name, m);
        return m;
      },
    };
  }

  // ---- Networking (Node fetch). Events are RCTDeviceEventEmitter events whose payload is the args array. ----
  networking() {
    const live = new Map();
    let next = 0;
    const ev = (name, ...args) => this.emit(name, args);
    const bodyOf = async (q) => {
      const d = q.data || {};
      if (d.string != null) return { body: d.string };
      if (d.base64 != null) return { body: node.Buffer.from(d.base64, 'base64') };
      if (d.blob) return { body: this.blobBuffer(d.blob) };
      if (d.uri) return { body: await readUri(d.uri) };
      if (d.formData) {
        const boundary = `----SimCoreFormBoundary${node.randomUUID().replace(/-/g, '')}`, chunks = [];
        for (const part of d.formData) {
          const hs = Object.entries(part.headers || {}).map(([k, v]) => `${k}: ${v}\r\n`).join('');
          chunks.push(node.Buffer.from(`--${boundary}\r\n${hs}\r\n`), part.uri ? await readUri(part.uri) : node.Buffer.from(String(part.string ?? '')), node.Buffer.from('\r\n'));
        }
        chunks.push(node.Buffer.from(`--${boundary}--\r\n`));
        return { body: node.Buffer.concat(chunks), contentType: `multipart/form-data; boundary=${boundary}` };
      }
      return {};
    };
    const run = async (id, q, flags) => {
      try {
        const { body, contentType } = await bodyOf(q);
        const headers = { ...q.headers };
        if (contentType) { for (const k of Object.keys(headers)) if (k.toLowerCase() === 'content-type') delete headers[k]; headers['content-type'] = contentType; }
        const method = String(q.method || 'GET').toUpperCase();
        const dec = new node.TextDecoder();
        let loaded = 0, total = -1;
        const inc = q.responseType === 'text' && q.incrementalUpdates;
        const req = nfetch(q.url, {
          method, headers, redirect: 'follow', body: method === 'GET' || method === 'HEAD' ? undefined : body,
          onChunk: inc ? (c) => { loaded += c.length; ev('didReceiveNetworkIncrementalData', id, dec.decode(c, { stream: true }), loaded, total); } : undefined,
        });
        flags.abort = req.abort;
        if (flags.aborted) req.abort();
        const res = await req;
        total = Number(res.get('content-length')) || -1;
        ev('didReceiveNetworkResponse', id, res.status, Object.fromEntries(res.headers), res.url);
        const bytes = await res.done;
        if (q.responseType === 'text') { if (!inc) ev('didReceiveNetworkData', id, bytes.toString('utf8')); }
        else {
          const buf = bytes;
          if (q.responseType === 'base64') ev('didReceiveNetworkData', id, buf.toString('base64'));
          else {
            const blobId = node.randomUUID();
            this.blobs.set(blobId, buf);
            ev('didReceiveNetworkData', id, { blobId, offset: 0, size: buf.length, type: res.get('content-type') || '' });
          }
        }
        ev('didCompleteNetworkResponse', id, '', false);
      } catch (e) {
        if (flags.aborted) return;
        ev('didCompleteNetworkResponse', id, flags.timedOut ? 'Request timed out' : String(e && (e.cause && e.cause.message || e.message) || e), !!flags.timedOut);
      } finally { live.delete(id); node.clearTimeout(flags.timer); }
    };
    return {
      addListener: () => {}, removeListeners: () => {},
      sendRequest: (q, cb) => {
        const id = ++next, flags = {};
        live.set(id, flags);
        if (q.timeout > 0) flags.timer = node.setTimeout(() => { flags.timedOut = true; flags.abort && flags.abort(); }, q.timeout);
        cb(id);
        node.setImmediate(this.guard(() => run(id, q, flags)));
      },
      abortRequest: (id) => { const f = live.get(id); if (f) { f.aborted = true; f.abort && f.abort(); } },
      clearCookies: (cb) => cb(false), // no cookie jar: nothing to clear
    };
  }
  blobBuffer(b) {
    const buf = this.blobs.get(b.blobId);
    if (!buf) throw new Error(`unknown blob ${b.blobId}`);
    return buf.subarray(b.offset || 0, (b.offset || 0) + (b.size ?? buf.length));
  }
  // Minimal BlobModule: an in-memory id -> Buffer store (no socket handlers).
  blobModule() {
    return {
      getConstants: () => ({ BLOB_URI_SCHEME: 'blob', BLOB_URI_HOST: null }),
      addNetworkingHandler: () => {}, addWebSocketHandler: () => {}, removeWebSocketHandler: () => {},
      sendOverSocket: () => unsupported('BlobModule', 'sendOverSocket'),
      createFromParts: (parts, id) => this.blobs.set(id, node.Buffer.concat(parts.map((p) => (p.type === 'blob' ? this.blobBuffer(p.data) : node.Buffer.from(String(p.data), 'utf8'))))),
      release: (id) => { this.blobs.delete(id); },
      addListener: () => {}, removeListeners: () => {},
    };
  }
  fileReaderModule() {
    return {
      readAsText: async (b, enc) => new node.TextDecoder(enc || 'utf-8').decode(this.blobBuffer(b)),
      readAsDataURL: async (b) => `data:${b.type || 'application/octet-stream'};base64,${this.blobBuffer(b).toString('base64')}`,
    };
  }

  // WebSocketModule: sockets live on the main thread (Node's WebSocket breaks once RN replaces global Event);
  // this just forwards. Needed for Metro's /hot (HMR) and expo's /message channels.
  webSocketModule() {
    const to = (op, args) => send({ type: '__ws', op, ...args });
    return {
      connect: (url, protocols, options, id) => to('connect', { url, protocols, id }),
      send: (data, id) => to('send', { data, id }),
      sendBinary: (b64, id) => to('send', { data: node.Buffer.from(b64, 'base64'), id }),
      ping: () => {},
      close: (code, reason, id) => to('close', { code, reason, id }),
      addListener: () => {}, removeListeners: () => {},
    };
  }

  // ---- expo-modules-core JSI surface (globalThis.expo) ----
  makeExpo() {
    const self = this;
    class EventEmitter {
      constructor() { this._l = new Map(); }
      addListener(n, f) { if (!this._l.has(n)) this._l.set(n, new Set()); this._l.get(n).add(f); return { remove: () => this._l.get(n)?.delete(f) }; }
      removeListener(n, f) { this._l.get(n)?.delete(f); }
      removeAllListeners(n) { this._l.delete(n); }
      emit(n, ...a) { for (const f of [...(this._l.get(n) || [])]) f(...a); }
      listenerCount(n) { return this._l.get(n)?.size || 0; }
      startObserving() {} stopObserving() {}
    }
    class SharedObject extends EventEmitter { release() {} }
    class SharedRef extends SharedObject {}
    class NativeModule extends EventEmitter {}
    // A native class whose every (non-SharedObject) method reports "unsupported" when called.
    const diagClass = (mod, cls) => {
      const C = class extends SharedObject {};
      Object.setPrototypeOf(C.prototype, new Proxy(SharedObject.prototype, {
        get: (t, p, r) => (p in t || typeof p === 'symbol' ? Reflect.get(t, p, r) : () => { unsupported(`expo.${mod}`, `${cls}.${String(p)}`); }),
      }));
      return C;
    };
    const defs = {
      ExpoFetchModule: () => self.expoFetch(SharedObject),
      ExpoAsset: () => ({ downloadAsync: async (uri) => { unsupported('expo.ExpoAsset', 'downloadAsync'); return uri; } }),
      ExpoFontLoader: () => ({
        getLoadedFonts: () => [], isLoaded: () => false,
        loadAsync: async (name) => { unsupported('expo.ExpoFontLoader', `loadAsync(${name})`); },
      }),
      // Screen-awake is a no-op in a simulator surface; accepted, not rendered.
      ExpoKeepAwake: () => ({ activate: async () => {}, deactivate: async () => {}, isActivated: async () => false }),
      // expo-constants: the manifest is the same one Metro hands a real Expo client; expo-asset uses it to
      // build dev-server asset URLs.
      ExponentConstants: () => ({
        manifest: self.manifest, experienceUrl: self.manifest ? `exp://${new URL(self.scriptURL).host}` : null,
        appOwnership: null, sessionId: cfg.session || 'simcore', installationId: 'simcore', statusBarHeight: 54,
        systemFonts: [], isDevice: false, deviceName: 'iPhone', expoVersion: null, platform: { ios: { model: 'iPhone', systemVersion: '26.5' } },
        getWebViewUserAgentAsync: async () => 'Mozilla/5.0 (iPhone; CPU iPhone OS 26_5 like Mac OS X) SimCore',
      }),
    };
    // Unknown modules are *absent* (undefined) so requireNativeModule throws / requireOptionalNativeModule is null,
    // exactly as on a device without the module linked.
    const modules = new Proxy({}, {
      get: (t, name) => {
        if (typeof name === 'symbol') return undefined;
        if (!(name in t)) {
          t[name] = defs[name] ? Object.assign(new NativeModule(), defs[name](), { __expo_module_name__: name }) : undefined;
          if (!defs[name]) log(`SimCore: expo module "${name}" is not available`);
        }
        return t[name];
      },
    });
    return {
      EventEmitter, SharedObject, SharedRef, NativeModule, modules,
      // ExpoModulesCore reads these; "bare" matches an app without native expo modules linked.
      getViewConfig: () => null, reloadAppAsync: async () => self.reload('expo reloadAppAsync'),
      uuidv4: () => node.randomUUID(), uuidv5: (n, ns) => ns,
      expoModulesCoreVersion: { version: '0.0.0', major: 0, minor: 0, patch: 0 },
      DeviceEventEmitter: undefined, // set below once RN's emitter exists
    };
  }

  // expo/fetch (global `fetch` in an Expo app): NativeRequest.start -> NativeResponse. The body is buffered
  // whole (startStreaming resolves with all bytes; no incremental didReceiveResponseData).
  expoFetch(SharedObject) {
    const toU8 = (buf) => new node.Uint8Array(buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength));
    class NativeResponse extends SharedObject {
      get bodyUsed() { return !!this.__used; }
      get _rawHeaders() { return this.__r ? this.__r.headers : []; }
      get status() { return this.__r ? this.__r.status : 0; }
      get statusText() { return this.__r ? this.__r.statusText : ''; }
      get url() { return this.__r ? this.__r.url : ''; }
      get redirected() { return this.__r ? this.__r.redirected : false; }
      async __bytes() { this.__used = true; return this.__r.done; }
      async startStreaming() { return toU8(await this.__bytes()); }
      cancelStreaming() { this.__used = true; if (this.__r) this.__r.abort(); }
      async arrayBuffer() { return toU8(await this.__bytes()).buffer; }
      async text() { return (await this.__bytes()).toString('utf8'); }
    }
    class NativeRequest extends SharedObject {
      constructor(response) { super(); this.response = response; }
      async start(url, init, body) {
        const method = (init.method || 'GET').toUpperCase();
        this.req = nfetch(url, { method, headers: init.headers || [], redirect: init.redirect || 'follow', body: body && method !== 'GET' && method !== 'HEAD' ? node.Buffer.from(body) : undefined });
        try { this.response.__r = await this.req; } catch (e) { throw new Error(String(e && e.message || e)); }
        return this.response;
      }
      cancel() { if (this.req) this.req.abort(); }
    }
    return { NativeResponse, NativeRequest };
  }

  // ---- boot ----
  async boot() {
    const { url: bundleUrl, manifest } = await resolveBundleUrl();
    this.scriptURL = bundleUrl;
    this.manifest = manifest;
    let src;
    try {
      const r = await node.fetch(bundleUrl);
      if (!r.ok) throw new Error(`HTTP ${r.status}: ${(await r.text()).slice(0, 500)}`);
      src = await r.text();
    } catch (e) { sendEvent('bundle.error', { message: e.message, url: bundleUrl }); return sendError(`bundle fetch failed (${bundleUrl}): ${e.message}`); }
    if (this.dead) return;

    const modules = this.makeModules(src);
    const t = this.makeTimers();
    const sandbox = {
      ...t,
      // RN iOS's HMRClient (metro-runtime) evals update modules through this.
      // In __DEV__ RN reports uncaught/render errors to LogBox only, never to ExceptionsManager.reportException.
      // ExceptionsManager.handleException (every error path ends there) asks this hook first; false = carry on as usual.
      'RN$handleException': (e) => { sendError(e && e.stack ? (e.name && !String(e.stack).startsWith(e.name) ? `${e.name}: ` : '') + e.stack : e); return false; },
      globalEvalWithSourceUrl: (code, url) => vm.runInThisContext(code, { filename: url }),
      nativeLoggingHook: (msg, level) => log(`[${level}] ${msg}`),
      nativePerformanceNow: () => node.performance.now(),
      RN$Bridgeless: true,
      RN$registerCallableModule: (name, f) => this.callable.set(name, f),
      nativeFabricUIManager: (this.fabric = this.makeFabric()),
      // No custom native views (DebuggingOverlay, TraceUpdateOverlay, ...) exist in the SimCore host.
      __nativeComponentRegistry__hasComponent: () => false,
      __turboModuleProxy: (n) => modules.get(n),
      nativeModuleProxy: new Proxy({}, { get: (_, n) => (typeof n === 'string' ? modules.explicit(n) : undefined) }),
      expo: this.makeExpo(),
    };
    Object.assign(globalThis, sandbox);
    delete globalThis.process; delete globalThis.Buffer; // Node-isms Hermes doesn't have; Metro's prelude makes its own `process`
    try {
      vm.runInThisContext(src, { filename: bundleUrl });
      const keys = globalThis.RN$AppRegistry.getAppKeys().filter((k) => k !== 'LogBox');
      const key = keys.includes('main') ? 'main' : keys[0];
      if (!key) return sendError('no component registered with AppRegistry');
      const reg = this.getCallable('AppRegistry');
      this.guard(() => reg.runApplication(key, { rootTag: ROOT_TAG, initialProps: {}, fabric: true }))();
      sendEvent(cfg.generation ? 'bundle.reloaded' : 'bundle.loaded', { url: bundleUrl });
      if (cfg.hmr) this.setupHmr();
    } catch (e) { this.onUncaught(e); } finally {
      if (env.SIMCORE_DEBUG_EVAL) log('DEBUG', String(vm.runInThisContext(env.SIMCORE_DEBUG_EVAL)));
    }
  }

  setupHmr() {
    const u = new URL(this.scriptURL);
    const hmr = this.getCallable('HMRClient');
    const entry = (u.pathname.match(/([^/]+)\.bundle/) || [0, 'index'])[1];
    if (hmr) this.guard(() => hmr.setup('ios', entry, u.hostname, Number(u.port) || (u.protocol === 'https:' ? 443 : 80), true, u.protocol.replace(':', '')))();
  }
}

// Manifest first (what a real Expo client does), then plain Metro bundle URL.
async function resolveBundleUrl() {
  const o = new URL(cfg.metro);
  try {
    const r = await node.fetch(cfg.metro + '/', { headers: { 'expo-platform': 'ios', accept: 'application/expo+json,application/json' } });
    if (r.ok && /json/.test(r.headers.get('content-type') || '')) {
      const manifest = await r.json();
      const u = new URL(manifest.launchAsset.url);
      u.protocol = o.protocol; u.host = o.host; // manifest may say 127.0.0.1
      u.searchParams.set('lazy', 'false'); // no lazy bundle splitting: one fetch, Node-side
      u.searchParams.set('minify', 'false');
      u.searchParams.delete('transform.bytecode'); // Hermes bytecode hint is meaningless for us
      u.searchParams.delete('unstable_transformProfile');
      u.searchParams.delete('transform.engine');
      return { url: u.toString(), manifest };
    }
  } catch (_) { /* not an Expo server */ }
  return { url: `${cfg.metro}/index.bundle?platform=ios&dev=true&minify=false`, manifest: null };
}

// Worker entry.
console = new console.Console(process.stderr); // eslint-disable-line no-global-assign
process.stdout.write = (...a) => process.stderr.write(...a);
process.on('uncaughtException', (e) => sendError(e && e.stack ? e.stack : e));
process.on('unhandledRejection', (e) => sendError(e && e.stack ? e.stack : e));
const rt = new Runtime();
parentPort.on('message', (m) => rt.onMessage(m));
rt.boot();

// Worker HTTP requests (nfetch) run here, in an untouched Node global; results stream back as messages.
async function mainFetch(w, m, fetching) {
  const ac = new AbortController(), post = (x) => { try { w.postMessage({ id: m.id, ...x }); } catch (_) { /* worker gone */ } };
  fetching.set(m.id, ac);
  try {
    const res = await fetch(m.url, { method: m.method, headers: m.headers, redirect: m.redirect || 'follow', body: m.body || undefined, signal: ac.signal });
    post({ type: '__fetchHead', status: res.status, statusText: res.statusText, url: res.url, redirected: res.redirected, headers: [...res.headers] });
    if (res.body) for await (const c of res.body) post({ type: '__fetchChunk', data: c });
    post({ type: '__fetchEnd' });
  } catch (e) { post({ type: '__fetchError', message: String((e && e.cause && e.cause.message) || (e && e.message) || e) }); }
  finally { fetching.delete(m.id); }
}

// Main thread: protocol I/O + worker supervisor.
function mainThread() {
  let worker = null, base = 0, last = 0, generation = -1;
  const out = (m) => process.stdout.write(JSON.stringify(m) + '\n');
  const start = () => {
    if (worker) worker.terminate();
    base = last; // tree versions stay monotonic across reloads
    const w = (worker = new Worker(__filename, { argv: process.argv.slice(2), stdout: false, stderr: false, workerData: { generation: ++generation } }));
    const sockets = new Map(), fetching = new Map();
    w.on('exit', () => { sockets.forEach((ws) => ws.close()); fetching.forEach((ac) => ac.abort()); fetching.clear(); });
    w.on('message', (m) => {
      if (m.type === 'tree') { m.version += base; last = m.version; }
      if (w !== worker) return; // a worker that was already replaced
      if (m.type === '__reload') return start();
      if (m.type === '__fetch') return mainFetch(w, m, fetching);
      if (m.type === '__fetchAbort') return fetching.get(m.id)?.abort();
      if (m.type !== '__ws') return out(m);
      const emit = (event, payload) => w.postMessage({ type: '__wsEvent', event, payload });
      if (m.op === 'connect') {
        if (env.SIMCORE_DEBUG) process.stderr.write(`SimCore: ws connect ${m.url}\n`);
        const ws = new WebSocket(m.url, m.protocols || undefined);
        ws.binaryType = 'arraybuffer';
        sockets.set(m.id, ws);
        ws.onopen = () => { if (env.SIMCORE_DEBUG) process.stderr.write('SimCore: ws open\n'); emit('websocketOpen', { id: m.id }); };
        ws.onmessage = (ev) => emit('websocketMessage', { id: m.id, type: typeof ev.data === 'string' ? 'text' : 'binary', data: typeof ev.data === 'string' ? ev.data : Buffer.from(ev.data).toString('base64') });
        ws.onerror = () => emit('websocketFailed', { id: m.id, message: 'WebSocket error' });
        ws.onclose = (ev) => { sockets.delete(m.id); emit('websocketClosed', { id: m.id, code: ev.code, reason: ev.reason }); };
      } else if (m.op === 'send') sockets.get(m.id)?.send(m.data);
      else if (m.op === 'close') sockets.get(m.id)?.close(m.code, m.reason);
    });
    w.on('error', (e) => out({ type: 'error', message: String(e && e.stack || e) }));
  };
  out({ type: 'hello', protocol: 1 });
  const rl = readline.createInterface({ input: process.stdin });
  rl.on('line', (line) => {
    let m; try { m = JSON.parse(line); } catch (_) { return; }
    if (m.type === 'shutdown') process.exit(0);
    else if (m.type === 'reload') start();
    else if (worker) worker.postMessage(m);
  });
  rl.on('close', () => process.exit(0));
  start();
}
