// OneSim native-module registry: ONE table of every native module the guest provides.
// The TurboModule proxy (`__turboModuleProxy`, `NativeModules`) and `globalThis.expo.modules` are generated from it, and
// `{"type":"modules"}` lists it.
//   entry = { name, package?, kind: 'turbo'|'expo', status: 'implemented'|'simulated'|'unsupported', constants?, methods: {name: impl} | () => {...},
//             events?: [names], permissions?: [names], async?: bool, strict?: bool, make?: () => object }
// A method impl of 'unsupported' (UNSUP) diagnoses and throws (rejects for `...Async` names or `async: true`).
// `strict` entries (and every unsupported one) diagnose any method they do not list. `make` is an escape hatch: the module as is.
'use strict';
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { Buffer } = require('node:buffer');
const process = require('node:process');
const UNSUP = 'unsupported';
// iOS release the host simulates (SIMCORE_OS_VERSION) -> its Simulator runtime build; pinned in docs/compatibility/versions.json.
const IOS_BUILDS = { '27.2': '24B8117', '26.5': '23F77' };
const OS_VERSION = IOS_BUILDS[process.env.SIMCORE_OS_VERSION] ? process.env.SIMCORE_OS_VERSION : '27.2';
const OS_BUILD = IOS_BUILDS[OS_VERSION];
const ANDROID_API_LEVEL = 37;
const ANDROID_RELEASE = '17';
// expo-device identity of the simulated phone (SIMCORE_DEVICE profile slug): [model id, year class]; others report a generic model.
const MODELS = { 'iphone-18-pro': ['iPhone19,2', 2026], 'iphone-18-pro-max': ['iPhone19,3', 2026], 'iphone-duo': ['iPhone19,4', 2026], 'iphone-17-pro': ['iPhone18,1', 2025], 'iphone-17-pro-max': ['iPhone18,2', 2025], 'iphone-17': ['iPhone18,3', 2025], 'iphone-air': ['iPhone18,4', 2025], 'iphone-17e': ['iPhone18,5', 2026] };
const DEVICE = process.env.SIMCORE_DEVICE || 'iphone-18-pro';
const MODEL_NAME = DEVICE.split('-').map((w) => (w === 'iphone' ? 'iPhone' : w === 'se' ? 'SE' : w[0].toUpperCase() + w.slice(1))).join(' ');
const [MODEL_ID, YEAR_CLASS] = MODELS[DEVICE] || [null, null];
const HIDDEN = new Set(['getConstants', 'addListener', 'removeListeners']);
// Optional modules RN feature-detects: absent => JS falls back (JS Animated driver, no perf API, ...).
const ABSENT = new Set(['NativeAnimatedModule', 'NativeAnimatedTurboModule', 'NativeCPUTime', 'ReactDevToolsRuntimeSettingsModule', 'NativeReactDevToolsSettingsManager', 'NativeIdleCallbacksCxx', 'NativeIdleCallbacks', 'JSCExecutor', 'IntentAndroid', 'SoundManager', 'Performance', 'HeadlessJsTaskSupport', 'PerformanceObserver', 'IntersectionObserver', 'MutationObserver', 'ResizeObserver', 'FrameRateLogger', 'UIManager', 'I18nManagerAndroid']);

module.exports = function createRegistry(rt, h) {
  const { node, log, sendError, sendEvent, unsupported, cfg, ver, insetsFor, loadImage, imageDone, readUri, expo: { NativeModule, SharedObject } } = h;
  const android = cfg.platform === 'android';
  const androidVersion = android ? (process.env.SIMCORE_OS_VERSION || ANDROID_RELEASE) : null;
  const androidModel = 'Pixel 10a';
  const noop = () => {};
  let alertSeq = 0;
  const listener = { addListener: noop, removeListeners: noop };
  const reportException = (d) => sendError(`${d.message}\n${(d.stack || []).map((f) => `  at ${f.methodName || '?'} (${f.file}:${f.lineNumber}:${f.column})`).join('\n')}`);
  const all = (names, v = UNSUP) => Object.fromEntries(names.map((n) => [n, v]));

  // ---- simulated platform state (host control: `simulate`) ----
  const sim = {
    clip: '', clipEm: null,
    perms: { location: 'undetermined', tracking: 'undetermined' }, // undetermined -> granted on request (the user taps Allow); denied stays denied
    pos: { latitude: 37.3349, longitude: -122.009, accuracy: 5, altitude: 0, heading: null, speed: null }, // Apple Park
    watchers: new Map(), // watchId -> expo-location module instance (event emitter)
    battery: { level: 1, state: 3, lowPowerMode: false }, brightness: 0.5, lock: 0, localauth: { success: true }, // expo-battery / -brightness / -screen-orientation / -local-authentication
  };
  const permission = (k) => { const s = sim.perms[k]; return { status: s, expires: 'never', granted: s === 'granted', canAskAgain: s !== 'denied' }; };
  const ask = (k) => { if (sim.perms[k] === 'undetermined') sim.perms[k] = 'granted'; return permission(k); };
  const locPermission = (r) => ({ ...r, ios: { scope: r.granted ? 'whenInUse' : 'none' } });
  const fix = () => { const p = sim.pos; return { coords: { latitude: p.latitude, longitude: p.longitude, altitude: p.altitude, accuracy: p.accuracy, altitudeAccuracy: p.accuracy, heading: p.heading, speed: p.speed }, timestamp: Date.now() }; };
  const needLocation = () => { if (sim.perms.location !== 'granted') throw Object.assign(new Error('Not authorized to use location services'), { code: 'E_LOCATION_UNAUTHORIZED' }); };
  const pushFix = (id, em) => rt.guard(() => em.emit('Expo.locationChanged', { watchId: id, location: fix() }))();
  const emitOn = (name, ev, d) => rt.guard(() => instance(byName.get(name)).emit(ev, d))();
  const called = (mod, fns) => Object.fromEntries(Object.entries(fns).map(([m, f]) => [m, function (...a) { sendEvent('module.called', { module: mod, method: m, args: a }); return f.apply(this, a); }]));
  const setClip = (t) => { sim.clip = String(t); };
  // expo-secure-store: a JSON file in the session data dir, so values survive reloads (the keychain survives app restarts).
  const storeFile = path.join(cfg.dataDir, 'securestore.json');
  const store = { read: () => { try { return JSON.parse(fs.readFileSync(storeFile, 'utf8')); } catch (_) { return {}; } }, write: (o) => fs.writeFileSync(storeFile, JSON.stringify(o)) };
  const storeKey = (key, o) => `${(o && o.keychainService) || 'app'}\u0000${key}`;
  const storeGet = (key, o) => store.read()[storeKey(key, o)] ?? null;
  const storeSet = (value, key, o) => { const s = store.read(); s[storeKey(key, o)] = String(value); store.write(s); return true; };

  const files = require('./filesystem.js')(h);
  const asyncStorage = require('./asyncstorage.js')(h);
  const hash = (alg, buf) => crypto.createHash(String(alg).replace('-', '').toLowerCase()).update(buf), granted = { status: 'granted', expires: 'never', granted: true, canAskAgain: true };
  const PORTRAIT_LOCKS = [0, 2, 3], ZERO_UUID = '00000000-0000-0000-0000-000000000000'; // OrientationLock DEFAULT, PORTRAIT, PORTRAIT_UP

  const simulators = {
    // replay = state the supervisor re-sends to a new worker: the store already holds it (and the app may have changed it since).
    securestore: (s, replay) => { if (!replay) for (const [k, v] of Object.entries(s)) { if (v == null) { const o = store.read(); delete o[storeKey(k)]; store.write(o); } else storeSet(v, k); } },
    clipboard: (s) => { setClip(s.text ?? ''); if (sim.clipEm) rt.guard(() => sim.clipEm.emit('onClipboardChanged', { contentTypes: ['plain-text'] }))(); },
    location: (s) => {
      for (const k of Object.keys(sim.pos)) if (s[k] !== undefined) sim.pos[k] = s[k];
      for (const [id, em] of sim.watchers) pushFix(id, em);
    },
    battery: (s) => { // {level 0..1, state 0 unknown|1 unplugged|2 charging|3 full, lowPowerMode}: updates the module and emits its change events
      const b = sim.battery, ev = { level: ['Expo.batteryLevelDidChange', 'batteryLevel'], state: ['Expo.batteryStateDidChange', 'batteryState'], lowPowerMode: ['Expo.powerModeDidChange', 'lowPowerMode'] };
      for (const [k, [e, f]] of Object.entries(ev)) if (s[k] !== undefined && s[k] !== b[k]) { b[k] = s[k]; emitOn('ExpoBattery', e, { [f]: b[k] }); }
    },
    localauth: (s) => { sim.localauth = { success: s.success !== false, error: s.error }; }, // next authenticateAsync result
    permissions: (s) => {
      for (const [k, v] of Object.entries(s)) {
        if (['granted', 'denied', 'undetermined'].includes(v)) sim.perms[k] = v; else log(`OneSim: simulate permissions: bad status ${JSON.stringify(v)} for ${k}`);
      }
    },
  };

  // Linking (iOS): the launch URL (`SIMCORE_LAUNCH_URL`, `create_session` `launch_url`) and URLs opened later (host
  // `open_url`, or the app opening one of its own schemes) reach RN's `url` event and expo-linking's `onURLReceived`.
  // URLs for other apps (Safari, Phone, Mail, Settings, ...) are handed to the simulated system: `module.called`
  // {module: 'Linking', method: 'openURL', args: [url]}. A scheme no app handles is refused, as on iOS.
  const link = { initial: cfg.launchURL || null, latest: cfg.launchURL || null };
  const SYSTEM_SCHEMES = new Set(['http', 'https', 'tel', 'telprompt', 'sms', 'mailto', 'facetime', 'facetime-audio', 'maps', 'itms-apps', 'itms-appss', 'app-settings', 'shortcuts', 'music', 'calshow', 'x-apple-reminderkit']);
  const ownSchemes = () => {
    const c = (rt.manifest && rt.manifest.extra && rt.manifest.extra.expoClient) || {};
    // An Expo Go / dev-client experience also answers to exp(s):// (its `experienceUrl`).
    return new Set([...[].concat(c.scheme || []), ...(c.ios && c.ios.bundleIdentifier ? [c.ios.bundleIdentifier] : []), ...(rt.manifest ? ['exp', 'exps'] : [])].map((x) => String(x).toLowerCase()));
  };
  // NSURL(string:) refuses what is not a URL (no scheme, spaces, ...): openURL / canOpenURL reject then.
  const opener = (u, method) => {
    const url = String(u ?? '');
    const m = /^([a-z][a-z0-9+.-]*):\S*$/i.exec(url);
    if (!m) throw Object.assign(new Error(`Unable to open URL: ${url}`), { code: 'EUNSPECIFIED', method });
    const scheme = m[1].toLowerCase();
    return ownSchemes().has(scheme) ? 'self' : SYSTEM_SCHEMES.has(scheme) ? 'system' : null;
  };
  const receiveURL = (url) => {
    link.latest = url;
    rt.emit('url', { url });
    rt.guard(() => instance(byName.get('ExpoLinking')).emit('onURLReceived', { url }))();
  };
  const open = (u) => {
    const to = opener(u, 'openURL');
    if (!to) throw Object.assign(new Error(`Unable to open URL: ${u}`), { code: 'EUNSPECIFIED' });
    if (to === 'self') node.setImmediate(() => receiveURL(String(u))); // iOS hands the app its own URL right back
    else sendEvent('module.called', { module: 'Linking', method: 'openURL', args: [String(u)] });
    return true;
  };

  // ActionSheetIOS: the sheet is a `dialog` layer (no anchor: the centred card iOS 26+ shows on iPhone; with `anchor`, a
  // popover at that view), the share sheet a `share` layer (protocol W). The host draws them; taps come back as `action`.
  const processed = (c) => (typeof c === 'number' ? { r: ((c >>> 16) & 255) / 255, g: ((c >>> 8) & 255) / 255, b: (c & 255) / 255, a: ((c >>> 24) & 255) / 255 } : null);
  const styleOf = (o) => (o.userInterfaceStyle === 'light' || o.userInterfaceStyle === 'dark' ? { appearance: o.userInterfaceStyle } : {});
  // UIActivityType of each share-sheet cell (`look::ShareLook`).
  const ACTIVITY = { Copy: 'com.apple.UIKit.activity.CopyToPasteboard', Reminders: 'com.apple.reminders.sharingextension', Safari: 'com.apple.mobilesafari.activity.openInSafari',
    'Open in Safari': 'com.apple.mobilesafari.activity.openInSafari', 'Add to Reading List': 'com.apple.UIKit.activity.AddToReadingList', 'Save to Files': 'com.apple.CloudDocsUI.AddToiCloudDrive' };
  const actionSheets = () => ({
    showActionSheetWithOptions: (o, cb) => {
      o = o || {};
      const n = ++alertSeq, run = new Map(), done = (i) => () => cb && cb(i);
      const opts = Array.isArray(o.options) ? o.options : [];
      const destructive = new Set([].concat(o.destructiveButtonIndices ?? o.destructiveButtonIndex ?? []));
      const disabled = new Set([].concat(o.disabledButtonIndices ?? []));
      const cancel = Number.isInteger(o.cancelButtonIndex) && o.cancelButtonIndex >= 0 && o.cancelButtonIndex < opts.length ? o.cancelButtonIndex : null;
      const actions = opts.map((text, i) => {
        const action = n * 1000 + i, id = `sheet${n}#${i}`;
        run.set(action, done(i));
        const role = i === cancel ? 'cancel' : destructive.has(i) ? 'destructive' : undefined;
        // `tintColor` colours the plain buttons (and cancel, unless `cancelButtonTintColor`); destructive stays red.
        const tint = role === 'destructive' ? null : processed(role === 'cancel' && o.cancelButtonTintColor != null ? o.cancelButtonTintColor : o.tintColor);
        const label = { kind: 'text', id: `${id}#label`, text: String(text), ...(tint && { modifiers: [{ foregroundColor: tint }] }) };
        return { kind: 'button', id, action, ...(role && { role }), ...(disabled.has(i) && { disabled: true }), label };
      });
      // A tap outside the card cancels, when there is a cancel button; without one only a button closes it.
      const dismiss = cancel != null ? n * 1000 + 999 : undefined;
      if (dismiss) run.set(dismiss, done(cancel));
      const title = o.title == null ? '' : String(o.title);
      const layer = { kind: 'dialog', title, titleVisible: title !== '', actions, ...(dismiss && { dismiss }), ...styleOf(o),
        ...(o.message != null && { message: { kind: 'text', id: `sheet${n}#message`, text: String(o.message) } }),
        ...(Number.isInteger(o.anchor) && { anchor: String(o.anchor) }) };
      rt.alerts.push({ layer, run, kind: 'sheet' });
      rt.scheduleEmit();
    },
    showShareActionSheetWithOptions: (o, failure, success) => {
      o = o || {};
      const items = [o.message, o.url].filter((x) => typeof x === 'string' && x !== '');
      if (!items.length) { if (failure) failure({ message: 'No `url` or `message` to share' }); return; }
      const n = ++alertSeq, run = new Map(), excluded = new Set(o.excludedActivityTypes || []);
      const finish = (completed, type) => () => { if (success) success(completed, type); };
      const activities = {};
      let next = n * 1000;
      for (const name of ['Reminders', 'Safari', 'More', 'Save to Files', 'Open in Safari', 'Add to Reading List']) {
        if (ACTIVITY[name] && excluded.has(ACTIVITY[name])) continue; // the cell stays, but does nothing
        const action = ++next;
        activities[name] = action;
        // Handed to the system (another app or the More list): reported like openURL; More shares nothing yet.
        run.set(action, () => { sendEvent('module.called', { module: 'ShareSheet', method: name, args: items }); finish(name !== 'More', name === 'More' ? null : ACTIVITY[name])(); });
      }
      const copy = excluded.has(ACTIVITY.Copy) ? undefined : ++next;
      if (copy) run.set(copy, () => { setClip(o.url || o.message); finish(true, ACTIVITY.Copy)(); });
      const dismiss = ++next;
      run.set(dismiss, finish(false, null));
      const layer = { kind: 'share', items, ...(o.subject != null && { subject: String(o.subject) }), ...(o.message != null && { message: String(o.message) }),
        url: items.length === 1 && typeof o.url === 'string' && /^[a-z][a-z0-9+.-]*:/i.test(o.url), dismiss, ...(copy && { copy }), activities,
        ...(Number.isInteger(o.anchor) && { anchor: String(o.anchor) }) };
      rt.alerts.push({ layer, run, kind: 'sheet' });
      rt.scheduleEmit();
    },
    // The topmost action / share sheet goes away without calling back (RCTActionSheetManager); none up: a warning.
    dismissActionSheet: () => {
      const i = rt.alerts.map((a) => a.kind).lastIndexOf('sheet');
      if (i < 0) return log('OneSim: ActionSheetIOS.dismissActionSheet: no action sheet is shown');
      rt.alerts.splice(i, 1);
      rt.scheduleEmit();
    },
  });

  // expo-asset: assets download into Caches as ExponentAsset-<md5>.<type> (AssetModule.swift); a cached file is reused
  // while its md5 matches.
  const md5 = (b) => { try { return crypto.createHash('md5').update(b).digest('hex'); } catch (_) { return crypto.createHash('sha1').update(b).digest('hex'); } };
  const assetDownload = async (url, hash, type) => {
    url = String(url);
    if (/^file:/.test(url)) return url;
    const caches = path.join(cfg.dataDir, 'Caches'), file = path.join(caches, `ExponentAsset-${hash || md5(url)}.${type}`);
    const have = fs.existsSync(file) && (!hash || md5(fs.readFileSync(file)) === hash);
    if (!have) {
      let buf;
      try { buf = await readUri(url); } catch (e) { throw Object.assign(new Error(`Unable to download asset from url: '${url}'`), { code: 'ERR_UNABLE_TO_DOWNLOAD_ASSET' }); }
      fs.mkdirSync(caches, { recursive: true });
      fs.writeFileSync(file, buf);
    }
    return require('node:url').pathToFileURL(file).href;
  };
  // expo-font: alias -> font files. The host registers the files and resolves `fontFamily` (alias, or a family /
  // PostScript name in a file) from the root's `__simcoreFonts`.
  const fonts = new Map();
  const FONT_MAGIC = ['00010000', '4f54544f', '74727565', '74746366']; // TrueType, OTTO, 'true', 'ttcf'
  const fontFile = (uri) => {
    const u = String(uri ?? ''), p = /^file:/.test(u) ? require('node:url').fileURLToPath(u) : u;
    let head = '';
    try { head = fs.readFileSync(p).subarray(0, 4).toString('hex'); } catch (_) { /* missing */ }
    if (!FONT_MAGIC.includes(head)) throw Object.assign(new Error(`Could not create font from loaded data for '${u}'`), { code: 'ERR_FONT_CREATION_FAILED' });
    return p;
  };
  const setFonts = (alias, paths) => {
    fonts.set(String(alias), paths);
    rt.rootProps.__simcoreFonts = Object.fromEntries(fonts);
    rt.scheduleEmit();
  };

  const entries = [
    // ---- React Native core (TurboModules) ----
    { name: 'PlatformConstants', status: 'implemented', constants: () => ({
      isTesting: false, isDisableAnimations: false, reactNativeVersion: { major: +ver[1], minor: +ver[2], patch: +ver[3] },
      ...(android
        ? { Version: ANDROID_API_LEVEL, Release: androidVersion, Model: androidModel, Brand: 'google', Manufacturer: 'Google', Serial: 'unknown', Fingerprint: 'unknown', uiMode: 'normal' }
        : { forceTouchAvailable: false, osVersion: OS_VERSION, systemName: 'iOS', interfaceIdiom: 'phone' }),
    }) },
    // DeviceInfo.Dimensions feeds RN's Dimensions / useWindowDimensions (session viewport); host `environment` updates it.
    { name: 'DeviceInfo', status: 'simulated', constants: () => ({ Dimensions: rt.dimensions() }) },
    { name: 'Appearance', status: 'simulated', methods: { getColorScheme: () => rt.appearance, ...listener }, events: ['appearanceChanged'] },
    { name: 'SourceCode', status: 'implemented', constants: () => ({ scriptURL: rt.scriptURL }) },
    { name: 'I18nManager', status: 'implemented', constants: { isRTL: false, doLeftAndRightSwapInRTL: true, localeIdentifier: 'en_US' }, methods: { allowRTL: noop, forceRTL: noop, swapLeftAndRightInRTL: noop } },
    { name: 'StatusBarManager', status: 'implemented', constants: () => ({ HEIGHT: android ? cfg.androidInsets.top : 54, DEFAULT_BACKGROUND_COLOR: 0, STYLE: 'default' }),
      methods: { ...listener, getHeight: (cb) => cb({ height: 54 }),
        setStyle: (s) => {
          if (!android) return log('OneSim: StatusBar.setStyle', s);
          rt.rootProps.__simcoreStatusBarStyle = s;
          rt.scheduleEmit();
        },
        setHidden: (hidden) => {
          if (!android) return;
          rt.rootProps.__simcoreStatusBarHidden = hidden;
          rt.scheduleEmit();
        }, setNetworkActivityIndicatorVisible: noop } },
    { name: 'ExceptionsManager', status: 'implemented', methods: { reportException, reportFatalException: (m, s) => reportException({ message: m, stack: s }), reportSoftException: (m) => log('OneSim soft exception:', m), updateExceptionMessage: noop, dismissRedbox: noop } },
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
      getSize: (uri) => loadImage(uri).then((i) => android ? { width: i.width, height: i.height } : [i.width, i.height]),
      getSizeWithHeaders: (uri, headers) => loadImage(uri, headers).then((i) => ({ width: i.width, height: i.height })),
      prefetchImage: (uri) => loadImage(uri).then(() => true, () => false),
      prefetchImageWithMetadata: (uri) => loadImage(uri).then(() => true, () => false),
      queryCache: (uris) => Promise.resolve(Object.fromEntries(uris.filter((u) => imageDone.has(u)).map((u) => [u, 'disk']))),
      abortRequest: noop } },
    { name: 'LinkingManager', status: android ? 'implemented' : 'simulated', async: true, events: ['url'], methods: android
      ? { getInitialURL: () => Promise.resolve(null), canOpenURL: () => Promise.resolve(false), openURL: UNSUP, openSettings: UNSUP, ...listener }
      : { getInitialURL: async () => link.initial, canOpenURL: async (u) => !!opener(u, 'canOpenURL'), openURL: async (u) => open(u), openSettings: async () => open('app-settings:'), ...listener } },
    ...(android ? [{ name: 'IntentAndroid', status: 'implemented', async: true,
      methods: { getInitialURL: async () => null, canOpenURL: async () => false, openURL: UNSUP, openSettings: UNSUP, sendIntent: UNSUP } }] : []),
    // JS defaults apply for every flag when the native getter returns undefined.
    { name: 'NativeReactNativeFeatureFlagsCxx', status: 'implemented', make: () => new Proxy({}, { get: (t, p) => (typeof p === 'string' ? () => undefined : undefined) }) },
    // Portrait insets by viewport height (62/34 on the 402x874 iPhone 17 Pro); the Rust host owns the real chrome.
    { name: 'RNCSafeAreaContext', package: 'react-native-safe-area-context', status: 'simulated', constants: () => {
      const insets = insetsFor(rt.window.height);
      return { initialWindowMetrics: { frame: { x: 0, y: 0, width: rt.window.width, height: rt.window.height }, insets, verticalCornerAdaptedInsets: insets, horizontalCornerAdaptedInsets: insets } };
    } },
    { name: 'RNSModule', package: 'react-native-screens', status: 'implemented' }, // empty spec, presence only
    { name: 'ExpoModulesCore', package: 'expo-modules-core', status: 'implemented', methods: { installModules: noop } },
    ...Object.entries(rt.worklets.modules).map(([name, make]) => ({ name, package: name === 'WorkletsModule' ? 'react-native-worklets' : 'react-native-reanimated', status: 'implemented', make })),
    { name: 'RNGestureHandlerModule', package: 'react-native-gesture-handler', status: 'implemented', make: () => rt.gestures.module },
    { name: 'NativeDOMCxx', status: 'implemented', make: () => rt.domModule() },
    { name: 'EventDispatcher', status: 'implemented' },
    // RN modules with platform behaviour we do not implement: every call is a diagnostic (and an error for the caller).
    ...(android ? [] : [{ name: 'ActionSheetManager', status: 'implemented', strict: true, methods: actionSheets() }]),
    // Alert.alert: an `alert` presentation layer the Rust host draws like SwiftUI's `.alert`; a button tap comes back as `action`.
    { name: 'AlertManager', status: 'implemented', strict: true, methods: { alertWithArgs: (a, cb) => {
      // ponytail: Alert.prompt (text fields) is not drawn; add a text-field layer when apps need it.
      if (a.type && a.type !== 'default') throw unsupported('AlertManager', `alertWithArgs (type: ${a.type}, Alert.prompt)`);
      const n = ++alertSeq, run = new Map();
      const destructive = a.destructiveButtonKeys || [a.destructiveButtonKey];
      const actions = (a.buttons && a.buttons.length ? a.buttons : [{ 0: 'OK' }]).map((b, i) => {
        const [k, text] = Object.entries(b)[0], action = n * 1000 + i, id = `alert${n}#${k}`;
        run.set(action, () => cb(+k));
        const role = k === a.cancelButtonKey ? 'cancel' : destructive.includes(k) ? 'destructive' : undefined;
        return { kind: 'button', id, action, ...(role && { role }), label: { kind: 'text', id: `${id}#label`, text } };
      });
      const layer = { kind: 'alert', title: a.title || '', actions, ...(a.message && { message: { kind: 'text', id: `alert${n}#message`, text: a.message } }) };
      rt.alerts.push({ layer, run, kind: 'alert' });
      rt.scheduleEmit();
    } } },
    { name: 'ShareModule', status: 'unsupported', async: true, methods: all(['share']) },
    { name: 'PushNotificationManager', status: 'unsupported', async: true },
    { name: 'ImageStoreManager', status: 'unsupported', async: true },
    { name: 'AccessibilityManager', status: 'unsupported' },
    { name: 'DevToolsSettingsManager', status: 'unsupported' },
    { name: 'Vibration', status: 'simulated', methods: called('Vibration', { vibrate: noop, cancel: noop }) },
    { name: 'ImageEditor', status: 'unsupported', async: true },
    // RN's Clipboard TurboModule (RN <= 0.8x NativeClipboard): the same in-memory clipboard as expo-clipboard.
    { name: 'Clipboard', status: 'simulated', strict: true, async: true, methods: called('Clipboard', { getString: async () => sim.clip, setString: (t) => { setClip(t); } }) },

    // ---- expo modules (globalThis.expo.modules) ----
    { name: 'ExpoFetchModule', package: 'expo', kind: 'expo', status: 'implemented', methods: () => rt.expoFetch(SharedObject) },
    { name: 'FileSystem', package: 'expo-file-system', kind: 'expo', status: 'implemented', async: true, events: ['downloadProgress'], constants: files.constants,
      methods: { ...files.methods, ...all(['downloadFileAsync', 'cancelDownloadAsync', 'pickDirectoryAsync', 'pickFileAsync', 'canPreview', 'preview']) } },
    { name: 'ExponentFileSystem', package: 'expo-file-system', kind: 'expo', status: 'implemented', async: true, events: ['expo-file-system.downloadProgress', 'expo-file-system.uploadProgress'], constants: files.legacyConstants,
      methods: { ...files.legacy, ...all(['downloadAsync', 'uploadAsync', 'uploadTaskStartAsync', 'downloadResumableStartAsync', 'downloadResumablePauseAsync', 'networkTaskCancelAsync']) } },
    { name: 'ExpoAsset', package: 'expo-asset', kind: 'expo', status: 'implemented', methods: { downloadAsync: assetDownload } },
    // iOS registers no isLoaded / unloadAsync natively (expo-font's JS answers isLoaded from getLoadedFonts).
    { name: 'ExpoFontLoader', package: 'expo-font', kind: 'expo', status: 'implemented', strict: true, methods: {
      getLoadedFonts: () => [...fonts.keys()],
      loadAsync: async (alias, localUri) => { setFonts(alias, [fontFile(localUri)]); },
      loadFontFamilyAsync: async (alias, faces) => {
        if (!Array.isArray(faces) || !faces.length) throw Object.assign(new Error(`Font family '${alias}' has no faces`), { code: 'ERR_FONT_FAMILY_EMPTY_FACES' });
        setFonts(alias, [...new Set(faces.map((f) => {
          if (!f || !f.localUri) throw Object.assign(new Error(`A face of font family '${alias}' has no local URI`), { code: 'ERR_FONT_FACE_MISSING_URI' });
          return fontFile(f.localUri);
        }))]);
      } } },
    // Screen-awake is a no-op in a simulator surface; accepted, not rendered.
    { name: 'ExpoKeepAwake', package: 'expo-keep-awake', kind: 'expo', status: 'implemented', methods: { activate: async () => {}, deactivate: async () => {}, isActivated: async () => false } },
    // expo-splash-screen: the native launch screen is not part of the OneSim surface, so hiding it has nothing to do.
    { name: 'ExpoSplashScreen', package: 'expo-splash-screen', kind: 'expo', status: 'implemented', methods: { setOptions: () => {}, hide: () => {}, hideAsync: async () => {}, preventAutoHideAsync: async () => true, internalPreventAutoHideAsync: async () => {}, internalMaybeHideAsync: async () => {} } },
    // expo-system-ui: the root view's background colour is a prop of the tree's root node (processed colour int, like any view).
    { name: 'ExpoSystemUI', package: 'expo-system-ui', kind: 'expo', status: 'implemented', methods: {
      setBackgroundColorAsync: async (c) => { if (c == null) delete rt.rootProps.backgroundColor; else rt.rootProps.backgroundColor = c; rt.scheduleEmit(); },
      getBackgroundColorAsync: async () => (rt.rootProps.backgroundColor == null ? null : '#' + (rt.rootProps.backgroundColor >>> 0).toString(16).padStart(8, '0').replace(/^(..)(......)$/, '$2$1')) } },
    { name: 'ExpoDevice', package: 'expo-device', kind: 'expo', status: 'simulated',
      constants: android
        ? { isDevice: false, brand: 'google', manufacturer: 'Google', modelId: null, modelName: androidModel, deviceYearClass: 2026, totalMemory: 8 * 1024 ** 3,
          osName: 'Android', osVersion: androidVersion, platformApiLevel: ANDROID_API_LEVEL, osBuildId: null, osInternalBuildId: null, deviceName: androidModel, deviceType: 1, supportedCpuArchitectures: ['arm64-v8a'] }
        : { isDevice: false, brand: 'Apple', manufacturer: 'Apple', modelId: MODEL_ID, modelName: MODEL_NAME, deviceYearClass: YEAR_CLASS, totalMemory: 12 * 1024 ** 3,
          osName: 'iOS', osVersion: OS_VERSION, osBuildId: OS_BUILD, osInternalBuildId: OS_BUILD, deviceName: 'iPhone', deviceType: 1, supportedCpuArchitectures: ['arm64e', 'arm64'] },
      methods: { getDeviceTypeAsync: async () => 1, getUptimeAsync: async () => node.performance.now(), isRootedExperimentalAsync: async () => false } },
    // expo-linking: no launch URL, so Expo Router falls back to the scheme root and lands on `index`. Opening URLs is unsupported.
    { name: 'ExpoLinking', package: 'expo-linking', kind: 'expo', status: android ? 'implemented' : 'simulated', events: ['onURLReceived'],
      methods: { getLinkingURL: () => (android ? null : link.latest), clearInitialURL: () => { link.latest = null; } } },
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
        appOwnership: null, executionEnvironment: 'bare', sessionId: cfg.session || 'onesim', installationId: 'simcore',
        statusBarHeight: android ? cfg.androidInsets.top : 54, systemFonts: [], isDevice: false, deviceName: android ? androidModel : 'iPhone',
        expoVersion: null, platform: android ? { android: { versionCode: null } } : { ios: { model: 'iPhone', systemVersion: OS_VERSION } } }),
      methods: { getWebViewUserAgentAsync: async () => android
        ? `Mozilla/5.0 (Linux; Android ${androidVersion}; ${androidModel}) OneSim`
        : `Mozilla/5.0 (iPhone; CPU iPhone OS ${OS_VERSION.replace('.', '_')} like Mac OS X) OneSim` } },

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
    { name: 'ExpoSecureStore', package: 'expo-secure-store', kind: 'expo', status: 'simulated', strict: true,
      constants: { AFTER_FIRST_UNLOCK: 0, AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY: 1, ALWAYS: 2, WHEN_PASSCODE_SET_THIS_DEVICE_ONLY: 3, ALWAYS_THIS_DEVICE_ONLY: 4, WHEN_UNLOCKED: 5, WHEN_UNLOCKED_THIS_DEVICE_ONLY: 6 },
      methods: {
        getValueWithKeyAsync: async (k, o) => storeGet(k, o), getValueWithKeySync: storeGet,
        setValueWithKeyAsync: async (v, k, o) => storeSet(v, k, o), setValueWithKeySync: storeSet,
        deleteValueWithKeyAsync: async (k, o) => { const s = store.read(); delete s[storeKey(k, o)]; store.write(s); },
        canUseBiometricAuthentication: () => false } },
    // expo-localization / -crypto / -application / -network / -battery / -brightness / -screen-orientation / -store-review / -tracking-transparency / -local-authentication
    { name: 'ExpoLocalization', package: 'expo-localization', kind: 'expo', status: 'implemented', events: ['onLocaleSettingsChanged', 'onCalendarSettingsChanged'],
      methods: {
        getLocales: () => [{ languageTag: 'en-US', languageCode: 'en', languageScriptCode: null, regionCode: 'US', languageRegionCode: 'US', currencyCode: 'USD', currencySymbol: '$', languageCurrencyCode: 'USD', languageCurrencySymbol: '$',
          decimalSeparator: '.', digitGroupingSeparator: ',', textDirection: 'ltr', measurementSystem: 'us', temperatureUnit: 'fahrenheit' }],
        getCalendars: () => [{ calendar: 'gregory', uses24hourClock: false, firstWeekday: 1, timeZone: process.env.TZ || 'America/Los_Angeles' }] } },
    { name: 'ExpoCrypto', package: 'expo-crypto', kind: 'expo', status: 'implemented', // ExpoCryptoAES (expo-crypto/aes) is not provided
      methods: {
        digestStringAsync: async (alg, data, o) => hash(alg, Buffer.from(String(data), 'utf8')).digest((o && o.encoding) || 'hex'),
        digestAsync: async (alg, data) => new Uint8Array(hash(alg, ArrayBuffer.isView(data) ? Buffer.from(data.buffer, data.byteOffset, data.byteLength) : Buffer.from(data)).digest()).buffer,
        getRandomValues: (a) => { crypto.randomFillSync(a); return a; }, randomUUID: () => crypto.randomUUID() } },
    { name: 'ExpoCryptoAES', package: 'expo-crypto', kind: 'expo', status: 'unsupported', // expo-crypto/aes: the classes exist so the package imports; every call diagnoses
      make: () => { const no = (m) => () => Promise.reject(unsupported('ExpoCryptoAES', m, 'expo-crypto')); const cls = (n, ...ms) => { const c = class extends SharedObject {}; for (const m of ms) c[m] = no(`${n}.${m}`); return c; };
        return { EncryptionKey: cls('EncryptionKey', 'generate', 'import'), SealedData: cls('SealedData', 'fromParts', 'fromCombined'), encryptAsync: no('encryptAsync'), decryptAsync: no('decryptAsync') }; } },
    { name: 'ExpoApplication', package: 'expo-application', kind: 'expo', status: 'simulated',
      constants: () => { const c = (rt.manifest && rt.manifest.extra && rt.manifest.extra.expoClient) || {}; return { applicationName: c.name || 'OneSim App', applicationId: (c.ios && c.ios.bundleIdentifier) || 'host.exp.Exponent', nativeApplicationVersion: c.version || '1.0.0', nativeBuildVersion: (c.ios && c.ios.buildNumber) || '1' }; },
      methods: { getIosIdForVendorAsync: async () => '00000000-0000-4000-8000-000000000001', getInstallationTimeAsync: async () => fs.statSync(cfg.dataDir).birthtimeMs, getLastUpdateTimeAsync: async () => fs.statSync(cfg.dataDir).birthtimeMs } },
    { name: 'ExpoNetwork', package: 'expo-network', kind: 'expo', status: 'simulated', events: ['onNetworkStateChanged'],
      methods: { getNetworkStateAsync: async () => ({ type: 'WIFI', isConnected: true, isInternetReachable: true }), getIpAddressAsync: async () => '192.168.1.2', isAirplaneModeEnabledAsync: async () => false } },
    { name: 'ExpoBattery', package: 'expo-battery', kind: 'expo', status: 'simulated', constants: { isSupported: true }, events: ['Expo.batteryLevelDidChange', 'Expo.batteryStateDidChange', 'Expo.powerModeDidChange'],
      methods: { getBatteryLevelAsync: async () => sim.battery.level, getBatteryStateAsync: async () => sim.battery.state, isLowPowerModeEnabledAsync: async () => sim.battery.lowPowerMode } },
    { name: 'ExpoBrightness', package: 'expo-brightness', kind: 'expo', status: 'simulated', events: ['Expo.brightnessDidChange'], permissions: ['brightness'],
      methods: {
        getBrightnessAsync: async () => sim.brightness, setBrightnessAsync: async (v) => { sim.brightness = v; emitOn('ExpoBrightness', 'Expo.brightnessDidChange', { brightness: v }); },
        getPermissionsAsync: async () => granted, requestPermissionsAsync: async () => granted } }, // Android-only system-brightness calls are absent (JS guards them by platform)
    { name: 'ExpoScreenOrientation', package: 'expo-screen-orientation', kind: 'expo', status: 'simulated', // portrait only: any other lock rejects
      methods: {
        getOrientationAsync: async () => 1, getOrientationLockAsync: async () => sim.lock, supportsOrientationLockAsync: async (l) => PORTRAIT_LOCKS.includes(l),
        lockAsync: async (l) => { if (!PORTRAIT_LOCKS.includes(l)) throw unsupported('ExpoScreenOrientation', `lockAsync(${l})`, 'expo-screen-orientation'); sim.lock = l; },
        lockPlatformAsync: async () => { throw unsupported('ExpoScreenOrientation', 'lockPlatformAsync', 'expo-screen-orientation'); }, getPlatformOrientationLockAsync: async () => ({ screenOrientationArrayIOS: [1] }) } },
    { name: 'ExpoStoreReview', package: 'expo-store-review', kind: 'expo', status: 'simulated',
      methods: { isAvailableAsync: async () => true, ...called('ExpoStoreReview', { requestReview: async () => {} }) } },
    { name: 'ExpoTrackingTransparency', package: 'expo-tracking-transparency', kind: 'expo', status: 'simulated', permissions: ['tracking'], // `simulate permissions {tracking}`
      methods: { getPermissionsAsync: async () => permission('tracking'), requestPermissionsAsync: async () => ask('tracking'), getAdvertisingId: () => ZERO_UUID, getAdvertisingIdAsync: async () => ZERO_UUID } },
    { name: 'ExpoLocalAuthentication', package: 'expo-local-authentication', kind: 'expo', status: 'simulated', // result controlled by `simulate localauth`
      methods: {
        hasHardwareAsync: async () => true, isEnrolledAsync: async () => true, supportedAuthenticationTypesAsync: async () => [2], getEnrolledLevelAsync: async () => 3, cancelAuthenticate: noop,
        ...called('ExpoLocalAuthentication', { authenticateAsync: async () => (sim.localauth.success ? { success: true } : { success: false, error: sim.localauth.error || 'user_cancel' }) }) } },
    ...asyncStorage,
    { name: 'ExpoCamera', package: 'expo-camera', kind: 'expo', status: 'unsupported', permissions: ['camera', 'microphone'],
      methods: all(['getCameraPermissionsAsync', 'requestCameraPermissionsAsync', 'getMicrophonePermissionsAsync', 'requestMicrophonePermissionsAsync', 'isAvailableAsync', 'getAvailableVideoCodecsAsync', 'scanFromURLAsync']) },
    // ---- Expo view modules drawn by the host (layout/rn.rs); props whitelisted in host.js EXPO_VIEWS ----
    { name: 'ExpoLinearGradient', package: 'expo-linear-gradient', kind: 'expo', status: 'implemented', constants: { ViewPrototypes: {} } },
    { name: 'ExpoBlur', package: 'expo-blur', kind: 'expo', status: 'implemented', constants: { ViewPrototypes: {} } },
    { name: 'ExpoUI', package: '@expo/ui', kind: 'expo', status: 'implemented', constants: () => require('./expo-ui.js').moduleConstants(cfg, rt, SharedObject) },
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
        if (!ABSENT.has(name) && !missed.has(`t:${name}`)) log(`OneSim: TurboModule "${name}" is not available (null, as on a binary without it)`);
        missed.add(`t:${name}`);
        return null;
      },
    },
    // globalThis.expo.modules: unknown modules are *absent* (undefined) so requireNativeModule throws / requireOptionalNativeModule is null.
    expoModules: new Proxy({}, {
      get: (t, name) => {
        if (typeof name === 'symbol') return undefined;
        const e = byName.get(name);
        if (!e || e.kind !== 'expo') { if (!missed.has(`e:${name}`)) log(`OneSim: expo module "${name}" is not available`); missed.add(`e:${name}`); return undefined; }
        return instance(e);
      },
    }),
    list: () => entries.map((e) => ({
      name: e.name, package: e.package, status: e.status,
      methods: Object.entries(e.make ? {} : implsOf(e)).filter(([k, v]) => typeof v === 'function' || v === UNSUP).map(([k]) => k).filter((k) => !HIDDEN.has(k)),
      events: e.events || [], permissions: e.permissions || [],
    })),
    // Host `open_url`: the system opens `url` in the app.
    openURL(url) { if (!android) receiveURL(String(url)); else log('OneSim: open_url is iOS-only for React Native'); },
    simulate(module, state, replay) {
      const f = simulators[String(module).toLowerCase()];
      if (f) f(state || {}, replay); else log(`OneSim: simulate: no simulated module "${module}" (have: ${Object.keys(simulators).join(', ')})`);
    },
  };
};
