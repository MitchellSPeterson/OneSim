// Issue #50 check (React Native gaps): node frameworks/react-native/guest/run-apis.mjs [port=8232]
// Boots examples/expo-apis and asserts what screenshots cannot show: LayoutAnimation's per-commit root prop and its
// end callback, ActionSheetIOS layers and callbacks (button, tap outside, dismissActionSheet, share sheet), Linking
// (launch URL, own-scheme round trip, system hand-off, canOpenURL, rejections, host `open_url`), expo-font (the asset
// downloaded into Caches, `__simcoreFonts`) and GIF / WebP image loads.
// A Metro already listening on the port is reused (and left running).
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { root, run, startHost, startMetro, walk } from './lib.mjs';

const port = Number(process.argv[2]) || 8232;
const ok = (m) => console.log('ok:', m);
const expect = (c, m) => { if (!c) throw new Error(m); };

run(async () => {
  await startMetro(join(root, 'examples/expo-apis'), port);
  const launch = 'expoapis://welcome?from=test';
  const trees = [];
  const h = startHost({ SIMCORE_METRO_URL: `http://localhost:${port}`, SIMCORE_VIEWPORT: '402x874', SIMCORE_HMR: '0', SIMCORE_LAUNCH_URL: launch }, (_, m) => trees.push(m));
  const home = async () => { h.tap('Back'); await h.until(() => /^APIs/.test(h.text), 'home'); };
  await h.until(() => /^APIs/.test(h.text), 'first tree');
  const mods = Object.fromEntries(h.modules[0].map((m) => [m.name, m]));
  expect(mods.ActionSheetManager?.status === 'implemented', 'ActionSheetManager implemented');
  expect(mods.LinkingManager?.status === 'simulated' && mods.ExpoLinking?.status === 'simulated', 'Linking simulated');
  expect(mods.ExpoFontLoader?.methods.includes('loadFontFamilyAsync'), 'ExpoFontLoader.loadFontFamilyAsync');

  // ---- LayoutAnimation: the configured commit carries the config once, with a fresh token; onEnd runs after it.
  h.tap('Layout>');
  await h.until(() => /Animations ended: 0/.test(h.text), 'Layout screen');
  const n0 = trees.length;
  h.tap('Toggle');
  await h.until(() => /Expanded/.test(h.text), 'expanded');
  const anim = trees.slice(n0).map((t) => t.root.props?.__simcoreLayoutAnimation ?? t.root.content?.props?.__simcoreLayoutAnimation).filter(Boolean);
  expect(anim.length === 1, `one animated tree, got ${anim.length}`);
  expect(anim[0].duration === 300 && anim[0].update?.type === 'easeInEaseOut' && anim[0].create?.property === 'opacity' && anim[0].token > 0, 'easeInEaseOut preset: ' + JSON.stringify(anim[0]));
  await h.until(() => /Animations ended: 1/.test(h.text), 'onAnimationDidEnd', 3000);
  h.tap('Spring');
  await h.until(() => /Collapsed/.test(h.text), 'collapsed');
  const spring = trees.slice(n0).map((t) => t.root.props?.__simcoreLayoutAnimation).filter(Boolean);
  expect(spring.length === 2 && spring[1].update?.type === 'spring' && spring[1].update.springDamping === 0.4 && spring[1].token > spring[0].token, 'spring preset: ' + JSON.stringify(spring[1]));
  await h.until(() => /Animations ended: 2/.test(h.text), 'spring end', 3000);
  ok(`LayoutAnimation: tokens ${spring.map((a) => a.token)}, both ended`);
  await home();

  // ---- ActionSheetIOS
  h.tap('Sheet>');
  await h.until(() => /Picked: none/.test(h.text), 'Sheet screen');
  h.tap('Show sheet');
  await h.until(() => h.layers.length === 1, 'action sheet layer');
  let sheet = h.layers[0];
  expect(sheet.kind === 'dialog' && sheet.title === 'Photo' && sheet.titleVisible && sheet.message.text === 'Choose what to do with this photo' && !sheet.anchor, 'dialog layer: ' + JSON.stringify(sheet).slice(0, 300));
  expect(sheet.actions.map((a) => [a.label.text, a.role ?? '']).join() === 'Cancel,cancel,Delete photo,destructive,Save to Files,', 'buttons in option order with roles');
  h.send({ type: 'action', id: sheet.actions[2].action });
  await h.until(() => /Picked: 2/.test(h.text) && h.layers.length === 0, 'Save to Files -> 2');
  h.tap('Show sheet');
  await h.until(() => h.layers.length === 1, 'sheet again');
  h.send({ type: 'action', id: h.layers[0].dismiss });
  await h.until(() => /Picked: 0/.test(h.text) && h.layers.length === 0, 'tap outside -> cancel index');
  // An anchor changes nothing on iPhone: the same card (the layer names it).
  h.tap('Anchored');
  await h.until(() => h.layers.length === 1, 'anchored sheet');
  expect(/^\d+$/.test(h.layers[0].anchor || ''), 'anchor tag on the layer');
  h.send({ type: 'action', id: h.layers[0].actions[1].action });
  await h.until(() => /Picked: 1/.test(h.text) && h.layers.length === 0, 'Delete photo -> 1');
  // Share sheet: a link item, Copy calls back (true, CopyToPasteboard), a tap outside (false, null).
  h.tap('Share');
  await h.until(() => h.layers.length === 1, 'share sheet');
  const share = h.layers[0];
  expect(share.kind === 'share' && share.url === true && share.items.join() === 'https://onesim.invalid/share' && share.copy && share.activities.Safari, 'share layer: ' + JSON.stringify(share));
  h.send({ type: 'action', id: share.copy });
  await h.until(() => /Picked: true com\.apple\.UIKit\.activity\.CopyToPasteboard/.test(h.text) && h.layers.length === 0, 'Copy -> completed');
  h.tap('Share');
  await h.until(() => h.layers.length === 1, 'share sheet again');
  h.send({ type: 'action', id: h.layers[0].dismiss });
  await h.until(() => /Picked: false null/.test(h.text) && h.layers.length === 0, 'dismissed share -> false null');
  ok('ActionSheetIOS: button index, tap-outside cancel, anchor, share sheet Copy and dismiss come back through the callbacks');
  await home();

  // ---- expo-font: Font.loadAsync downloads the asset into Caches and the root lists the file under its alias.
  h.tap('Fonts>');
  await h.until(() => /Loaded: true/.test(h.text) && /Roboto 34/.test(h.text), 'font loaded');
  const fonts = trees[trees.length - 1].root.props.__simcoreFonts;
  expect(fonts?.Roboto?.length === 1 && /Caches\/ExponentAsset-[0-9a-f]+\.ttf$/.test(fonts.Roboto[0]) && existsSync(fonts.Roboto[0]), '__simcoreFonts: ' + JSON.stringify(fonts));
  ok(`expo-font: Roboto -> ${fonts.Roboto[0].split('/').slice(-2).join('/')}`);
  // renderToImageAsync: the host draws it; the app shows the PNG it got back.
  await h.until(() => { let ok2 = false; walk(h.tree, (v) => { if (v.component === 'RCTImageView' && /\/tmp\/[0-9A-F-]+\.png$/.test(v.props.__simcoreImage?.path ? [].concat(v.props.source)[0]?.uri : '')) ok2 = true; }); return ok2; }, 'renderToImageAsync image');
  await home();

  // ---- Images: GIF and WebP sources load (cached with their own extension) for core Image and expo-image.
  h.tap('Images>');
  await h.until(() => { let n = 0; walk(h.tree, (v) => { if (v.props.__simcoreImage) n++; }); return n === 11; }, 'eleven images loaded');
  const exts = []; walk(h.tree, (v) => { if (v.props.__simcoreImage) exts.push(v.props.__simcoreImage.path.split('.').pop()); });
  // GIF / WebP stills and animations, the ref (WebP), the repeat dot, the decoded blurhash and a placeholder (PNG).
  expect(exts.sort().join() === 'gif,gif,gif,gif,png,png,png,webp,webp,webp,webp', 'image files: ' + exts);
  ok('images: ' + exts.join(' '));
  // expo-image: a generated blurhash (the same string iOS 27.2 gives for this GIF), a blurhash source, a loadAsync ref,
  // repeat tiles, and stop / start on an autoplay={false} GIF.
  await h.until(() => /blurhash LbL3\.FJ8J8Nv67I\[1z:hv\{Nu:\}\]i/.test(h.text), 'generateBlurhashAsync');
  const imgs = []; walk(h.tree, (v) => { if (v.props.__simcoreImage) imgs.push(v); });
  expect(imgs.some((v) => v.component === 'ViewManagerAdapter_ExpoImage' && typeof v.props.source === 'number'), 'an ImageRef source (shared object id)');
  expect(imgs.some((v) => v.props.resizeMode === 'repeat'), 'repeat image loaded');
  expect(imgs.some((v) => [].concat(v.props.source)[0]?.uri?.startsWith('blurhash:/')), 'blurhash source decoded by the host');
  // No source: the placeholder draws, with placeholderContentFit.
  expect(imgs.some((v) => v.props.accessibilityLabel === 'Placeholder only' && ![].concat(v.props.source ?? []).length && v.props.contentFit === 'scale-down'), 'placeholder without a source');
  // Cached files are named by key and content, so new bytes behind the same key are a new file the host decodes.
  expect(imgs.every((v) => /\/onesim-images\/[0-9a-f]{40}-[0-9a-f]{16}\.(png|gif|webp|jpg)$/.test(v.props.__simcoreImage.path) || v.props.source?.uri?.startsWith?.('file:') || typeof v.props.source === 'number'), 'content-versioned cache files');
  const paused = () => { let a; walk(h.tree, (v) => { if (v.component === 'ViewManagerAdapter_ExpoImage' && v.props.autoplay === false) a = v.props.__simcoreAnimating; }); return a; };
  h.tap('Play');
  await h.until(() => paused() === true, 'startAnimating');
  h.tap('Pause');
  await h.until(() => paused() === false, 'stopAnimating');
  await home();

  // ---- Linking
  h.tap('Links>');
  await h.until(() => h.text.includes(`Initial: ${launch}`), 'launch URL is the initial URL');
  h.tap('Open self');
  // Outside Expo Go, expo-linking builds the URL on the app's scheme (app.json `scheme`).
  await h.until(() => /Last: expoapis:\/\/done\?n=1/.test(h.text) && /Result: self true/.test(h.text), 'own URL comes back as a url event');
  const e0 = h.events.length;
  h.tap('Open web');
  await h.until(() => /Result: web true/.test(h.text), 'web resolves');
  expect(h.events.slice(e0).some((e) => e.name === 'module.called' && e.detail.module === 'Linking' && e.detail.args[0] === 'https://expo.dev'), 'web URL handed to the system');
  await h.until(() => /States: inactive background inactive active/.test(h.text), 'AppState round trip through Safari');
  h.tap('Can tel');
  await h.until(() => /Result: tel true/.test(h.text), 'canOpenURL tel');
  h.tap('Can odd');
  await h.until(() => /Result: odd false/.test(h.text), 'canOpenURL unknown scheme');
  h.tap('Settings');
  await h.until(() => /Result: settings true/.test(h.text), 'openSettings');
  // Back from Settings (a tap while the app is away would be cancelled, as on iOS).
  await h.until(() => /States: (inactive background inactive active ?){2}Open/.test(h.text), 'AppState round trip through Settings');
  h.tap('Bad URL');
  await h.until(() => /Result: bad failed Unable to open URL: no scheme/.test(h.text), 'invalid URL rejects');
  h.send({ type: 'open_url', url: 'expoapis://from-host' });
  await h.until(() => /Last: expoapis:\/\/from-host/.test(h.text), 'host open_url reaches the url event');
  // canOpenURL on a phone: the system apps answer, an app that is not installed does not (even one Expo Go declares).
  h.tap('Probe');
  await h.until(() => /probe /.test(h.text), 'probe');
  expect(/probe https=1 http=1 tel=1 mailto=1 sms=1 facetime=1 maps=1 itms-apps=1 app-settings=1 waze=0 exp=1 expoapis=1 notinstalled=0/.test(h.text), 'canOpenURL answers: ' + /probe [^R]*/.exec(h.text)[0]);
  // Host scene phases drive AppState.
  h.send({ type: 'scene_phase', phase: 'inactive' });
  h.send({ type: 'scene_phase', phase: 'background' });
  const states = () => (/States: ([a-z ]*?)Open self/.exec(h.text) || [])[1]?.trim() || '';
  await h.until(() => states().endsWith('active inactive background'), 'scene_phase -> AppState');
  h.send({ type: 'scene_phase', phase: 'inactive' });
  h.send({ type: 'scene_phase', phase: 'active' });
  await h.until(() => states().endsWith('background inactive active'), 'back to active');
  ok('Linking: launch URL, own-scheme round trip, system hand-off, canOpenURL, rejection, host open_url');
}, 240000);
