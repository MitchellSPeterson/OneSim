// SimCore native-module registry: ONE table of every native module the guest provides.
// The TurboModule proxy (`__turboModuleProxy`, `NativeModules`) and `globalThis.expo.modules` are generated from it, and
// `{"type":"modules"}` lists it.
//   entry = { name, package?, kind: 'turbo'|'expo', status: 'implemented'|'simulated'|'unsupported', constants?, methods: {name: impl} | () => {...},
//             events?: [names], permissions?: [names], async?: bool, strict?: bool, make?: () => object }
// A method impl of 'unsupported' (UNSUP) diagnoses and throws (rejects for `...Async` names or `async: true`).
// `strict` entries (and every unsupported one) diagnose any method they do not list. `make` is an escape hatch: the module as is.
'use strict';
const UNSUP = 'unsupported';
const HIDDEN = new Set(['getConstants', 'addListener', 'removeListeners']);
// Optional modules RN feature-detects: absent => JS falls back (JS Animated driver, no perf API, ...).
const ABSENT = new Set(['NativeAnimatedModule', 'NativeAnimatedTurboModule', 'NativeCPUTime', 'ReactDevToolsRuntimeSettingsModule', 'NativeReactDevToolsSettingsManager', 'NativeIdleCallbacksCxx', 'NativeIdleCallbacks', 'JSCExecutor', 'IntentAndroid', 'SoundManager', 'Performance', 'HeadlessJsTaskSupport', 'PerformanceObserver', 'IntersectionObserver', 'MutationObserver', 'ResizeObserver', 'FrameRateLogger', 'UIManager', 'I18nManagerAndroid']);

module.exports = function createRegistry(rt, h) {
  const { node, log, sendError, sendEvent, unsupported, cfg, ver, insetsFor, loadImage, imageDone, expo: { NativeModule, SharedObject } } = h;
  const noop = () => {};
  const listener = { addListener: noop, removeListeners: noop };
  const reportException = (d) => sendError(`${d.message}\n${(d.stack || []).map((f) => `  at ${f.methodName || '?'} (${f.file}:${f.lineNumber}:${f.column})`).join('\n')}`);
  const all = (names, v = UNSUP) => Object.fromEntries(names.map((n) => [n, v]));

  // ---- simulated platform state (host control: `simulate`) ----
  const sim = {
    clip: '', clipEm: null,
    perms: { location: 'undetermined' }, // undetermined -> granted on request (the user taps Allow); denied stays denied
    pos: { latitude: 37.3349, longitude: -122.009, accuracy: 5, altitude: 0, heading: null, speed: null }, // Apple Park
    watchers: new Map(), // watchId -> expo-location module instance (event emitter)
  };
  const permission = (k) => { const s = sim.perms[k]; return { status: s, expires: 'never', granted: s === 'granted', canAskAgain: s !== 'denied' }; };
  const ask = (k) => { if (sim.perms[k] === 'undetermined') sim.perms[k] = 'granted'; return permission(k); };
  const locPermission = (r) => ({ ...r, ios: { scope: r.granted ? 'whenInUse' : 'none' } });
  const fix = () => { const p = sim.pos; return { coords: { latitude: p.latitude, longitude: p.longitude, altitude: p.altitude, accuracy: p.accuracy, altitudeAccuracy: p.accuracy, heading: p.heading, speed: p.speed }, timestamp: Date.now() }; };
  const needLocation = () => { if (sim.perms.location !== 'granted') throw Object.assign(new Error('Not authorized to use location services'), { code: 'E_LOCATION_UNAUTHORIZED' }); };
  const pushFix = (id, em) => rt.guard(() => em.emit('Expo.locationChanged', { watchId: id, location: fix() }))();
  const called = (mod, fns) => Object.fromEntries(Object.entries(fns).map(([m, f]) => [m, function (...a) { sendEvent('module.called', { module: mod, method: m, args: a }); return f.apply(this, a); }]));
  const setClip = (t) => { sim.clip = String(t); };

  const simulators = {
    clipboard: (s) => { setClip(s.text ?? ''); if (sim.clipEm) rt.guard(() => sim.clipEm.emit('onClipboardChanged', { contentTypes: ['plain-text'] }))(); },
    location: (s) => {
      for (const k of Object.keys(sim.pos)) if (s[k] !== undefined) sim.pos[k] = s[k];
      for (const [id, em] of sim.watchers) pushFix(id, em);
    },
    permissions: (s) => {
      for (const [k, v] of Object.entries(s)) {
        if (['granted', 'denied', 'undetermined'].includes(v)) sim.perms[k] = v; else log(`SimCore: simulate permissions: bad status ${JSON.stringify(v)} for ${k}`);
      }
    },
  };

  const entries = [
    // ---- React Native core (TurboModules) ----
    { name: 'PlatformConstants', status: 'implemented', constants: () => ({ isTesting: false, isDisableAnimations: false, reactNativeVersion: { major: +ver[1], minor: +ver[2], patch: +ver[3] }, forceTouchAvailable: false, osVersion: '26.5', systemName: 'iOS', interfaceIdiom: 'phone' }) },
    // DeviceInfo.Dimensions feeds RN's Dimensions / useWindowDimensions (session viewport); host `environment` updates it.
    { name: 'DeviceInfo', status: 'simulated', constants: () => ({ Dimensions: rt.dimensions() }) },
    { name: 'Appearance', status: 'simulated', methods: { getColorScheme: () => rt.appearance, ...listener }, events: ['appearanceChanged'] },
    { name: 'SourceCode', status: 'implemented', constants: () => ({ scriptURL: rt.scriptURL }) },
    { name: 'I18nManager', status: 'implemented', constants: { isRTL: false, doLeftAndRightSwapInRTL: true, localeIdentifier: 'en_US' }, methods: { allowRTL: noop, forceRTL: noop, swapLeftAndRightInRTL: noop } },
    // Host chrome (status bar) is the Rust host's job; calls are accepted and logged, not rendered.
    { name: 'StatusBarManager', status: 'implemented', constants: { HEIGHT: 54, DEFAULT_BACKGROUND_COLOR: 0, STYLE: 'default' },
      methods: { ...listener, getHeight: (cb) => cb({ height: 54 }), setStyle: (s) => log('SimCore: StatusBar.setStyle', s), setHidden: noop, setNetworkActivityIndicatorVisible: noop } },
    { name: 'ExceptionsManager', status: 'implemented', methods: { reportException, reportFatalException: (m, s) => reportException({ message: m, stack: s }), reportSoftException: (m) => log('SimCore soft exception:', m), updateExceptionMessage: noop, dismissRedbox: noop } },
    { name: 'AppState', status: 'implemented', constants: { initialAppState: 'active' }, methods: { getCurrentAppState: (cb) => cb({ app_state: 'active' }), ...listener } },
    { name: 'SettingsManager', status: 'implemented', constants: { settings: { AppleLocale: 'en_US', AppleLanguages: ['en'] } }, methods: { setValues: noop, deleteValues: noop } },
    { name: 'AccessibilityInfo', status: 'implemented', constants: {}, methods: {
      ...Object.fromEntries(['isReduceMotionEnabled', 'isReduceTransparencyEnabled', 'isBoldTextEnabled', 'isGrayscaleEnabled', 'isInvertColorsEnabled', 'isDarkerSystemColorsEnabled', 'isScreenReaderEnabled', 'getCurrentReduceMotionState', 'getCurrentReduceTransparencyState',
        'getCurrentBoldTextState', 'getCurrentGrayscaleState', 'getCurrentInvertColorsState', 'getCurrentDarkerSystemColorsState', 'getCurrentPrefersCrossFadeTransitionsState'].map((n) => [n, (cb) => cb(false)])),
      setAccessibilityFocus: noop, announceForAccessibility: noop, ...listener } },
    { name: 'KeyboardObserver', status: 'implemented', methods: listener },
    { name: 'Networking', status: 'implemented', methods: () => rt.networking() },
    { name: 'BlobModule', status: 'implemented', methods: () => rt.blobModule() },
    { name: 'FileReaderModule', status: 'implemented', methods: () => rt.fileReaderModule() },
    { name: 'WebSocketModule', status: 'implemented', methods: () => rt.webSocketModule() },
    { name: 'LogBox', status: 'implemented', methods: { show: noop, hide: noop } },
    { name: 'DevSettings', status: 'implemented', methods: { reload: () => rt.reload('reload'), reloadWithReason: (r) => rt.reload(r), onFastRefresh: () => sendEvent('refresh.applied', {}), ...listener, setHotLoadingEnabled: noop, setIsDebuggingRemotely: noop, toggleElementInspector: noop, addMenuItem: noop, openDebugger: noop, setProfilingEnabled: noop } },
    { name: 'DevLoadingView', status: 'implemented', methods: { showMessage: noop, hide: noop } },
    { name: 'DevMenu', status: 'implemented', methods: { show: noop, reload: noop, setProfilingEnabled: noop, setHotLoadingEnabled: noop } },
    { name: 'RedBox', status: 'implemented', methods: { setExtraData: noop, dismiss: noop } },
    { name: 'Timing', status: 'implemented', methods: { createTimer: noop, deleteTimer: noop, setSendIdleEvents: noop } },
    { name: 'NativePerformanceCxx', status: 'implemented', methods: {
      now: () => node.performance.now(), timeOrigin: () => node.performance.timeOrigin, reportMark: noop, reportMeasure: noop,
      getMarkTime: () => undefined, clearMarks: noop, clearMeasures: noop, getEntries: () => [], getEntriesByName: () => [],
      getEntriesByType: () => [], getEventCounts: () => [], getSimpleMemoryInfo: () => ({}), getReactNativeStartupTiming: () => ({}),
      createObserver: () => ({}), getDroppedEntriesCount: () => 0, observe: noop, disconnect: noop, takeRecords: () => [],
      getSupportedPerformanceEntryTypes: () => [], clearEventCountsForTesting: noop } },
    { name: 'NativeMicrotasksCxx', status: 'implemented', methods: { queueMicrotask: (f) => node.queueMicrotask(rt.guard(f)) } },
    // Sizes are pixels (scale 1), like iOS for an unscaled image.
    { name: 'ImageLoader', status: 'implemented', methods: {
      getSize: (uri) => loadImage(uri).then((i) => [i.width, i.height]),
      getSizeWithHeaders: (uri, headers) => loadImage(uri, headers).then((i) => ({ width: i.width, height: i.height })),
      prefetchImage: (uri) => loadImage(uri).then(() => true, () => false),
      prefetchImageWithMetadata: (uri) => loadImage(uri).then(() => true, () => false),
      queryCache: (uris) => Promise.resolve(Object.fromEntries(uris.filter((u) => imageDone.has(u)).map((u) => [u, 'disk']))),
      abortRequest: noop } },
    { name: 'LinkingManager', status: 'implemented', async: true, methods: { getInitialURL: () => Promise.resolve(null), canOpenURL: () => Promise.resolve(false), openURL: UNSUP, openSettings: UNSUP, ...listener } },
    // JS defaults apply for every flag when the native getter returns undefined.
    { name: 'NativeReactNativeFeatureFlagsCxx', status: 'implemented', make: () => new Proxy({}, { get: (t, p) => (typeof p === 'string' ? () => undefined : undefined) }) },
    // Portrait insets from the host's device profile (62/34 on the iPhone 17 Pro).
    { name: 'RNCSafeAreaContext', package: 'react-native-safe-area-context', status: 'simulated', constants: () => {
      const insets = insetsFor();
      return { initialWindowMetrics: { frame: { x: 0, y: 0, width: rt.window.width, height: rt.window.height }, insets, verticalCornerAdaptedInsets: insets, horizontalCornerAdaptedInsets: insets } };
    } },
    { name: 'RNSModule', package: 'react-native-screens', status: 'implemented' }, // empty spec, presence only
    { name: 'ExpoModulesCore', package: 'expo-modules-core', status: 'implemented', methods: { installModules: noop } },
    ...Object.entries(rt.worklets.modules).map(([name, make]) => ({ name, package: name === 'WorkletsModule' ? 'react-native-worklets' : 'react-native-reanimated', status: 'implemented', make })),
    { name: 'NativeDOMCxx', status: 'implemented', make: () => rt.domModule() },
    { name: 'EventDispatcher', status: 'implemented' },
    // RN modules with platform behaviour we do not implement: every call is a diagnostic (and an error for the caller).
    { name: 'ActionSheetManager', status: 'unsupported', methods: all(['showActionSheetWithOptions', 'showShareActionSheetWithOptions', 'dismissActionSheet']) },
    { name: 'AlertManager', status: 'unsupported', methods: all(['alertWithArgs']) },
    { name: 'ShareModule', status: 'unsupported', async: true, methods: all(['share']) },
    { name: 'PushNotificationManager', status: 'unsupported', async: true },
    { name: 'ImageStoreManager', status: 'unsupported', async: true },
    { name: 'AccessibilityManager', status: 'unsupported' },
    { name: 'DevToolsSettingsManager', status: 'unsupported' },
    { name: 'Vibration', status: 'unsupported', methods: all(['vibrate', 'cancel']) },
    { name: 'ImageEditor', status: 'unsupported', async: true },
    // RN's Clipboard TurboModule (RN <= 0.8x NativeClipboard): the same in-memory clipboard as expo-clipboard.
    { name: 'Clipboard', status: 'simulated', strict: true, async: true, methods: called('Clipboard', { getString: async () => sim.clip, setString: (t) => { setClip(t); } }) },

    // ---- expo modules (globalThis.expo.modules) ----
    { name: 'ExpoFetchModule', package: 'expo', kind: 'expo', status: 'implemented', methods: () => rt.expoFetch(SharedObject) },
    { name: 'ExpoAsset', package: 'expo-asset', kind: 'expo', status: 'implemented', methods: { downloadAsync: async (uri) => { unsupported('ExpoAsset', 'downloadAsync', 'expo-asset'); return uri; } } },
    { name: 'ExpoFontLoader', package: 'expo-font', kind: 'expo', status: 'implemented', methods: { getLoadedFonts: () => [], isLoaded: () => false, loadAsync: async (name) => { unsupported('ExpoFontLoader', `loadAsync(${name})`, 'expo-font'); } } },
    // Screen-awake is a no-op in a simulator surface; accepted, not rendered.
    { name: 'ExpoKeepAwake', package: 'expo-keep-awake', kind: 'expo', status: 'implemented', methods: { activate: async () => {}, deactivate: async () => {}, isActivated: async () => false } },
    // expo-splash-screen: the native launch screen is not part of the SimCore surface, so hiding it has nothing to do.
    { name: 'ExpoSplashScreen', package: 'expo-splash-screen', kind: 'expo', status: 'implemented', methods: { setOptions: () => {}, hide: () => {}, hideAsync: async () => {}, preventAutoHideAsync: async () => true, internalPreventAutoHideAsync: async () => {}, internalMaybeHideAsync: async () => {} } },
    // expo-system-ui: the root view's background colour is a prop of the tree's root node (processed colour int, like any view).
    { name: 'ExpoSystemUI', package: 'expo-system-ui', kind: 'expo', status: 'implemented', methods: {
      setBackgroundColorAsync: async (c) => { if (c == null) delete rt.rootProps.backgroundColor; else rt.rootProps.backgroundColor = c; rt.scheduleEmit(); },
      getBackgroundColorAsync: async () => (rt.rootProps.backgroundColor == null ? null : '#' + (rt.rootProps.backgroundColor >>> 0).toString(16).padStart(8, '0').replace(/^(..)(......)$/, '$2$1')) } },
    { name: 'ExpoDevice', package: 'expo-device', kind: 'expo', status: 'simulated',
      constants: { isDevice: false, brand: 'Apple', manufacturer: 'Apple', modelId: 'iPhone18,1', modelName: 'iPhone 17 Pro', deviceYearClass: 2025, totalMemory: 12 * 1024 ** 3,
        osName: 'iOS', osVersion: '26.5', osBuildId: '23F77', osInternalBuildId: '23F77', deviceName: 'iPhone', deviceType: 1, supportedCpuArchitectures: ['arm64e', 'arm64'] },
      methods: { getDeviceTypeAsync: async () => 1, getUptimeAsync: async () => node.performance.now(), isRootedExperimentalAsync: async () => false } },
    // expo-linking: no launch URL, so Expo Router falls back to the scheme root and lands on `index`. Opening URLs is unsupported.
    { name: 'ExpoLinking', package: 'expo-linking', kind: 'expo', status: 'implemented', methods: { getLinkingURL: () => null, clearInitialURL: () => {} } },
    { name: 'ExpoWebBrowser', package: 'expo-web-browser', kind: 'expo', status: 'unsupported', methods: all(['openBrowserAsync', 'dismissBrowser', 'openAuthSessionAsync', 'dismissAuthSession', 'warmUpAsync', 'coolDownAsync', 'mayInitWithUrlAsync', 'getCustomTabsSupportingBrowsers']) },
    // expo-image: ImageLoader shares the guest's image cache; the rest of the cache API is not backed by anything.
    { name: 'ExpoImage', package: 'expo-image', kind: 'expo', status: 'implemented', constants: { ViewPrototypes: {} },
      methods: { configureCache: () => {}, clearMemoryCache: async () => false, clearDiskCache: async () => false,
        prefetch: async (urls) => (await Promise.all(urls.map((u) => loadImage(u).then(() => true, () => false)))).every(Boolean),
        ...all(['generateBlurhashAsync', 'generateThumbhashAsync', 'getCachePathAsync', 'writeToCacheAsync', 'readFromCacheAsync', 'loadAsync']) } },
    { name: 'SymbolModule', package: 'expo-symbols', kind: 'expo', status: 'implemented', constants: { ViewPrototypes: {} } },
    { name: 'ExpoGlassEffect', package: 'expo-glass-effect', kind: 'expo', status: 'implemented', constants: { isLiquidGlassAvailable: true, isGlassEffectAPIAvailable: true, ViewPrototypes: {} } },
    // expo-constants: the manifest is the same one Metro hands a real Expo client; expo-asset uses it to build dev-server asset URLs.
    { name: 'ExponentConstants', package: 'expo-constants', kind: 'expo', status: 'implemented',
      constants: () => ({ manifest: rt.manifest, experienceUrl: rt.manifest ? `exp://${new URL(rt.scriptURL).host}` : null,
        appOwnership: null, executionEnvironment: 'bare', sessionId: cfg.session || 'simcore', installationId: 'simcore', statusBarHeight: 54,
        systemFonts: [], isDevice: false, deviceName: 'iPhone', expoVersion: null, platform: { ios: { model: 'iPhone', systemVersion: '26.5' } } }),
      methods: { getWebViewUserAgentAsync: async () => 'Mozilla/5.0 (iPhone; CPU iPhone OS 26_5 like Mac OS X) SimCore' } },

    // ---- simulated expo modules (host control through `simulate`) ----
    { name: 'ExpoClipboard', package: 'expo-clipboard', kind: 'expo', status: 'simulated', strict: true, events: ['onClipboardChanged'], constants: { isPasteButtonAvailable: false },
      methods: {
        getStringAsync: async function () { sim.clipEm = this; return sim.clip; },
        ...called('ExpoClipboard', { setStringAsync: async function (t) { sim.clipEm = this; setClip(t); return true; } }),
        hasStringAsync: async () => sim.clip !== '',
        ...all(['getImageAsync', 'setImageAsync', 'hasImageAsync']) } },
    { name: 'ExpoHaptics', package: 'expo-haptics', kind: 'expo', status: 'simulated', strict: true,
      methods: called('ExpoHaptics', { impactAsync: async () => {}, notificationAsync: async () => {}, selectionAsync: async () => {}, performHapticsAsync: async () => {} }) },
    { name: 'ExpoLocation', package: 'expo-location', kind: 'expo', status: 'simulated', strict: true, permissions: ['location'], events: ['Expo.locationChanged', 'Expo.locationError'],
      methods: {
        getForegroundPermissionsAsync: async () => locPermission(permission('location')),
        requestForegroundPermissionsAsync: async () => locPermission(ask('location')),
        hasServicesEnabledAsync: async () => true,
        getProviderStatusAsync: async () => ({ locationServicesEnabled: true, backgroundModeEnabled: false }),
        getCurrentPositionAsync: async () => { needLocation(); return fix(); },
        getLastKnownPositionAsync: async () => (sim.perms.location === 'granted' ? fix() : null),
        watchPositionImplAsync: async function (id) { needLocation(); sim.watchers.set(id, this); node.setImmediate(() => sim.watchers.has(id) && pushFix(id, this)); }, // iOS reports the first fix right away
        removeWatchAsync: async (id) => { sim.watchers.delete(id); },
        ...all(['getBackgroundPermissionsAsync', 'requestBackgroundPermissionsAsync', 'geocodeAsync', 'reverseGeocodeAsync', 'startLocationUpdatesAsync', 'stopLocationUpdatesAsync', 'hasStartedLocationUpdatesAsync',
          'startGeofencingAsync', 'stopGeofencingAsync', 'hasStartedGeofencingAsync', 'watchDeviceHeading', 'getMotionActivityPermissionsAsync', 'requestMotionActivityPermissionsAsync', 'watchMotionActivityImplAsync']) } },
    { name: 'ExpoCamera', package: 'expo-camera', kind: 'expo', status: 'unsupported', permissions: ['camera', 'microphone'],
      methods: all(['getCameraPermissionsAsync', 'requestCameraPermissionsAsync', 'getMicrophonePermissionsAsync', 'requestMicrophonePermissionsAsync', 'isAvailableAsync', 'getAvailableVideoCodecsAsync', 'scanFromURLAsync']) },
  ].map((e) => ({ kind: 'turbo', package: e.kind === 'expo' ? undefined : 'react-native', ...e }));
  const byName = new Map(entries.map((e) => [e.name, e]));

  const implsOf = (e) => (typeof e.methods === 'function' ? e.methods() : e.methods || {});
  const fail = (e, m) => { const err = unsupported(e.name, m, e.package); if (e.async || /Async$/.test(m)) return Promise.reject(err); throw err; };
  // Instance of an entry: methods + constants for its kind, with the diagnostics for unsupported/unknown methods.
  const build = (e) => {
    if (e.make) return e.make();
    const m = {};
    for (const [k, f] of Object.entries(implsOf(e))) m[k] = f === UNSUP ? () => fail(e, k) : f;
    const consts = e.constants;
    let obj;
    if (e.kind === 'expo') obj = Object.assign(new NativeModule(), typeof consts === 'function' ? consts() : consts, m, { __expo_module_name__: e.name });
    else obj = consts ? { getConstants: typeof consts === 'function' ? consts : () => consts, ...m } : m;
    if (!(e.strict || e.status === 'unsupported')) return obj;
    return new Proxy(obj, {
      get: (t, p, r) => {
        if (p in t || typeof p !== 'string' || !/^[A-Za-z]\w*$/.test(p) || p === 'then' || p === 'toJSON') return Reflect.get(t, p, r);
        if (p === 'getConstants') return () => { unsupported(e.name, p, e.package); return {}; }; // import-time read: diagnosed, not fatal
        return (...a) => fail(e, p);
      },
    });
  };
  const cache = new Map(), missed = new Set(); // modules already reported as absent
  const instance = (e) => { if (!cache.has(e.name)) cache.set(e.name, build(e)); return cache.get(e.name); };

  return {
    // TurboModuleRegistry / NativeModules.
    turbo: {
      absent: ABSENT,
      explicit: (name) => { const e = byName.get(name); return e && e.kind === 'turbo' && e.status !== 'unsupported' ? instance(e) : undefined; }, // legacy NativeModules.X: absent = undefined
      get(name) {
        const e = byName.get(name);
        if (e && e.kind === 'turbo') return instance(e);
        if (!ABSENT.has(name) && !missed.has(`t:${name}`)) log(`SimCore: TurboModule "${name}" is not available (null, as on a binary without it)`);
        missed.add(`t:${name}`);
        return null;
      },
    },
    // globalThis.expo.modules: unknown modules are *absent* (undefined) so requireNativeModule throws / requireOptionalNativeModule is null.
    expoModules: new Proxy({}, {
      get: (t, name) => {
        if (typeof name === 'symbol') return undefined;
        const e = byName.get(name);
        if (!e || e.kind !== 'expo') { if (!missed.has(`e:${name}`)) log(`SimCore: expo module "${name}" is not available`); missed.add(`e:${name}`); return undefined; }
        return instance(e);
      },
    }),
    list: () => entries.map((e) => ({
      name: e.name, package: e.package, status: e.status,
      methods: Object.entries(e.make ? {} : implsOf(e)).filter(([k, v]) => typeof v === 'function' || v === UNSUP).map(([k]) => k).filter((k) => !HIDDEN.has(k)),
      events: e.events || [], permissions: e.permissions || [],
    })),
    simulate(module, state) {
      const f = simulators[String(module).toLowerCase()];
      if (f) f(state || {}); else log(`SimCore: simulate: no simulated module "${module}" (have: ${Object.keys(simulators).join(', ')})`);
    },
  };
};
