#!/usr/bin/env node
// OneSim React Native guest. Runs an unmodified Metro bundle in a bare vm context (like Hermes: ES
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
const expoUI = require('./expo-ui.js');
const { patchAnimated } = require('./bundle-patches.js');
const { Worker, isMainThread, parentPort, workerData } = require('node:worker_threads');

Error.stackTraceLimit = 60;
const ROOT_TAG = 1;
const TEXT_INPUTS = new Set(['RCTSinglelineTextInputView', 'RCTMultilineTextInputView', 'AndroidTextInput']);
const SCROLL_VIEWS = new Set(['RCTScrollView', 'AndroidHorizontalScrollView']);
// Native views of third-party libraries the host draws/understands (viewName on the wire). Expo module views are
// `ViewManagerAdapter_<Module>[_<View>]`.
const NATIVE_VIEWS = new Set(['RNSScreen', 'RNSModalScreen', 'RNSScreenStack', 'RNSScreenStackHeaderConfig', 'RNSScreenStackHeaderSubview', 'RNSScreenContainer',
  'RNSScreenNavigationContainer', 'RNSScreenContentWrapper', 'RNSScreenFooter', 'RNSSearchBar', 'RNSSafeAreaView', 'RNSFullWindowOverlay',
  'RNSTabsHostIOS', 'RNSTabsScreenIOS', 'RNSTabsBottomAccessory', 'RNSTabsBottomAccessoryContent', 'RNCSafeAreaProvider', 'RNCSafeAreaView',
  'RNGestureHandlerRootView', 'RNGestureHandlerButton', 'RNGestureHandlerDetector']);
// Props / events of expo module views (from the modules' Swift View definitions): `getViewConfig` hands them to RN as the
// native view config, so only listed props reach the host. Event `onFoo` is the direct event `topFoo`.
const EXPO_VIEWS = {
  ExpoImage: { events: ['onLoadStart', 'onProgress', 'onError', 'onLoad', 'onDisplay'], props: ['source', 'placeholder', 'contentFit', 'placeholderContentFit', 'contentPosition', 'transition', 'blurRadius', 'tintColor', 'svgVariables', 'priority', 'cachePolicy', 'enableLiveTextInteraction', 'accessible', 'accessibilityLabel', 'accessibilityElementsHidden', 'recyclingKey', 'allowDownscaling', 'autoplay', 'sfEffect', 'symbolWeight', 'symbolSize', 'useAppleWebpCodec', 'enforceEarlyResizing', 'preferHighDynamicRange'] },
  SymbolModule: { events: [], props: ['name', 'type', 'scale', 'tintColor', 'animated', 'weight', 'colors', 'resizeMode', 'animationSpec'] },
  ExpoGlassEffect_GlassView: { events: [], props: ['glassEffectStyle', 'tintColor', 'isInteractive', 'colorScheme', 'borderRadius', 'borderBottomLeftRadius', 'borderBottomRightRadius', 'borderTopLeftRadius', 'borderTopRightRadius', 'borderTopStartRadius', 'borderTopEndRadius', 'borderBottomStartRadius', 'borderBottomEndRadius', 'borderCurve'] },
  ExpoGlassEffect_GlassContainer: { events: [], props: ['spacing'] },
  ExpoLinearGradient: { events: [], props: ['colors', 'startPoint', 'endPoint', 'locations', 'borderRadii'] },
  ExpoBlur_ExpoBlurView: { events: [], props: ['tint', 'intensity'] },
  ...Object.fromEntries(Object.entries(expoUI.views).map(([view, config]) => ['ExpoUI_' + view, config])),
};
function expoViewConfig(key) {
  const v = EXPO_VIEWS[key];
  if (!v) return null;
  return {
    validAttributes: Object.fromEntries([...v.props, ...v.events].map((k) => [k, true])),
    directEventTypes: Object.fromEntries(v.events.map((e) => ['top' + e.slice(2), { registrationName: e }])),
  };
}
// Duration of a stack push/pop (UINavigationController default) between willAppear and appear; SIMCORE_NAV_MS overrides (0 = instant).
// Pull distance (pt) past the top at which a RefreshControl fires onRefresh. ponytail: approximate, not measured on iOS.
const REFRESH_PULL = 100;
const NAV_MS = process.env.SIMCORE_NAV_MS != null ? Number(process.env.SIMCORE_NAV_MS) : 350;
// Safe-area insets of portrait iPhones by logical height (Dynamic Island 62, notch/DI 59 or 47, X-style notch 44, home-button 20).
const insetsFor = (h) => cfg.platform === 'android'
  ? cfg.androidInsets
  : { top: h >= 874 ? 62 : h >= 852 ? 59 : h >= 812 ? (h >= 844 ? 47 : 44) : 20, left: 0, right: 0, bottom: h >= 812 ? 34 : 0 };
const NAV_KIND = { RNSScreenStack: 'stacks', RNSTabsHostIOS: 'tabs', RNCSafeAreaProvider: 'providers', RNSScreenStackHeaderConfig: 'headers' };
const EXPO_IMAGE = 'ViewManagerAdapter_ExpoImage';
const isNativeView = (n) => NATIVE_VIEWS.has(n) || /^ViewManagerAdapter_/.test(n);
const env = process.env; // `process` is removed from the guest global before the bundle runs
if (isMainThread) return mainThread();

// Node intrinsics captured before RN's polyfills replace the globals of the same name.
const node = {
  Buffer, setTimeout, clearTimeout, setInterval, clearInterval, setImmediate, queueMicrotask, performance, fetch,
  AbortController, TextDecoder, Uint8Array,
  inspect: require('node:util').inspect, randomUUID: require('node:crypto').randomUUID,
  // Reading a lazy undici global loads undici now: RN's polyfillGlobal reads these descriptors once Buffer is gone.
  Headers: globalThis.Headers,
};
// HTTP runs on the main thread (see __fetch): once RN's polyfills replace timers/Promise/Buffer in this global,
// Node's own fetch (undici) breaks here. The worker only keeps the plain `node.fetch` for Metro's manifest.
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
  platform: env.SIMCORE_PLATFORM === 'android' ? 'android' : 'ios',
  appearance: process.env.SIMCORE_APPEARANCE === 'dark' ? 'dark' : 'light',
  scale: Number(process.env.SIMCORE_SCALE) || 3,
  session: process.env.SIMCORE_SESSION_ID || '',
  hmr: process.env.SIMCORE_HMR !== '0' && !env.SIMCORE_BUNDLE_URL, // Fast Refresh via Metro's /hot; SIMCORE_HMR=0 turns it off
  generation: (workerData && workerData.generation) || 0, // 0 = first load, >0 = after a reload
  dataDir: workerData && workerData.dataDir, // per-session sandbox (documents, caches, secure store): survives reloads, removed on exit
  launchURL: env.SIMCORE_LAUNCH_URL || null, // the URL the app was launched to open (Linking.getInitialURL)
};
{
  const android = cfg.platform === 'android';
  const fallback = android ? [1080 / 2.625, 2424 / 2.625] : [393, 852];
  const m = /^(\d+(?:\.\d+)?)x(\d+(?:\.\d+)?)$/.exec(env.SIMCORE_VIEWPORT || '');
  [cfg.width, cfg.height] = m ? m.slice(1).map(Number) : fallback;
  const [top, bottom] = env.SIMCORE_INSETS ? env.SIMCORE_INSETS.split(',').map(Number) : [142 / 2.625, 24];
  cfg.androidInsets = { top, bottom, left: 0, right: 0 };
  if (!android && env.SIMCORE_INSETS && Number.isFinite(top)) cfg.iosStatusBarHeight = top;
  if (android && !env.SIMCORE_SCALE) cfg.scale = 2.625;
}

const sendEvent = (name, detail) => send({ type: 'event', name, detail: detail || {} });
const sendError = (message) => { log('OneSim error:', message); send({ type: 'error', message: String(message) }); };
const reportedModules = new Set();
// Diagnostic for functionality OneSim does not provide: stderr every time, `error` + `module.unsupported` once per module+method.
// Returns the Error callers throw / reject with (message = first diagnostic line).
function unsupported(mod, method, pkg) {
  const first = `OneSim: ${mod} is not supported.`, text = `${first}\n\nModule: ${pkg || mod}\nMethod: ${method}`;
  log(text);
  if (env.SIMCORE_DEBUG) log(new Error().stack);
  if (!reportedModules.has(`${mod}.${method}`)) {
    reportedModules.add(`${mod}.${method}`);
    send({ type: 'error', message: text });
    sendEvent('module.unsupported', pkg ? { module: mod, method, package: pkg } : { module: mod, method });
  }
  return new Error(first);
}

// ---------------------------------------------------------------------------------------------
// Image cache: $TMPDIR/onesim-images/<sha1(key)>-<sha1(bytes)>.<ext>. Shared by RCTImageView and ImageLoader. The
// content hash gives changed bytes a new file, so the host (which caches decodes by path) draws the new image after
// a reload.
// ---------------------------------------------------------------------------------------------
const imageCache = new Map(); // uri -> Promise<{path,width,height}> (pixels)
const imageDone = new Map(); // uri -> info, once loaded (queryCache)
// Pixel size + extension from the file header (PNG, JPEG, GIF, WebP); null if unrecognised.
function imageInfo(b) {
  if (b.length > 24 && b.readUInt32BE(0) === 0x89504e47) return { ext: 'png', width: b.readUInt32BE(16), height: b.readUInt32BE(20) };
  if (b.length > 10 && b.toString('latin1', 0, 4) === 'GIF8') {
    // Animated: more than one graphic control extension (one per frame).
    let n = 0;
    for (let i = b.indexOf(0x21); i >= 0 && n < 2; i = b.indexOf(0x21, i + 1)) if (b[i + 1] === 0xf9 && b[i + 2] === 4) n++;
    return { ext: 'gif', width: b.readUInt16LE(6), height: b.readUInt16LE(8), animated: n > 1 };
  }
  if (b.length > 30 && b.toString('latin1', 0, 4) === 'RIFF' && b.toString('latin1', 8, 12) === 'WEBP') {
    const k = b.toString('latin1', 12, 16);
    if (k === 'VP8 ') return { ext: 'webp', width: b.readUInt16LE(26) & 0x3fff, height: b.readUInt16LE(28) & 0x3fff };
    if (k === 'VP8L') { const v = b.readUInt32LE(21); return { ext: 'webp', width: (v & 0x3fff) + 1, height: ((v >> 14) & 0x3fff) + 1 }; }
    if (k === 'VP8X') return { ext: 'webp', width: 1 + b.readUIntLE(24, 3), height: 1 + b.readUIntLE(27, 3), animated: (b[20] & 2) !== 0 };
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
// `blurhash:/` and `thumbhash:/` (expo-image placeholders and sources) are decoded by the host (`image.decode_hash`).
const HASH_URI = /^(blurhash|thumbhash):\//;
// `o.key`: the cache key (expo-image `cacheKey`; default the URI, with the size for a blurhash).
function loadImage(uri, headers, o = {}) {
  const key = o.key || (/^blurhash:/.test(uri) ? `${uri}#${o.width || 16}x${o.height || 16}` : uri);
  let p = imageCache.get(key);
  if (!p) {
    const bytes = HASH_URI.test(uri)
      ? rt.request('image.decode_hash', { uri, width: o.width, height: o.height }).then((r) => node.Buffer.from(r.png, 'base64'))
      : readUri(uri, headers);
    p = bytes.then((buf) => {
      const i = imageInfo(buf);
      if (!i) throw new Error(`not a PNG/JPEG/GIF/WebP image: ${uri.slice(0, 120)}`);
      const dir = path.join(os.tmpdir(), 'onesim-images');
      fs.mkdirSync(dir, { recursive: true });
      const sha1 = (v) => crypto.createHash('sha1').update(v).digest('hex');
      const file = path.join(dir, `${sha1(key)}-${sha1(buf).slice(0, 16)}.${i.ext}`);
      fs.writeFileSync(file, buf);
      const info = { path: file, width: i.width, height: i.height, animated: !!i.animated };
      imageDone.set(key, info);
      return info;
    });
    imageCache.set(key, p);
    p.catch(() => imageCache.delete(key)); // failures are not cached
  }
  return p;
}
// The cache forgets `key` (all keys without one): the next load reads the source again. Files stay for views showing them.
function forgetImage(key) {
  if (key === undefined) { imageCache.clear(); imageDone.clear(); } else { imageCache.delete(key); imageDone.delete(key); }
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
    this.kbFrame = null; // software keyboard frame while the host shows it (KeyboardObserver)
    this.images = new Map(); // tag -> {uri, key, info?, err?, ph?: placeholder {uri, info}}
    this.imageViews = new Map(); // expo-image tag -> {animating?, locked?} (view functions)
    this.reqs = new Map(); // host requests in flight: id -> {res, rej}
    this.reqSeq = 0;
    this.scrolled = new Map(); // tag -> last reported [x,y]
    this.shown = new Set(); // Modal tags that got topShow
    this.alerts = []; // presentation layers (Alert.alert, ActionSheetIOS): [{layer, run: Map(action id -> handler), kind}], bottom to top
    this.nextLayoutAnim = null; // LayoutAnimation.configureNext waiting for its commit: {config, total ms, done}
    this.layoutAnim = null; // ... committed, rides on the next emitted tree
    this.layoutAnimSeq = 0;
    this.appState = 'active'; // AppState: active | inactive | background
    this.liveTouch = null; // the finger down (last start / move), cancelled when the app resigns active
    this.touchGone = false;
    this.blobs = new Map(); // BlobModule store: blobId -> Buffer
    this.eventHandler = null;
    this.simQueue = [];
    this.eventPriority = 32;
    this.worklets = require('./worklets.js')(this, { node, log, sendError, unsupported }); // react-native-worklets / -reanimated natives
    this.gestures = require('./gestures.js')(this, { node, log, unsupported }); // react-native-gesture-handler natives
    this.nav = { stacks: new Map(), tabs: new Map(), providers: new Map(), headers: new Set() }; // native navigation state, see navCommit
    this.rootProps = {}; // props of the root surface (ExpoSystemUI background)
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
    // Shadow nodes are JSI host objects on a device; react-native-worklets recognises those by `MAGIC in value`.
    const hostObject = (n) => Object.defineProperty(n, 'REANIMATED_MAGIC_KEY', { value: true });
    const mk = (n, props, children) => hostObject({ reactTag: n.reactTag, viewName: n.viewName, rootTag: n.rootTag, props, children, instanceHandle: n.instanceHandle });
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
        return reg(hostObject({ reactTag, viewName, rootTag, props: merge({}, props), children: [], instanceHandle }));
      },
      cloneNode: (n) => reg(mk(n, n.props, n.children.slice())),
      cloneNodeWithNewChildren: (n) => reg(mk(n, n.props, [])),
      cloneNodeWithNewProps: (n, p) => reg(mk(n, merge(n.props, p), n.children.slice())),
      cloneNodeWithNewChildrenAndProps: (n, p) => reg(mk(n, merge(n.props, p), [])),
      createChildSet: () => [],
      appendChild: (parent, child) => { parent.children.push(child); return parent; },
      appendChildToSet: (set, child) => { set.push(child); },
      completeRoot: (rootTag, childSet) => {
        env.SIMCORE_DEBUG && log('completeRoot', rootTag, childSet.length);
        self.rootChildren = childSet;
        // LayoutAnimation.configureNext applies to the next commit: this one.
        if (self.nextLayoutAnim) { self.layoutAnim = self.nextLayoutAnim; self.nextLayoutAnim = null; }
        self.scheduleEmit();
      },
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
      // The host animates the commit (protocol H: `__simcoreLayoutAnimation`); `onEnd` runs once it has played.
      configureNextLayoutAnimation: (config, onEnd, onFail) => {
        if (!config || typeof config !== 'object') { if (typeof onFail === 'function') node.queueMicrotask(self.guard(onFail)); return; }
        if (self.nextLayoutAnim) self.nextLayoutAnim.done(); // replaced before any commit: the earlier one ends unplayed
        const part = (k) => config[k] && typeof config[k] === 'object' ? config[k] : null;
        const ms = [part('create'), part('update'), part('delete')].filter(Boolean)
          .map((p) => (Number.isFinite(p.duration) ? p.duration : Number(config.duration) || 0) + (Number(p.delay) || 0));
        const total = Math.max(0, ...ms);
        self.nextLayoutAnim = { config: JSON.parse(JSON.stringify(config)), total, done: typeof onEnd === 'function' ? self.guard(() => onEnd()) : () => {} };
      },
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
      // A responder that blocks native ones (a PanResponder's grant) stops the scroll views inside it (RCTScrollView's
      // _shouldDisableScrollInteraction); the host arbitrates.
      setIsJSResponder: (n, isResponder, block) => send({ type: 'js_responder', tag: isResponder && block ? n.reactTag : null }),
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
      this.worklets.beforeEmit(this.rootChildren);
      const after = []; // events to dispatch once the tree is out (first emit of an image / visible modal)
      const found = { stacks: [], tabs: [], providers: [], headers: [] };
      const ser = (n, parent) => {
        this.parents.set(n.reactTag, parent);
        const kind = NAV_KIND[n.viewName];
        if (kind) found[kind].push(n);
        const props = {};
        const animated = this.worklets.overlay(n.reactTag);
        for (const [k, v] of Object.entries(animated ? Object.assign({}, n.props, animated) : n.props)) {
          if (typeof v === 'function' || v === undefined) continue;
          props[k] = v;
        }
        if (TEXT_INPUTS.has(n.viewName)) props.__simcoreText = this.input(n).text;
        else if (expoUI.isField(n.viewName)) {
          props.__simcoreText = this.expoFieldText(n);
          if (props.autoFocus && !this.shown.has(n.reactTag)) {
            this.shown.add(n.reactTag);
            after.push(() => this.requestFocus(n.reactTag));
          }
        }
        else if (n.viewName === 'RCTImageView' || n.viewName === EXPO_IMAGE) this.imageProps(n, props, after);
        else if (n.viewName === 'RNSTabsScreenIOS') this.tabIcons(n, props);
        else if (n.viewName === 'ModalHostView') {
          if (props.visible && !this.shown.has(n.reactTag)) { this.shown.add(n.reactTag); after.push(() => this.dispatch(n, 'topShow', { target: n.reactTag })); }
          else if (!props.visible) this.shown.delete(n.reactTag);
        }
        // RN's Fabric name is ModalHostView; the wire contract says RCTModalHostView
        const children = n.children.flatMap((c) => ser(c, n.reactTag));
        if (props.display === 'contents') return children; // no box of its own (RNGH detectors): children join the parent's layout
        const view = { kind: 'rn', id: String(n.reactTag), component: n.viewName === 'ModalHostView' ? 'RCTModalHostView' : n.viewName, props, children };
        if (n.viewName === expoUI.HOST) view.children = [expoUI.nativeTree(view, cfg.platform === 'android', this.appearance)];
        return view;
      };
      let rootProps = this.rootProps;
      const la = this.layoutAnim;
      if (la) { // once: later trees of the same commit (or unrelated ones) do not animate
        this.layoutAnim = null;
        rootProps = { ...rootProps, __simcoreLayoutAnimation: { ...la.config, token: ++this.layoutAnimSeq } };
        node.setTimeout(la.done, la.total);
      }
      const root = { kind: 'rn', id: String(ROOT_TAG), component: 'RootView', props: rootProps, children: this.rootChildren.flatMap((c) => ser(c, ROOT_TAG)) };
      if (env.SIMCORE_DEBUG) log('emit tree', version + 1);
      // An alert up: the RN root becomes a `presentation`'s content (protocol G/H).
      const treeRoot = this.alerts.length ? { kind: 'presentation', id: 'presentation', content: root, layers: this.alerts.map((a) => a.layer) } : root;
      send({ type: '__treeJson', version: ++version, rootJson: JSON.stringify(treeRoot) });
      if (this.navCheck && !this.navSeen) this.navCheck();
      this.navCommit(found, after);
      after.forEach((f) => f());
      this.fireLayouts();
    });
  }

  fireLayouts() {
    for (const [tag, f] of Object.entries(this.frames)) {
      const n = this.nodes.get(+tag);
      if (!n || !this.parents.has(+tag)) continue;
      if (n.viewName === expoUI.HOST && this.frames[tag + '#content']) {
        const [width, height] = (this.frames[tag + '#content'] ?? f).slice(2);
        const key = [width, height].join(',');
        const contentKey = tag + '#content';
        if (this.lastLaidOut[contentKey] !== key) {
          this.lastLaidOut[contentKey] = key;
          this.dispatch(n, 'topLayoutContent', { width, height });
        }
      }
      if (!n.props.onLayout) continue;
      const key = f.join(',');
      if (this.lastLaidOut[tag] === key) continue;
      this.lastLaidOut[tag] = key;
      const pf = this.frames[this.parents.get(+tag)];
      this.dispatch(n, 'topLayout', { layout: { x: f[0] - (pf ? pf[0] : 0), y: f[1] - (pf ? pf[1] : 0), width: f[2], height: f[3] }, target: +tag });
    }
  }

  // ---- native navigation (react-native-screens, safe-area-context) ----
  // The guest owns the shadow tree, so it plays the part of UIKit here: after every committed tree it diffs the screen
  // stacks / tab hosts / safe-area providers against what it reported before and fires the native->JS events iOS would.
  // Descendants of `n` named `names`, not looking inside nested ones of the same kind (wrapper views are transparent).
  collect(n, names, out = []) {
    for (const c of n.children) { if (names.includes(c.viewName)) out.push(c); else if (!NAV_KIND[c.viewName]) this.collect(c, names, out); }
    return out;
  }
  // Run `fn` after the transition time (immediately when 0 or the surface is gone).
  later(ms, fn) { if (ms > 0) node.setTimeout(this.guard(fn), ms); else fn(); }
  ev(tag, type, payload, priority) { const n = this.nodes.get(tag); if (n) this.dispatch(n, type, { target: tag, ...payload }, priority); }

  navCommit(found, after) {
    const nav = this.nav;
    for (const n of found.providers) nav.providers.set(n.reactTag, nav.providers.get(n.reactTag) || '');
    for (const t of [...nav.providers.keys()]) if (!this.parents.has(t)) nav.providers.delete(t);
    after.push(() => found.providers.forEach((n) => this.providerEvent(n.reactTag)));
    for (const n of found.stacks) this.stackCommit(n);
    for (const n of found.tabs) this.tabsCommit(n);
    // iOS attaches the header config to its screen's navigation item once, when the screen is added to the stack.
    for (const n of found.headers) if (!nav.headers.has(n.reactTag)) { nav.headers.add(n.reactTag); after.push(() => this.ev(n.reactTag, 'topAttached', {})); }
  }

  // RNCSafeAreaProvider: topInsetsChange when insets or the provider's frame changed (first time after mount).
  providerEvent(tag) {
    const f = this.frames[tag] || [0, 0, this.window.width, this.window.height];
    const insets = insetsFor(this.window.height), key = JSON.stringify([insets, f]);
    if (this.nav.providers.get(tag) === key) return;
    this.nav.providers.set(tag, key);
    this.ev(tag, 'topInsetsChange', { insets, verticalCornerAdaptedInsets: insets, horizontalCornerAdaptedInsets: insets, frame: { x: f[0], y: f[1], width: f[2], height: f[3] } });
  }

  // RNSScreenStack: the top screen (last RNSScreen child) changing is a push (new top) or pop (old top gone/below).
  stackCommit(stack) {
    const tag = stack.reactTag, all = this.collect(stack, ['RNSScreen', 'RNSModalScreen']).map((s) => s.reactTag);
    let st = this.nav.stacks.get(tag);
    const first = !st;
    if (first) this.nav.stacks.set(tag, (st = { list: [], top: undefined, dismissed: new Set() }));
    // Screens popped natively stay in the tree until JS handles topDismissed; they no longer count.
    for (const t of st.dismissed) if (!all.includes(t)) st.dismissed.delete(t);
    const screens = all.filter((t) => !st.dismissed.has(t));
    const prevList = st.list, from = st.top, to = screens[screens.length - 1];
    st.list = screens;
    if (to === from || to === undefined) { st.top = to; return; }
    st.top = to;
    const forward = !prevList.includes(to), anim = !first && this.nodes.get(to).props.stackAnimation !== 'none';
    this.transition(tag, from, to, forward, anim ? NAV_MS : 0);
  }
  // willDisappear(from) / willAppear(to), then appear / disappear / finishTransitioning when the animation ends.
  transition(stack, from, to, forward, ms, then) {
    const fw = forward ? 1 : 0, prog = (tag, progress, closing) => this.ev(tag, 'topTransitionProgress', { progress, closing, goingForward: fw });
    if (from !== undefined) this.ev(from, 'topWillDisappear', {});
    this.ev(to, 'topWillAppear', {});
    if (ms > 0) { if (from !== undefined) prog(from, 0, 1); prog(to, 0, 0); }
    this.later(ms, () => {
      if (ms > 0) { if (from !== undefined) prog(from, 1, 1); prog(to, 1, 0); }
      if (from !== undefined) this.ev(from, 'topDisappear', {});
      this.ev(to, 'topAppear', {});
      this.ev(stack, 'topFinishTransitioning', {});
      if (then) then();
    });
  }
  // Host -> guest `native_back`: the user popped the top screen with the back button or the swipe-back gesture.
  nativeBack(tag) {
    const st = this.nav.stacks.get(tag);
    if (!st || st.list.length < 2) return log(`OneSim: native_back for ${st ? 'a root screen' : 'unknown stack'} ${tag}`);
    const top = st.list[st.list.length - 1], revealed = st.list[st.list.length - 2], t = this.nodes.get(top);
    if (t && t.props.preventNativeDismiss) return this.ev(top, 'topNativeDismissCancelled', { dismissCount: 1 });
    st.dismissed.add(top); st.list = st.list.slice(0, -1); st.top = revealed;
    this.transition(tag, top, revealed, false, NAV_MS, () => this.ev(top, 'topDismissed', { dismissCount: 1 }));
  }

  // RNSTabsHostIOS: follows JS' navStateRequest {selectedScreenKey, baseProvenance} (echoing it as 'programmatic-js'),
  // and the user's own selection comes in through selectTab.
  tabsCommit(host) {
    const tag = host.reactTag, req = host.props.navStateRequest, keys = this.collect(host, ['RNSTabsScreenIOS']).map((s) => s.props.screenKey);
    let st = this.nav.tabs.get(tag);
    if (!st) this.nav.tabs.set(tag, (st = { key: undefined, provenance: (req && req.baseProvenance) || 0 }));
    if (!req || req.selectedScreenKey === st.key || !keys.includes(req.selectedScreenKey)) return;
    const prev = st.key;
    st.key = req.selectedScreenKey;
    if (prev === undefined) return this.tabLifecycle(host, undefined, st.key);
    st.provenance++;
    this.tabLifecycle(host, prev, st.key);
    this.ev(tag, 'topTabSelected', { selectedScreenKey: st.key, provenance: st.provenance, isRepeated: false, hasTriggeredSpecialEffect: false, actionOrigin: 'programmatic-js' });
  }
  tabLifecycle(host, from, to) {
    const screen = (k) => this.collect(host, ['RNSTabsScreenIOS']).find((s) => s.props.screenKey === k);
    const [a, b] = [screen(from), screen(to)];
    if (a) this.ev(a.reactTag, 'topWillDisappear', {});
    if (b) this.ev(b.reactTag, 'topWillAppear', {});
    if (a) this.ev(a.reactTag, 'topDidDisappear', {});
    if (b) this.ev(b.reactTag, 'topDidAppear', {});
  }
  // Host -> guest `select_tab`: the user tapped a tab bar item. `key` is the screenKey, `index` the position among the tabs.
  selectTab(m) {
    const host = this.nodes.get(m.tag), st = this.nav.tabs.get(m.tag);
    if (!host || !st) return log(`OneSim: select_tab for unknown tab host ${m.tag}`);
    const screens = this.collect(host, ['RNSTabsScreenIOS']), target = m.key != null ? screens.find((s) => s.props.screenKey === m.key) : screens[m.index];
    if (!target) return log(`OneSim: select_tab: no tab ${m.key ?? m.index}`);
    const key = target.props.screenKey, ev = (type, p) => this.ev(m.tag, type, { selectedScreenKey: st.key, provenance: st.provenance, ...p });
    if (target.props.preventNativeSelection && key !== st.key) return ev('topTabSelectionPrevented', { preventedScreenKey: key });
    const prev = st.key;
    if (key !== prev) { st.key = key; this.tabLifecycle(host, prev, key); }
    st.provenance++;
    ev('topTabSelected', { selectedScreenKey: key, provenance: st.provenance, isRepeated: key === prev, hasTriggeredSpecialEffect: false, actionOrigin: 'user' });
  }

  dispatch(node, type, nativeEvent, priority = 32) {
    if (!this.eventHandler || this.dead) return;
    this.eventPriority = priority;
    this.worklets.onEvent(node.reactTag, type, nativeEvent);
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
      // The host's scroll view asks who owns the touch, after the touches it already sent (a PanResponder grant
      // above has sent js_responder by now).
      case 'touch_ask': send({ type: 'touch_answer', ask: m.ask, captured: this.gestures.captured() }); break;
      case 'touch': {
        // A touch the system cancelled (the app resigned active) gets nothing more until the next finger.
        if (m.phase === 'start') this.touchGone = false;
        else if (this.touchGone) break;
        this.liveTouch = m.phase === 'start' || m.phase === 'move' ? m : null;
        const n = this.nodes.get(m.tag);
        if (!n) return log(`OneSim: touch for unknown tag ${m.tag}`);
        const ts = m.timestamp ?? Date.now();
        if (this.gestures.onTouch(m)) break; // a gesture handler owns the touch (RN's responder got topTouchCancel)
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
          if (t?.viewName === 'RCTSwitch' && !t.props.disabled) this.dispatch(t, 'topChange', { value: !t.props.value, target: m.tag }, 2);
          if (t?.viewName === 'AndroidSwitch' && t.props.enabled !== false) this.dispatch(t, 'topChange', { value: !t.props.on, target: m.tag }, 2);
        }
        break;
      }
      case 'simulate': if (this.registry) this.registry.simulate(m.module, m.state, m.replay); else this.simQueue.push(m); break; // before boot: applied once the registry exists
      case 'action': { // a button of an alert / action sheet / share sheet: drop that layer, then run its handler
        const n = this.parents.has(m.id) && this.nodes.get(m.id);
        if (n && expoUI.viewName(n.viewName) && expoUI.action(n, cfg.platform, (event, payload) => this.dispatch(n, event, payload, 2))) break;
        const i = this.alerts.findIndex((a) => a.run.has(m.id));
        if (i < 0) return log(`OneSim: action ${m.id} matches no alert`);
        const [a] = this.alerts.splice(i, 1);
        this.scheduleEmit();
        this.guard(a.run.get(m.id))();
        break;
      }
      case 'open_url': if (this.registry) this.registry.openURL(m.url); break;
      case 'response': {
        const r = this.reqs.get(m.id);
        if (!r) break;
        this.reqs.delete(m.id);
        if (m.error != null) r.rej(new Error(m.error)); else r.res(m.result);
        break;
      }
      case 'scene_phase': this.setAppState(m.phase); break;
      case 'native_back': this.nativeBack(m.tag); break;
      case 'select_tab': this.selectTab(m); break;
      case 'focus': this.focus(m.tag); break;
      case 'blur': this.blur(m.tag); break;
      case 'keyboard': this.keyboard(m.height); break;
      case 'text_input': { // `version` is ignored: the guest owns the text
        const n = this.nodes.get(m.field);
        if (n && expoUI.isField(n.viewName)) {
          const max = n.props.maxLength;
          const text = Number.isInteger(max) && max >= 0 ? m.text.slice(0, max) : m.text;
          this.expoFieldState(n)?.setValue({ value: text });
          this.input(n).text = text;
          const selection = { start: text.length, end: text.length };
          this.expoUIState?.get(n.props.selection)?.setValue({ value: selection });
          if (cfg.platform === 'android') this.dispatch(n, 'topValueChange', { text, selection }, 2);
          else this.dispatch(n, 'topTextChange', { value: text }, 2);
          this.dispatch(n, 'topSelectionChange', selection, 2);
          this.scheduleEmit();
          break;
        }
        if (n && expoUI.isSlider(n.viewName)) {
          const { min = 0, max = 1, steps, step } = n.props;
          const increment = step ?? (steps > 0 ? (max - min) / (steps + 1) : 0);
          const next = Number(m.text);
          const value = Math.max(min, Math.min(max, increment > 0 ? min + Math.round((next - min) / increment) * increment : next));
          if (cfg.platform === 'android') this.dispatch(n, 'topValueChange', { value }, 2);
          else this.dispatch(n, 'topValueChanged', { value, eventCount: ++this.input(n).eventCount }, 2);
          break;
        }
        if (!n || !TEXT_INPUTS.has(n.viewName)) return log(`OneSim: text_input for unknown field ${m.field}`);
        const st = this.input(n), text = String(m.text ?? '');
        st.text = text; st.eventCount++;
        this.dispatch(n, 'topChange', { text, eventCount: st.eventCount, target: m.field }, 2);
        this.dispatch(n, 'topSelectionChange', { selection: { start: text.length, end: text.length }, target: m.field }, 2);
        this.scheduleEmit();
        break;
      }
      case 'submit': {
        const n = this.nodes.get(m.tag);
        if (n && expoUI.isField(n.viewName)) {
          if (cfg.platform === 'android') {
            const type = n.props.keyboardOptions?.imeAction;
            this.dispatch(n, 'topKeyboardAction', { action: type && type !== 'default' ? type : 'done', value: this.expoFieldText(n) }, 2);
          } else {
            this.dispatch(n, 'topGlobalEvent', { onSubmit: {} }, 2);
          }
          break;
        }
        if (!n || !TEXT_INPUTS.has(n.viewName)) return;
        const st = this.input(n);
        this.dispatch(n, 'topSubmitEditing', { text: st.text, eventCount: st.eventCount, target: m.tag }, 2);
        const single = n.viewName !== 'RCTMultilineTextInputView' && !n.props.multiline;
        if (n.props.submitBehavior === 'blurAndSubmit' || (single && !n.props.submitBehavior)) {
          this.blur(m.tag); send({ type: 'focus_request', tag: null });
        }
        break;
      }
      case 'scroll': {
        const n = this.nodes.get(m.tag);
        if (!n) return;
        const last = this.scrolled.get(m.tag), cur = [m.x, m.y];
        this.scrolled.set(m.tag, cur);
        const ev = { contentOffset: { x: m.x, y: m.y }, contentSize: { width: m.contentWidth, height: m.contentHeight }, layoutMeasurement: { width: m.width, height: m.height }, contentInset: { top: 0, left: 0, bottom: 0, right: 0 }, zoomScale: 1, target: m.tag };
        if (m.phase === 'begin') this.dispatch(n, 'topScrollBeginDrag', ev, 8);
        else if (m.phase === 'scroll') {
          this.dispatch(n, 'topScroll', ev, 8);
          // Overscroll past the top beyond UIRefreshControl's trigger distance starts a refresh.
          const rc = m.y <= -REFRESH_PULL && (n.children.map((c) => this.nodes.get(c.reactTag) || c).find((c) => c.viewName === 'PullToRefreshView')
            || this.nodes.get(this.parents.get(n.reactTag)));
          if (rc && ['PullToRefreshView', 'AndroidSwipeRefreshLayout'].includes(rc.viewName) && !rc.props.refreshing && rc.props.enabled !== false) {
            this.dispatch(rc, 'topRefresh', {}, 2);
          }
        }
        else if (m.phase === 'end' || m.phase === 'release') {
          if (!last || last[0] !== m.x || last[1] !== m.y) this.dispatch(n, 'topScroll', ev, 8); // programmatic scrollTo reply
          this.dispatch(n, 'topScrollEndDrag', ev, 8);
          this.dispatch(n, m.phase === 'end' ? 'topMomentumScrollEnd' : 'topMomentumScrollBegin', ev, 8);
        }
        else if (m.phase === 'momentum_end') this.dispatch(n, 'topMomentumScrollEnd', ev, 8);
        break;
      }
      case 'layout': Object.assign(this.frames, m.frames); this.nav.providers.forEach((_, t) => this.providerEvent(t)); this.fireLayouts(); break;
      case 'environment': {
        if (m.appearance && m.appearance !== this.appearance) { this.appearance = m.appearance; this.emit('appearanceChanged', { colorScheme: this.appearance }); }
        if (m.viewport) { this.window = { width: m.viewport.width, height: m.viewport.height }; this.emit('didUpdateDimensions', this.dimensions()); }
        break;
      }
    }
  }
  // AppState (UIApplication.State): host `scene_phase`, and the round trip through another app on a hand-off. iOS cancels
  // the app's touches as it resigns active. A tree follows every change, so the host knows the guest has it.
  setAppState(state) {
    if (!['active', 'inactive', 'background'].includes(state) || state === this.appState) return;
    this.appState = state;
    if (state !== 'active' && this.liveTouch) { const t = this.liveTouch; this.onMessage({ ...t, phase: 'cancel' }); this.touchGone = true; }
    this.emit('appStateDidChange', { app_state: state });
    this.scheduleEmit();
  }
  // Work only the host can do (image hashes, text rendering): `request` -> `response` (protocol section H).
  request(method, params) {
    const id = ++this.reqSeq;
    return new Promise((res, rej) => { this.reqs.set(id, { res, rej }); send({ type: 'request', id, method, params }); });
  }
  input(n) {
    let st = this.inputs.get(n.reactTag);
    if (!st) this.inputs.set(n.reactTag, (st = { text: String(n.props.text ?? n.props.value ?? n.props.defaultValue ?? ''), eventCount: 0 }));
    return st;
  }
  requestFocus(tag) { send({ type: 'focus_request', tag }); }
  expoFieldState(n) { return this.expoUIState?.get(cfg.platform === 'android' ? n.props.value : n.props.text); }
  expoFieldText(n) { return String(this.expoFieldState(n)?.getValue() ?? this.inputs.get(n.reactTag)?.text ?? ''); }
  focus(tag) {
    const n = this.nodes.get(tag);
    if (!n || this.focused === tag) return;
    if (this.focused != null) this.blur(this.focused);
    this.focused = tag;
    if (expoUI.isField(n.viewName)) {
      this.dispatch(n, cfg.platform === 'android' ? 'topFocusChanged' : 'topFocusChange', { value: true }, 2);
    } else this.dispatch(n, 'topFocus', { target: tag }, 2);
  }
  blur(tag) {
    const n = this.nodes.get(tag);
    if (!n || this.focused !== tag) return;
    this.focused = null;
    if (expoUI.isField(n.viewName)) {
      this.dispatch(n, cfg.platform === 'android' ? 'topFocusChanged' : 'topFocusChange', { value: false }, 2);
      return;
    }
    this.dispatch(n, 'topEndEditing', { text: this.input(n).text, target: tag }, 2);
    this.dispatch(n, 'topBlur', { target: tag }, 2);
  }
  // KeyboardObserver: notifications for keyboard focus (height > 0) or dismissal (0).
  // duration 0: OneSim renders settled states, so KeyboardAvoidingView jumps to its final layout (no LayoutAnimation).
  keyboard(height) {
    const { width, height: H } = this.window, from = this.kbFrame;
    const to = height > 0 ? { screenX: 0, screenY: H - height, width, height } : null;
    const off = { screenX: 0, screenY: H, width, height: (to || from || {}).height || 0 };
    if (!to === !from) return;
    this.kbFrame = to;
    const ev = { startCoordinates: from || off, endCoordinates: to || off, duration: 0, easing: 'keyboard', isEventFromThisApp: true };
    const what = to ? 'Show' : 'Hide';
    const events = cfg.platform === 'android'
      ? [`keyboardDid${what}`]
      : ['keyboardWillChangeFrame', `keyboardWill${what}`, 'keyboardDidChangeFrame', `keyboardDid${what}`];
    for (const n of events) this.emit(n, ev);
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
    } else if (n.viewName === 'AndroidSwitch' && name === 'setNativeValue') {
      this.fabric.setNativeProps(n, { on: args[0] });
      return;
    } else if (SCROLL_VIEWS.has(n.viewName)) {
      if (name === 'scrollTo') return send({ type: 'scroll_to', tag, x: args[0] || 0, y: args[1] || 0 });
      if (name === 'scrollToEnd') return send({ type: 'scroll_to', tag, x: 'end', y: 'end' });
      if (name === 'flashScrollIndicators' || name === 'zoomToRect') return;
    }
    unsupported('FabricUIManager', `dispatchCommand:${name} on ${n.viewName}`);
  }

  // Image: download once per source (shared with ImageLoader), expose the cached file to the host. expo-image adds image
  // refs and hash sources (images.js), `cacheKey`, a `placeholder` drawn until the source is in, and its view functions.
  imageProps(n, props, after) {
    const tag = n.reactTag, expo = n.viewName === EXPO_IMAGE, lib = this.registry.images, v = this.imageViews.get(tag) || {};
    if (expo && v.animating !== undefined) props.__simcoreAnimating = v.animating; // startAnimating / stopAnimating
    const src = lib.resolve([].concat(props.source ?? [])[0]), uri = src && src.uri;
    if (!uri && !expo) return;
    // No source (`source={null}` while data loads): expo-image still draws its placeholder.
    const key = !uri ? null : src.ref ? `ref:${src.ref.__expo_shared_object_id__}` : src.cacheKey || uri;
    let st = this.images.get(tag);
    if (!uri) {
      if (!st || (st.key !== null && !(v.locked && st.info))) this.images.set(tag, (st = { uri: null, key: null }));
    } else if (!st || (st.key !== key && !(v.locked && st.info))) {
      const mine = (st = { uri, key });
      this.images.set(tag, st);
      const done = (type, ev) => { if (this.images.get(tag) !== mine) return; this.dispatch(this.nodes.get(tag) || n, type, { target: tag, ...ev }); };
      after.push(() => {
        done('topLoadStart', {});
        const loaded = src.ref ? Promise.resolve(src.ref.__simcore.info) : loadImage(uri, src.headers, { key: src.cacheKey, width: src.width, height: src.height });
        loaded.then((info) => {
          mine.info = info;
          const s = src.scale || 1;
          if (expo) { // expo-image's events: pixel size, cacheType, then onDisplay once drawn
            done('topLoad', { cacheType: src.ref ? 'memory' : 'none', source: { url: uri, width: info.width, height: info.height, mediaType: lib.media(info), isAnimated: !!info.animated } });
            done('topDisplay', {});
          } else { done('topLoad', { source: { uri, width: info.width / s, height: info.height / s } }); done('topLoadEnd', {}); }
          if (this.images.get(tag) === mine) this.scheduleEmit();
        }, (e) => { mine.err = e; done('topError', { error: String(e && e.message || e) }); if (!expo) done('topLoadEnd', {}); if (this.images.get(tag) === mine) this.scheduleEmit(); });
      });
    }
    if (st.info) { props.__simcoreImage = st.info; return; }
    // expo-image `placeholder` (an image, a blurhash or a thumbhash): drawn with `placeholderContentFit` until the
    // source is in, and kept when it fails.
    const ph = expo && lib.resolve([].concat(props.placeholder ?? [])[0]);
    if (!ph || !ph.uri) return;
    if (!st.ph || st.ph.uri !== ph.uri) {
      const mine = (st.ph = { uri: ph.uri });
      const p = ph.ref ? Promise.resolve(ph.ref.__simcore.info) : loadImage(ph.uri, ph.headers, { key: ph.cacheKey, width: ph.width, height: ph.height });
      p.then((info) => { mine.info = info; this.scheduleEmit(); }, () => {});
    }
    if (st.ph.info) { props.__simcoreImage = st.ph.info; props.contentFit = props.placeholderContentFit || 'scale-down'; }
  }

  // Tab bar icons are image sources (Metro asset URLs): download them like images and hand the host the cached file.
  tabIcons(n, props) {
    for (const [src, out] of [['iconImageSource', '__simcoreIconImage'], ['selectedIconImageSource', '__simcoreSelectedIconImage']]) {
      const uri = props[src] && props[src].uri;
      if (!uri) continue;
      const done = imageDone.get(uri);
      if (done) props[out] = done;
      else loadImage(uri).then(() => this.scheduleEmit(), (e) => sendError(`tab icon ${uri}: ${e && e.message || e}`));
    }
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
    log('OneSim: reload:', reason || '');
    send({ type: '__reload', reason: String(reason || '') });
  }

  dispose() {
    this.dead = true;
  }

  // ---- native modules: the registry in modules.js (TurboModules, expo modules, simulated state) ----
  makeRegistry(bundleSource) {
    const ver = /ReactNativeVersion\.major = (\d+);\s*ReactNativeVersion\.minor = (\d+);\s*ReactNativeVersion\.patch = (\d+)/.exec(bundleSource) || [0, 0, 0, 0];
    this.registry = require('./modules.js')(this, { node, log, sendError, sendEvent, unsupported, cfg, ver, insetsFor, loadImage, forgetImage, imageDone, readUri, expo: this.expoClasses() });
    for (const m of this.simQueue.splice(0)) this.registry.simulate(m.module, m.state, m.replay);
    return this.registry;
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
        const boundary = `----OneSimFormBoundary${node.randomUUID().replace(/-/g, '')}`, chunks = [];
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
    const start = (id, q, onId) => {
      const flags = {};
      live.set(id, flags);
      if (q.timeout > 0) flags.timer = node.setTimeout(() => { flags.timedOut = true; flags.abort && flags.abort(); }, q.timeout);
      onId?.(id);
      node.setImmediate(this.guard(() => run(id, q, flags)));
    };
    return {
      addListener: () => {}, removeListeners: () => {},
      sendRequest: cfg.platform === 'android'
        ? (method, url, id, headers, data, responseType, incrementalUpdates, timeout) =>
          start(id, { method, url, headers: Object.fromEntries(headers), data, responseType, incrementalUpdates, timeout })
        : (q, cb) => start(++next, q, cb),
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
  expoClasses() {
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
    return (this.classes = { EventEmitter, SharedObject, SharedRef, NativeModule });
  }
  makeExpo() {
    const self = this;
    return {
      ...this.classes, modules: this.registry.expoModules,
      // ExpoModulesCore reads these; "bare" matches an app without native expo modules linked.
      getViewConfig: (mod, view) => expoViewConfig(view ? `${mod}_${view}` : mod), reloadAppAsync: async () => self.reload('expo reloadAppAsync'),
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
      // Main-thread HTTP like every other request: a keep-alive socket left in this worker's undici fires its idle
      // timer after RN replaced setTimeout (`this.timeout?.unref is not a function`), e.g. a static server for `expo export`.
      const r = await nfetch(bundleUrl);
      src = (await r.done).toString('utf8');
      if (r.status < 200 || r.status > 299) throw new Error(`HTTP ${r.status}: ${src.slice(0, 500)}`);
      // Animated.flushQueue must not throw without NativeAnimatedModule (dev and minified bundles alike): bundle-patches.js.
      const p = patchAnimated(src);
      src = p.src;
      if (p.hasAnimated && !p.flushQueue) log('warning: Animated.flushQueue patch found no match; Animated views may fail to mount');
    } catch (e) { sendEvent('bundle.error', { message: e.message, url: bundleUrl }); return sendError(`bundle fetch failed (${bundleUrl}): ${e.message}`); }
    if (this.dead) return;

    const modules = this.makeRegistry(src).turbo;
    send({ type: 'modules', modules: this.registry.list() });
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
      // Native views the OneSim host knows (react-native-screens, safe-area-context, expo views); RN's own optional views
      // (DebuggingOverlay, VirtualView, ...) stay absent so RN falls back. Legacy view managers show up in UIManager constants.
      __nativeComponentRegistry__hasComponent: (n) => isNativeView(n),
      RN$LegacyInterop_UIManager_getConstants: () => Object.fromEntries([...NATIVE_VIEWS].map((n) => [n, {}])),
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
      this.watchNavigation();
    } catch (e) { this.onUncaught(e); } finally {
      if (env.SIMCORE_DEBUG_EVAL) log('DEBUG', String(vm.runInThisContext(env.SIMCORE_DEBUG_EVAL)));
    }
  }

  // `navigation.changed`: Expo Router keeps one React Navigation container ref (`global-state/navigationRef`); no app change needed.
  // Metro (dev) lets us reach its modules by path. route = pathname ("/item/Apple"), stack = pathnames of the focused stack's history.
  // ponytail: Expo Router only; a bare React Navigation app has no global ref (add a tree-based fallback if needed).
  watchNavigation() {
    try {
      const find = (suffix) => { for (const [id, m] of globalThis.__r.getModules()) if ((m.verboseName || '').endsWith(suffix)) return globalThis.__r(id); };
      const ref = find('expo-router/build/global-state/navigationRef.js')?.navigationRef, info = find('expo-router/build/global-state/getRouteInfoFromState.js')?.getRouteInfoFromState;
      if (!ref || !info) return;
      let last = '';
      const changed = this.guard(() => {
        const root = ref.isReady() && ref.getRootState();
        if (!root) return;
        const chain = [];
        for (let s = root; s; ) { chain.push(s); const r = s.routes[s.index ?? s.routes.length - 1]; s = r && r.state; }
        const withRoutes = (level, routes) => { // the root state with chain[level]'s history cut to `routes`
          let s = { ...chain[level], routes, index: routes.length - 1 };
          for (let k = level - 1; k >= 0; k--) { const p = chain[k]; s = { ...p, routes: p.routes.map((r, j) => (j === p.index ? { ...r, state: s } : r)) }; }
          return s;
        };
        const now = info(root), level = chain.map((s) => s.type).lastIndexOf('stack');
        const stack = level < 0 ? [now.pathname] : chain[level].routes.map((_, i) => info(withRoutes(level, chain[level].routes.slice(0, i + 1))).pathname);
        const detail = { route: now.pathname, params: now.params, stack }, key = JSON.stringify(detail);
        if (key !== last) { last = key; this.navSeen = true; sendEvent('navigation.changed', detail); }
      });
      ref.addListener('state', changed);
      this.navCheck = changed; // the container's first 'state' event can precede our listener: poll after commits until one event is out
    } catch (e) { if (env.SIMCORE_DEBUG) log('OneSim: navigation watch failed:', e && e.stack); }
  }

  setupHmr() {
    const u = new URL(this.scriptURL);
    const hmr = this.getCallable('HMRClient');
    const entry = (u.pathname.match(/([^/]+)\.bundle/) || [0, 'index'])[1];
    if (hmr) this.guard(() => hmr.setup(cfg.platform, entry, u.hostname, Number(u.port) || (u.protocol === 'https:' ? 443 : 80), true, u.protocol.replace(':', '')))();
  }
}

// Manifest first (what a real Expo client does), then plain Metro bundle URL.
async function resolveBundleUrl() {
  if (env.SIMCORE_BUNDLE_URL) return { url: env.SIMCORE_BUNDLE_URL, manifest: null }; // prebuilt bundle (expo export): no Metro
  const o = new URL(cfg.metro);
  try {
    const r = await node.fetch(cfg.metro + '/', { headers: { 'expo-platform': cfg.platform, accept: 'application/expo+json,application/json' } });
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
  return { url: `${cfg.metro}/index.bundle?platform=${cfg.platform}&dev=true&minify=false`, manifest: null };
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
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), 'onesim-session-'));
  process.on('exit', () => fs.rmSync(dataDir, { recursive: true, force: true }));
  const simulated = new Map(); // module -> merged `simulate` state, replayed into every new worker (state survives reloads)
  const out = (m) => process.stdout.write(JSON.stringify(m) + '\n');
  const start = () => {
    if (worker) worker.terminate();
    out({ type: 'hello', protocol: 1 }); // every (re)start: the host drops what it told the previous worker (layout)
    base = last; // tree versions stay monotonic across reloads
    const w = (worker = new Worker(__filename, { argv: process.argv.slice(2), stdout: false, stderr: false, workerData: { generation: ++generation, dataDir } }));
    for (const [module, state] of simulated) w.postMessage({ type: 'simulate', module, state, replay: true });
    const sockets = new Map(), fetching = new Map();
    w.on('exit', () => { sockets.forEach((ws) => ws.close()); fetching.forEach((ac) => ac.abort()); fetching.clear(); });
    w.on('message', (m) => {
      if (m.type === '__treeJson') {
        if (w !== worker) return; // a worker that was already replaced
        const v = m.version + base;
        last = v;
        return process.stdout.write(`{"type":"tree","version":${v},"root":${m.rootJson}}\n`);
      }
      if (m.type === 'tree') { m.version += base; last = m.version; }
      if (w !== worker) return; // a worker that was already replaced
      if (m.type === '__reload') return start();
      if (m.type === '__fetch') return mainFetch(w, m, fetching);
      if (m.type === '__fetchAbort') return fetching.get(m.id)?.abort();
      if (m.type !== '__ws') return out(m);
      const emit = (event, payload) => w.postMessage({ type: '__wsEvent', event, payload });
      if (m.op === 'connect') {
        if (env.SIMCORE_DEBUG) process.stderr.write(`OneSim: ws connect ${m.url}\n`);
        const ws = new WebSocket(m.url, m.protocols || undefined);
        ws.binaryType = 'arraybuffer';
        sockets.set(m.id, ws);
        ws.onopen = () => { if (env.SIMCORE_DEBUG) process.stderr.write('OneSim: ws open\n'); emit('websocketOpen', { id: m.id }); };
        ws.onmessage = (ev) => emit('websocketMessage', { id: m.id, type: typeof ev.data === 'string' ? 'text' : 'binary', data: typeof ev.data === 'string' ? ev.data : Buffer.from(ev.data).toString('base64') });
        ws.onerror = () => emit('websocketFailed', { id: m.id, message: 'WebSocket error' });
        ws.onclose = (ev) => { sockets.delete(m.id); emit('websocketClosed', { id: m.id, code: ev.code, reason: ev.reason }); };
      } else if (m.op === 'send') sockets.get(m.id)?.send(m.data);
      else if (m.op === 'close') sockets.get(m.id)?.close(m.code, m.reason);
    });
    w.on('error', (e) => out({ type: 'error', message: String(e && e.stack || e) }));
  };
  const rl = readline.createInterface({ input: process.stdin });
  rl.on('line', (line) => {
    let m; try { m = JSON.parse(line); } catch (_) { return; }
    if (m.type === 'shutdown') process.exit(0);
    else if (m.type === 'reload') start();
    else if (m.type === 'simulate') { simulated.set(m.module, { ...simulated.get(m.module), ...m.state }); if (worker) worker.postMessage(m); }
    else if (worker) worker.postMessage(m);
  });
  rl.on('close', () => process.exit(0));
  start();
}
