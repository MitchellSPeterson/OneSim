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
const { Worker, isMainThread, parentPort } = require('node:worker_threads');

Error.stackTraceLimit = 60;
const ROOT_TAG = 1;
const env = process.env; // `process` is removed from the guest global before the bundle runs
if (isMainThread) return mainThread();

// Node intrinsics captured before RN's polyfills replace the globals of the same name.
const node = {
  Buffer, setTimeout, clearTimeout, setInterval, clearInterval, setImmediate, queueMicrotask, performance, fetch,
  inspect: require('node:util').inspect, randomUUID: require('node:crypto').randomUUID,
};
const send = (m) => parentPort.postMessage(m);
const log = (...a) => fs.writeSync(2, a.join(' ') + '\n');
const arg = (n) => { const i = process.argv.indexOf(n); return i > 0 ? process.argv[i + 1] : undefined; };

const cfg = {
  metro: (arg('--url') || process.env.SIMCORE_METRO_URL || 'http://localhost:8081').replace(/\/+$/, ''),
  appearance: process.env.SIMCORE_APPEARANCE === 'dark' ? 'dark' : 'light',
  scale: Number(process.env.SIMCORE_SCALE) || 3,
  session: process.env.SIMCORE_SESSION_ID || '',
  hmr: process.env.SIMCORE_HMR === '1',
};
{
  const m = /^(\d+(?:\.\d+)?)x(\d+(?:\.\d+)?)$/.exec(process.env.SIMCORE_VIEWPORT || '393x852');
  cfg.width = m ? +m[1] : 393; cfg.height = m ? +m[2] : 852;
}

const sendError = (message) => { log('SimCore error:', message); send({ type: 'error', message: String(message) }); };
const reportedModules = new Set();
function unsupported(mod, method) {
  const msg = `SimCore: ${mod} is not supported (method: ${method})`;
  log(msg);
  if (env.SIMCORE_DEBUG) log(new Error().stack);
  if (!reportedModules.has(mod)) { reportedModules.add(mod); send({ type: 'error', message: msg }); }
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
      setNativeProps: (n, p) => { n.props = merge(n.props, p); self.scheduleEmit(); },
      dispatchCommand: (n, name) => unsupported('FabricUIManager', `dispatchCommand:${name} on ${n.viewName}`),
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
      const ser = (n, parent) => {
        this.parents.set(n.reactTag, parent);
        const props = {};
        for (const [k, v] of Object.entries(n.props)) {
          if (typeof v === 'function' || v === undefined) continue;
          props[k] = v;
        }
        return { kind: 'rn', id: String(n.reactTag), component: n.viewName, props, children: n.children.map((c) => ser(c, n.reactTag)) };
      };
      const root = { kind: 'rn', id: String(ROOT_TAG), component: 'RootView', props: {}, children: this.rootChildren.map((c) => ser(c, ROOT_TAG)) };
      if (env.SIMCORE_DEBUG) log('emit tree', version + 1);
      send({ type: 'tree', version: ++version, root });
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
      case '__wsEvent': this.emit(m.event, m.payload); break;
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
      Networking: () => ({ ...listener, sendRequest: () => unsupported('Networking', 'sendRequest'), abortRequest: noop }),
      LogBox: () => ({ show: noop, hide: noop }),
      DevSettings: () => ({ reload: noop, ...listener, setHotLoadingEnabled: noop, setIsDebuggingRemotely: noop, toggleElementInspector: noop, addMenuItem: noop, openDebugger: noop, setProfilingEnabled: noop }),
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
      ImageLoader: () => ({ getSize: () => unsupported('ImageLoader', 'getSize'), getSizeWithHeaders: () => unsupported('ImageLoader', 'getSizeWithHeaders'), prefetchImage: () => { unsupported('ImageLoader', 'prefetchImage'); return Promise.resolve(false); }, queryCache: () => Promise.resolve({}), abortRequest: noop }),
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
    const absent = new Set(['NativeAnimatedModule', 'NativeAnimatedTurboModule', 'BlobModule', 'NativeCPUTime', 'ReactDevToolsRuntimeSettingsModule', 'NativeReactDevToolsSettingsManager', 'NativeAnimatedModule', 'NativeIdleCallbacksCxx', 'NativeIdleCallbacks', 'JSCExecutor', 'IntentAndroid', 'SoundManager', 'Performance', 'HeadlessJsTaskSupport', 'PerformanceObserver', 'IntersectionObserver', 'MutationObserver', 'ResizeObserver', 'FrameRateLogger', 'UIManager', 'I18nManagerAndroid']);
    // Real iOS RN modules with platform behaviour we do not implement: every call is a loud diagnostic.
    // Anything not listed anywhere is simply absent (null), so getEnforcing throws and the error is reported.
    const known = new Set(['ActionSheetManager', 'AlertManager', 'Clipboard', 'ShareModule', 'PushNotificationManager', 'ImageStoreManager', 'FileReaderModule', 'AccessibilityManager', 'DevToolsSettingsManager', 'Vibration', 'ImageEditor']);
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
      ExpoFetchModule: () => ({ NativeResponse: diagClass('ExpoFetchModule', 'NativeResponse'), NativeRequest: diagClass('ExpoFetchModule', 'NativeRequest') }),
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
      getViewConfig: () => null, reloadAppAsync: async () => self.reload && self.reload(),
      uuidv4: () => node.randomUUID(), uuidv5: (n, ns) => ns,
      expoModulesCoreVersion: { version: '0.0.0', major: 0, minor: 0, patch: 0 },
      DeviceEventEmitter: undefined, // set below once RN's emitter exists
    };
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
    } catch (e) { return sendError(`bundle fetch failed (${bundleUrl}): ${e.message}`); }
    if (this.dead) return;

    const modules = this.makeModules(src);
    const t = this.makeTimers();
    const sandbox = {
      ...t,
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
      if (!cfg.hmr) log('SimCore: HMR client not started (set SIMCORE_HMR=1 to call HMRClient.setup)');
      const keys = globalThis.RN$AppRegistry.getAppKeys().filter((k) => k !== 'LogBox');
      const key = keys.includes('main') ? 'main' : keys[0];
      if (!key) return sendError('no component registered with AppRegistry');
      const reg = this.getCallable('AppRegistry');
      this.guard(() => reg.runApplication(key, { rootTag: ROOT_TAG, initialProps: {}, fabric: true }))();
      if (cfg.hmr) this.setupHmr();
    } catch (e) { this.onUncaught(e); } finally {
      if (env.SIMCORE_DEBUG_EVAL) log('DEBUG', String(vm.runInThisContext(env.SIMCORE_DEBUG_EVAL)));
    }
  }

  setupHmr() {
    const u = new URL(this.scriptURL);
    const hmr = this.getCallable('HMRClient');
    if (hmr) this.guard(() => hmr.setup('ios', 'index', u.hostname, Number(u.port) || 80, true, u.protocol.replace(':', '')))();
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

// Main thread: protocol I/O + worker supervisor.
function mainThread() {
  let worker = null, base = 0, last = 0;
  const out = (m) => process.stdout.write(JSON.stringify(m) + '\n');
  const start = () => {
    if (worker) worker.terminate();
    base = last; // tree versions stay monotonic across reloads
    const w = (worker = new Worker(__filename, { argv: process.argv.slice(2), stdout: false, stderr: false }));
    const sockets = new Map();
    w.on('exit', () => sockets.forEach((ws) => ws.close()));
    w.on('message', (m) => {
      if (m.type === 'tree') { m.version += base; last = m.version; }
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
