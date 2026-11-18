'use strict';
// react-native-worklets + react-native-reanimated: the native side, for the SimCore guest.
//
// Real device: two JS runtimes (React Native, UI) on two threads; "Serializables" are immutable C++ trees that are
// materialised into whichever runtime reads them; the UI runtime is driven by the display link.
// Here: the same design on one thread. The RN runtime is the worker global (where the Metro bundle runs); the UI
// runtime is a second `vm` realm with its own globals (`_WORKLET`, requestAnimationFrame, console, the Reanimated
// natives...). Serializables are `Ref` trees that `des()` rebuilds in the target realm, so worklets are eval'd by the
// library's own unpackers exactly as on Hermes, closures are copies, and nothing leaks between realms.
//
// Reanimated's native side (what its C++ ReanimatedModuleProxy does) is the second half: animated props become an
// overlay merged into the emitted tree (PropsRegistry), `entering` layout animations are started when their view is
// first committed, and native events reach `useEvent` handlers on the UI realm.
//
// Limits (diagnosed, never faked): worker runtimes (createWorkletRuntime), sensors, CSS animations/transitions,
// exiting / layout / shared-element animations, keyboard events, bundle mode.
const vm = require('node:vm');
const util = require('node:util');

const RN = 1, UI = 2; // runtime ids == RuntimeKind
const STATIC_FLAGS = { // package defaults (staticFlags.json); cross-runtime stack traces off: capturing them is costly and the stacks are RN-only
  RUNTIME_TEST_FLAG: false, IOS_DYNAMIC_FRAMERATE_ENABLED: true, ENABLE_CROSS_RUNTIME_STACK_TRACES: false,
  DISABLE_COMMIT_PAUSING_MECHANISM: false, ANDROID_SYNCHRONOUSLY_UPDATE_UI_PROPS: false, IOS_SYNCHRONOUSLY_UPDATE_UI_PROPS: false,
  EXPERIMENTAL_CSS_ANIMATIONS_FOR_SVG_COMPONENTS: true, IOS_CSS_CORE_ANIMATION: false, ANDROID_CSS_PLATFORM_TRANSITIONS: false,
  USE_COMMIT_HOOK_ONLY_FOR_REACT_COMMITS: true, ENABLE_SHARED_ELEMENT_TRANSITIONS: false, USE_LEGACY_LAYOUT_ANIMATIONS_PROXY: false,
  FORCE_REACT_RENDER_FOR_SETTLED_ANIMATIONS: true, USE_ANIMATION_BACKEND: false,
};
const FRAME_MS = 1000 / 60;

// A Serializable. `__serializableRef` is the marker the JS side looks for.
// t: s scalar | o object | a array | h host object | f remote function | w worklet | m map | st set | e error | rx regexp
//    ab array buffer | abv array buffer view | c custom | sh shareable | sy synchronizable
class Ref {
  constructor(t, v, x) { this.t = t; this.v = v; this.x = x; }
  get __serializableRef() { return true; }
}

module.exports = function createWorklets(rt, { node, log, sendError, unsupported }) {
  const rn = { id: RN, name: 'ReactNative', get g() { return globalThis; } };
  const ctx = vm.createContext({});
  const ui = { id: UI, name: 'UI', ctx, g: vm.runInContext('globalThis', ctx) };
  const realms = { [RN]: rn, [UI]: ui };
  const remoteFns = new WeakMap(); // guard function made in a realm -> the original function
  const customTypes = [];
  let uiQueue = [], draining = false;

  // ---- Serializables ----
  const scalar = (v) => new Ref('s', v);
  const U = scalar(undefined), N = scalar(null), T = scalar(true), F = scalar(false);
  // Any JS value -> Ref (the C++ makeSerializableClone, for values the JS side did not already flatten).
  function from(v) {
    if (v instanceof Ref) return v;
    if (v === undefined) return U;
    if (v === null) return N;
    if (typeof v !== 'object' && typeof v !== 'function') return scalar(typeof v === 'symbol' ? String(v) : v);
    if (v.__serializableRef === true && v.t) return v;
    if (v.__workletHash !== undefined) return worklet(v);
    if (typeof v === 'function') return new Ref('f', v, v.name);
    if (Array.isArray(v)) return new Ref('a', v.map(from));
    return obj(v);
  }
  const obj = (o, persist) => { const m = {}; for (const k of Object.keys(o)) m[k] = from(o[k]); const r = new Ref('o', m); r.persist = persist; return r; };
  const worklet = (o) => new Ref('w', obj(o).v);

  const cached = (r, realm, make) => { // persisted refs (worklet __initData) are materialised once per realm
    if (!r.persist) return make();
    r.cache ??= new Map();
    if (!r.cache.has(realm.id)) r.cache.set(realm.id, make());
    return r.cache.get(realm.id);
  };
  // Ref -> value in `realm` (the C++ Serializable::toJSValue).
  function des(r, realm) {
    const g = realm.g;
    switch (r.t) {
      case 's': return r.v;
      case 'o': return cached(r, realm, () => { const o = new g.Object(); for (const k in r.v) o[k] = des(r.v[k], realm); return o; });
      case 'a': { const a = new g.Array(); for (const e of r.v) a.push(des(e, realm)); return a; }
      case 'h': return r.v;
      case 'f': {
        if (realm.id === r.home) return r.v;
        const guard = g.__remoteFunctionUnpacker(r.x);
        remoteFns.set(guard, r.v);
        return guard;
      }
      case 'w': { const o = new g.Object(); for (const k in r.v) o[k] = des(r.v[k], realm); return g.__valueUnpacker(o, 'Worklet'); }
      case 'm': { const m = new g.Map(); r.v.forEach(([k, v]) => m.set(des(k, realm), des(v, realm))); return m; }
      case 'st': { const s = new g.Set(); r.v.forEach((e) => s.add(des(e, realm))); return s; }
      case 'e': { const e = new g.Error(r.v.message); e.name = r.v.name; if (r.v.stack !== undefined) e.stack = r.v.stack; return e; }
      case 'rx': return new g.RegExp(r.v.pattern, r.v.flags);
      case 'ab': { const b = new g.ArrayBuffer(r.v.byteLength); new g.Uint8Array(b).set(new Uint8Array(r.v)); return b; }
      case 'abv': { const b = des(r.v.buffer, realm); return new g[r.v.typeName](b, r.v.byteOffset, r.v.length); }
      case 'c': return g.__customSerializableUnpacker(des(r.v, realm), r.x);
      case 'sh': return shareable(r, realm);
      case 'sy': return g.__synchronizableUnpacker(r, r.x.fixed);
    }
    throw new Error(`[Worklets] cannot materialise serializable "${r.t}"`);
  }
  // Shareable: the value lives on its host runtime; every other runtime gets a guest (get/set through the host).
  function shareable(r, realm) {
    const s = r.x;
    if (realm.id === s.hostId) return hostValue(r);
    return realm.g.__shareableGuestUnpacker(s.hostId, r, des(s.decorateGuest, realm));
  }
  function hostValue(r) {
    const s = r.x, host = realms[s.hostId];
    if (!s.hasHost) { s.host = host.g.__shareableHostUnpacker(des(s.initial, host), des(s.decorateHost, host)); s.hasHost = true; }
    return s.host;
  }

  // ---- The UI runtime's job queue ----
  function reportUI(e) { sendError(`UI runtime: ${e && e.stack ? e.stack : e}`); }
  function runJob(fn) { try { fn(); } catch (e) { reportUI(e); } }
  function drainUI() {
    if (draining) return;
    draining = true;
    try { while (uiQueue.length) runJob(uiQueue.shift()); } finally { draining = false; }
  }
  const pushUI = (job) => { uiQueue.push(job); node.setImmediate(rt.guard(drainUI)); };
  const onRN = (fn) => node.setImmediate(rt.guard(fn)); // scheduleOnRN: a later turn of the JS thread
  const realmOf = (id) => { const r = realms[typeof id === 'object' ? id && id.__runtimeId : id]; if (!r) throw new Error(`[Worklets] unknown runtime ${id}`); return r; };
  const flagOf = (n) => STATIC_FLAGS[n] === true;

  // ---- __workletsModuleProxy (one per realm; only getCurrentThreadId and result materialisation differ) ----
  const proxyFor = (cur) => {
    const p = {
      loadUnpackersWithCode(...a) { for (let i = 0; i < a.length; i += 3) vm.runInContext(a[i], ctx, { filename: a[i + 1] || 'unpacker' })(); },
      loadUnpackersWithBytecode() { throw new Error('[Worklets] Hermes bytecode unpackers are not supported by the SimCore guest.'); },
      createSerializableImport() { throw new Error('[Worklets] Bundle mode (createSerializableImport) is not supported by the SimCore guest.'); },
      createSerializableString: scalar, createSerializableNumber: scalar, createSerializableBigInt: scalar,
      createSerializableBoolean: (b) => (b ? T : F), createSerializableUndefined: () => U, createSerializableNull: () => N,
      createSerializableTurboModuleLike: (props) => obj(props),
      createSerializableObject: (o, retain) => obj(o, retain),
      createSerializableHostObject: (o) => new Ref('h', o),
      createSerializableArray: (a) => new Ref('a', a.map(from)),
      createSerializableArrayBuffer: (b) => new Ref('ab', b.slice(0)),
      createSerializableMap: (ks, vs) => new Ref('m', ks.map((k, i) => [from(k), from(vs[i])])),
      createSerializableSet: (vs) => new Ref('st', vs.map(from)),
      createSerializableError: (name, message, stack) => new Ref('e', { name, message, stack }),
      createSerializableRegExp: (pattern, flags) => new Ref('rx', { pattern, flags }),
      createSerializableArrayBufferView: (typeName, buffer, byteOffset, length) => new Ref('abv', { typeName, buffer: new Ref('ab', buffer.slice(0)), byteOffset, length }),
      createSerializableNonWorkletFunction: (fn, name) => { const r = new Ref('f', fn, name); r.home = cur.id; return r; },
      createSerializableWorklet: (w) => worklet(w),
      createSerializableLEGACY: (v) => from(v),
      createCustomSerializable: (data, typeId) => new Ref('c', from(data), typeId),
      registerCustomSerializable(determine, pack, unpack, typeId) {
        customTypes[typeId] = { determine, pack, unpack };
        ui.g.__customSerializationRegistry[typeId] = { determine: des(determine, ui), pack: des(pack, ui), unpack: des(unpack, ui) };
      },
      createShareable(hostId, initial, initSync, decorateHost, decorateGuest) {
        const r = new Ref('sh', null, { hostId, initial, decorateHost, decorateGuest, hasHost: false });
        if (initSync) hostValue(r);
        return r;
      },
      scheduleOnRN(fn, args) { const f = remoteFns.get(fn) || fn, a = args ? des(args, rn) : []; onRN(() => f(...a)); },
      handlePromise(settle, value) { const f = remoteFns.get(settle) || settle, v = des(value, rn); onRN(() => f(v)); },
      scheduleOnUI(worklets, args) {
        worklets.v.forEach((w, i) => pushUI(() => des(w, ui)(...des(args.v[i], ui))));
      },
      runOnUISync(w) { drainUI(); return des(des(w, ui)(), cur); },
      createWorkletRuntime() { unsupported('Worklets', 'createWorkletRuntime (worker runtimes)'); throw new Error('[Worklets] Worker runtimes are not supported by the SimCore guest.'); },
      scheduleOnRuntime: (r, w) => p.scheduleOnRuntimeWithId(realmOf(r).id, w),
      scheduleOnRuntimeWithId(id, w) { const r = realmOf(id); const run = () => des(w, r)(); if (r.id === UI) pushUI(run); else onRN(run); },
      runOnRuntimeSync: (r, w) => p.runOnRuntimeSyncWithId(realmOf(r).id, w),
      runOnRuntimeSyncWithId(id, w) { const r = realmOf(id); if (r.id === UI) drainUI(); return des(des(w, r)(), cur); },
      reportFatalErrorOnJS: (message, stack, name) => sendError(`${name}: ${message}\n${stack}`),
      createSynchronizable: (value, fixed) => new Ref('sy', null, { fixed, value }), // fixed: a plain number/boolean, else a Ref
      synchronizableGetDirty: (r) => (r.x.fixed ? r.x.value : des(r.x.value, cur)),
      synchronizableGetBlocking: (r) => (r.x.fixed ? r.x.value : des(r.x.value, cur)),
      synchronizableSetBlocking(r, v) { r.x.value = v; }, synchronizableSetDirty(r, v) { r.x.value = v; },
      synchronizableLock() {}, synchronizableUnlock() {},
      getStaticFeatureFlag: flagOf, setDynamicFeatureFlag() {},
      getCurrentThreadId: () => cur.name,
      getUIRuntimeHolder: () => ({ __runtimeId: UI }), getUISchedulerHolder: () => ({}),
    };
    return p;
  };

  // ---- the UI runtime's globals ----
  function decorateUI() {
    const g = ui.g;
    g.global = g; g._WORKLET = true; g.__RUNTIME_KIND = UI; g.__RUNTIME_NAME = ui.name; g.__DEV__ = true;
    g.__customSerializationRegistry = [];
    g.__workletsModuleProxy = proxyFor(ui);
    g.performance = { now: () => node.performance.now() };
    g.HermesInternal = { enqueueJob: (job) => g.Promise.resolve().then(job) };
    g.__drainMicrotasks = () => {}; // microtasks drain on their own between turns
    g.evalWithSourceUrl = (code, url) => vm.runInContext(code, ctx, { filename: url });
    const out = (level) => (...a) => log(`[UI ${level}]`, a.map((x) => (typeof x === 'string' ? x : util.inspect(x, { depth: 3 }))).join(' '));
    g.console = { log: out('log'), warn: out('warn'), error: out('error'), info: out('info'), debug: out('debug'), assert: out('assert') };
    g._log = (v) => log('[UI]', typeof v === 'string' ? v : util.inspect(v, { depth: 3 }));
    g._toString = (v) => (typeof v === 'string' ? v : util.inspect(v, { depth: 2 }));
    g._beginSection = () => {}; g._endSection = () => {};
    const make = { _createSerializableHostObject: (o) => new Ref('h', o), _createSerializableString: scalar, _createSerializableNumber: scalar,
      _createSerializableBoolean: (b) => (b ? T : F), _createSerializableBigInt: scalar, _createSerializableUndefined: () => U, _createSerializableNull: () => N,
      _createSerializableArray: (a) => new Ref('a', a.map(from)), _createSerializableObject: (o, retain) => obj(o, retain), _createSerializableWorklet: (w) => worklet(w),
      _createSerializableSynchronizable: (s) => s, _scheduleOnRuntime: (r, w) => proxyFor(ui).scheduleOnRuntime(r, w) };
    Object.assign(g, make);
    // The display link: every frame callback runs on the UI realm, on the JS thread's timer.
    g.__nativeRequestAnimationFrame = (cb) => {
      node.setTimeout(rt.guard(() => runJob(() => { drainUI(); cb(node.performance.now()); })), FRAME_MS);
    };
    g._getAnimationTimestamp = () => node.performance.now();
  }

  // ---- TurboModules ----
  const worklets = {
    installTurboModule() {
      globalThis._WORKLET = false;
      globalThis._WORKLETS_VERSION_CPP = globalThis._WORKLETS_VERSION_JS; // one package: same build as the JS
      globalThis.__workletsModuleProxy = proxyFor(rn);
      decorateUI();
      return true;
    },
    prepareBundleMode: () => false, start: () => true, toggleSlowAnimationsOnUIRuntime: () => false,
  };

  // ---- Reanimated's native part ----
  const overlay = new Map(); // tag -> props animated by useAnimatedStyle / animatedProps (PropsRegistry)
  const layoutAnim = new Map(); // tag -> props animated by a layout (entering) animation
  const entering = new Map(); // nativeID (Reanimated id) -> Ref of the config
  const started = new Set(); // tags that began their entering animation
  const handlers = new Map(); // id -> {tag, name, handler}
  let handlerSeq = 0, warnedLayout = false;
  const nodeOf = (w) => (typeof w === 'number' ? rt.nodes.get(w) : w && rt.nodes.get(w.reactTag));
  const merge = (map, tag, props) => {
    const cur = Object.assign({}, map.get(tag));
    for (const k of Object.keys(props)) { if (props[k] === undefined) delete cur[k]; else cur[k] = props[k]; }
    map.set(tag, cur);
    rt.scheduleEmit();
  };
  const topName = (n) => (/^on[A-Z]/.test(n) ? 'top' + n.slice(2) : n);

  function decorateReanimatedUI() {
    const g = ui.g;
    g._updateProps = (ops) => { for (const { shadowNodeWrapper: w, updates } of ops) if (nodeOf(w)) merge(overlay, w.reactTag, updates); };
    g._maybeFlushUIUpdatesQueue = () => rt.scheduleEmit();
    g._notifyAboutProgress = (tag, style) => merge(layoutAnim, tag, style);
    g._notifyAboutEnd = (tag) => { layoutAnim.delete(tag); rt.scheduleEmit(); };
    g._measure = (w) => {
      const n = nodeOf(w), f = n && rt.frames[n.reactTag];
      return f ? { x: f[0], y: f[1], width: f[2], height: f[3], pageX: f[0], pageY: f[1] } : null; // null until the host laid the view out
    };
    g._dispatchCommand = (w, name, args) => { const n = nodeOf(w); if (n) rt.command(n, name, Array.from(args)); };
    g._obtainProp = (w, name) => { const n = nodeOf(w), v = n && Object.assign({}, n.props, overlay.get(n.reactTag))[name]; return v === undefined ? '' : typeof v === 'string' ? v : JSON.stringify(v); };
    g._setGestureState = () => unsupported('Reanimated', '_setGestureState');
  }
  const cssDiag = (m) => () => unsupported('Reanimated CSS animations/transitions', m);
  const reanimated = {
    installTurboModule() {
      globalThis._REANIMATED_VERSION_CPP = globalThis._REANIMATED_VERSION_JS;
      decorateReanimatedUI();
      globalThis.__reanimatedModuleProxy = {
        registerEventHandler(handler, name, tag) { const id = ++handlerSeq; handlers.set(id, { tag, name: topName(name), ref: handler }); return id; },
        unregisterEventHandler: (id) => { handlers.delete(id); },
        getViewProp(w, name, cb) {
          const n = nodeOf(w), v = n && Object.assign({}, n.props, overlay.get(n.reactTag))[name];
          if (cb) onRN(() => cb(v));
          return Promise.resolve(v);
        },
        registerSensor() { unsupported('Reanimated', 'registerSensor'); return -1; }, unregisterSensor() {},
        getStaticFeatureFlag: flagOf, setDynamicFeatureFlag() {},
        subscribeForKeyboardEvents() { return -1; }, unsubscribeFromKeyboardEvents() {}, // no keyboard in the simulator surface
        configureLayoutAnimationBatch(batch) {
          for (const { viewTag, type, config } of batch) {
            if (type === 1) { if (config) entering.set(String(viewTag), config); else entering.delete(String(viewTag)); }
            else if (config && !warnedLayout) { warnedLayout = true; unsupported('Reanimated layout animations', `type ${type} (exiting / layout / shared transition)`); }
          }
        },
        setShouldAnimateExitingForTag() {},
        setViewStyle: (tag, style) => merge(overlay, tag, style),
        setCSSEventHandler() {}, markNodeAsRemovable() {}, unmarkNodeAsRemovable() {},
        registerCSSKeyframes: cssDiag('registerCSSKeyframes'), unregisterCSSKeyframes() {}, applyCSSAnimations: cssDiag('applyCSSAnimations'),
        unregisterCSSAnimations() {}, runCSSTransition: cssDiag('runCSSTransition'), unregisterCSSTransition() {},
        registerPseudoStyles: cssDiag('registerPseudoStyles'), unregisterPseudoStyles() {},
        getSettledUpdates: () => [],
      };
      return true;
    },
  };

  return {
    modules: { WorkletsModule: () => worklets, ReanimatedModule: () => reanimated },
    // Animated props to merge into `tag`'s props when the tree is emitted (layout animation on top of animated style).
    overlay: (tag) => { const a = overlay.get(tag), b = layoutAnim.get(tag); return a && b ? Object.assign({}, a, b) : a || b; },
    // Before each emit: start the `entering` animation of newly committed views (nativeID = Reanimated's id), drop overlays of removed views.
    beforeEmit(children) {
      const live = new Set();
      const visit = (n) => {
        live.add(n.reactTag);
        if (entering.size && n.props.nativeID != null && entering.has(n.props.nativeID) && !started.has(n.reactTag)) this.start(n);
        n.children.forEach(visit);
      };
      children.forEach(visit);
      for (const m of [overlay, layoutAnim]) for (const tag of m.keys()) if (!live.has(tag)) m.delete(tag);
      for (const tag of started) if (!live.has(tag)) started.delete(tag);
    },
    start(n) {
      started.add(n.reactTag);
      const f = rt.frames[n.reactTag] || [0, 0, 0, 0], p = rt.frames[rt.parents.get(n.reactTag)] || [0, 0];
      const v = { targetOriginX: f[0] - p[0], targetOriginY: f[1] - p[1], targetWidth: f[2], targetHeight: f[3],
        targetGlobalOriginX: f[0], targetGlobalOriginY: f[1], windowWidth: rt.window.width, windowHeight: rt.window.height };
      runJob(() => {
        const mgr = ui.g.LayoutAnimationsManager;
        if (!mgr) throw new Error('[Reanimated] LayoutAnimationsManager is not initialised');
        mgr.start(n.reactTag, 1, Object.assign(new ui.g.Object(), v), des(entering.get(n.props.nativeID), ui));
      });
    },
    // Native event -> `useEvent` handlers registered for this view (called by Runtime.dispatch).
    onEvent(tag, type, payload) {
      for (const h of handlers.values()) {
        if (h.tag !== tag || h.name !== type) continue;
        pushUI(() => des(h.ref, ui)(Object.assign(des(from(payload), ui), { eventName: type })));
      }
    },
  };
};
