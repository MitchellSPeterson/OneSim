# rn-apis-ios ground-truth fixtures

`examples/expo-apis` (plain React Native, Expo SDK 58: LayoutAnimation, ActionSheetIOS, GIF / WebP images through core
`Image` and expo-image, ActivityIndicator, Linking, expo-font) running in **Expo Go 58.0.7** on the iPhone 17 Pro
simulator, iOS 26.5 (402 x 874 pt, scale 3, PNGs 1206 x 2622). Compared by `scripts/compare-ios.sh` (`rr_states_apis`)
against `onesim expo`. Captured 2026-11-22.

## Capture steps
1. `cd examples/expo-apis && npm install && CI=1 npx expo start --port 8231`. Light appearance, Expo Go's Tools button
   off (see `../rn-showcase-ios/README.md`).
2. Open the project in Expo Go (`exp://<host>:8231`; from another machine use the Metro host's LAN address).
3. Each PNG: `agent-device screenshot <name>.png --pixel-density 3 --normalize-status-bar`, retaken until the Dynamic
   Island is drawn (light states). Taps are agent-device presses on the accessibility labels below.
4. Dark states: `agent-device settings appearance dark`, wait, screenshot, then back to light.

## States and OneSim equivalents (`onesim expo --url http://localhost:PORT ...`)
| state | iOS capture | OneSim |
|---|---|---|
| home / home-dark | launch | (none) / `--dark` |
| layout-expanded | Layout, Toggle, wait 2 s (`LayoutAnimation.Presets.easeInEaseOut` grows the card, inserts a row; "Animations ended: 1") | `--click Layout --click Toggle --wait 1000` |
| actionsheet | Sheet, Show sheet: `ActionSheetIOS.showActionSheetWithOptions` (title, message, Cancel index 0, Delete photo destructive) | `--click Sheet --click "Show sheet"` |
| actionsheet-picked | the sheet, then Save to Files ("Picked: 2") | `... --click "Save to Files"` |
| actionsheet-anchored | after actionsheet-picked, Anchored: the same options with `anchor` (iPhone ignores it: the same card) | `... --click Anchored` |
| actionsheet-dark | dark: Sheet, Share, tap outside ("Picked: false null"), Show sheet | `--dark --click Sheet --click Share --click 200,400 --click "Show sheet"` |
| share | Sheet, Show sheet, Cancel ("Picked: 0"), Share: `showShareActionSheetWithOptions({url: 'https://onesim.invalid/share'})` | `--click Sheet --click "Show sheet" --click Cancel --click Share` |
| share-dark | dark: Sheet, Share | `--dark --click Sheet --click Share` |
| images / -dark | Images: still GIF (transparent corner) and lossy WebP (core Image), lossless WebP with alpha and GIF (expo-image), animated GIF / WebP (40 pt) and two spinners; `resizeMode="repeat"` dots, a `blurhash:/` source, an `Image.loadAsync` ref; the text of `generateBlurhashAsync` for the checker GIF (iOS: `LbL3.FJ8J8Nv67I[1z:hv{Nu:}]i`, OneSim the same); an `autoplay={false}` GIF | `--click Images` |
| fonts / -dark | Fonts: `Font.loadAsync({Roboto: require('./assets/Roboto.ttf')})`, then Roboto 34 / 17 lines, a system line, a Roboto span and `renderToImageAsync('Drawn 28', {fontFamily: 'Roboto', size: 28, color: '#FF9500'})` | `--click Fonts` |

## App facts OneSim must match
- Root `paddingTop: 62`; header 44 pt (Back, gray title); buttons blue `#007AFF`, radius 12.
- iOS 26+ draws an action sheet without an anchor as a centred card on iPhone: the alert's width (320 pt), position
  (centre 14 pt below the screen's) and dim, Liquid Glass with the popover's shadow, buttons stacked (48 pt, 8 pt apart)
  in option order with Cancel last, every button label regular weight, plain ones in the label color. A tap outside
  the card picks the cancel index.
- The share sheet for a link iOS cannot preview (`.invalid`, so the reference does not depend on a fetched page): the
  host name by a white tile with a gray compass; the More tile stays white in dark.
- Links (not compared; `run-apis.mjs` checks it): Probe's `canOpenURL` answers in Expo Go on the iOS 27.2 Simulator
  were https/http/sms/maps/app-settings/exp 1; tel/mailto/facetime/itms-apps 0 (the Simulator has no Phone, Mail,
  FaceTime or App Store); waze 0 (Expo Go declares it, it is not installed); expoapis 0 (Expo Go registers only `exp`).
  OneSim answers as a phone (those Apple apps installed) running the app as a development build (its own scheme).
- Animated images and spinners run; their frame at capture time is arbitrary (40 pt boxes: a small share of the diff).
- `Roboto.ttf` is the variable Roboto from `assets/fonts` (OFL, `Roboto-OFL.txt`).
