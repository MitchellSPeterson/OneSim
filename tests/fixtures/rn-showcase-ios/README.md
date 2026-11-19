# rn-showcase-ios ground-truth fixtures

`examples/expo-showcase` (plain React Native, Expo SDK 58) running in **Expo Go 58.0.7** on the iPhone 17 Pro simulator, iOS 26.5
(402 x 874 pt, scale 3, PNGs 1206 x 2622). Compared by `scripts/compare-ios.sh` (RN block) against `simcore expo`.

## Capture steps
1. `cd examples/expo-showcase && npm install && CI=1 npx expo start --port 8095` (Expo Go 58 must be installed on the sim; `npx expo start --ios` installs it).
2. Light appearance. Open Expo Go, then `open host.exp.Exponent "exp://127.0.0.1:8095?__expo_disable_fab=1&__expo_disable_auto_launch=1&__expo_disable_onboarding=1"`
   with agent-device (this adds a "< Showcase" back-link to the status bar; instead open Expo Go without URL and tap the "expo-showcase" recent entry).
3. First run only: tap the floating gear (Expo Go tools button), scroll the sheet, turn **Tools button** off, close. The setting persists, so screenshots show only the app.
4. Each PNG: `agent-device screenshot <name>.png --pixel-density 3 --normalize-status-bar`, retaken until the Dynamic Island is fully drawn (status bar 9:41, battery full).
   Taps are agent-device `press X Y` in points; typing is `agent-device type Hello` (if the runner watchdog trips, relaunch Expo Go and redo).
5. `home-dark`: `agent-device settings appearance dark`, wait, screenshot, then back to light. The app follows `useColorScheme` (black bg, white text).

## States and SimCore equivalents (`simcore expo --url http://localhost:PORT ...`)
| state | iOS capture | SimCore |
|---|---|---|
| home | launch | (none) |
| layout / text / controls | press row (60,145) / (60,197) / (60,249) (7 home rows: ... Image, Visuals, Refresh) | `--click Layout` / `--click Text` / `--click Controls` |
| controls-changed | Controls, press Increment (60,194) twice, press Alpha switch (354,250) | `--click Controls --click Increment --click Increment --click Alpha` |
| controls-typed | fresh Controls, press input (200,358), type `Hello` (keyboard visible) | `--click Controls --click "Type here" --type Hello` |
| scroll | press Scroll row (60,301) | `--click Scroll` |
| scroll-scrolled | Scroll, press **Jump** (365,84): `scrollToOffset({offset: 440, animated: false})` | `--click Scroll --click Jump` (or `--scroll 200,500,0,440`: same 440 pt offset) |
| image | press Image row (60,353) | `--click Image` |
| home-dark | dark appearance | `--dark` |
| keyboard-avoiding / -dark | press the "Showcase" title (60,90) → Form screen, press the Message input (200,836), type `Hello` (keyboard up; `KeyboardAvoidingView` behavior padding lifts the form above it); -dark in dark appearance (form text stays black, as in Controls) | `--click Showcase --click Message --type Hello` (`--dark` first for -dark) |
| alert / alert-dark | Controls, press "Count: 0" (80,140): `Alert.alert` with Cancel + Reset (destructive); alert-dark in dark appearance | `--click Controls --click Count` (`--dark` first for alert-dark) |
| visuals / visuals-dark | press Visuals row (60,405); -dark in dark appearance | `--click Visuals` (`--dark` first for -dark) |
| refresh-pulled / -dark | press Refresh row (60,457), `agent-device scroll up 0.5 --duration-ms 1000` (pulls ~145 pt past the top: onRefresh, title "Refreshing", spinner stays), wait ~1 s; -dark in dark appearance | `--click Refresh --scroll 200,300,0,-150` (`--dark` first for -dark) |

## App facts SimCore must match
- Root `paddingTop: 62` (iPhone 17 Pro inset), no safe-area lib; Expo Go adds no chrome with the Tools button off. `StatusBar` style follows scheme.
- Plain system font (SF Pro), explicit sizes; Home title 34 bold, rows 52 pt with 1 pt hairline, chevron is a ">" glyph, #C7C7CC.
- Header row: 44 pt tall at y=62 ("Back" link blue 17pt left, title gray 17pt semibold, Scroll also "Jump" right). Labels (`accessibilityLabel`): rows Layout/Text/Controls/Scroll/Image, Back, Increment, Count (the "Count: N" text; opens the reset alert), Alpha, Beta (Switch), "Type here" (TextInput), Jump.
- FlatList: 50 rows "Row N", fixed height 44, `getItemLayout`; starts at y=106.
- Native controls: iOS 26 UISwitch, UITextInput with caret and floating iOS 26 keyboard (lowercase keys when empty/typed text).
- Form screen (no visible entry: the home title is a `Pressable`): `KeyboardAvoidingView` behavior `padding`, `keyboardVerticalOffset` 62, content bottom-aligned (label "Message" + TextInput).
- Visuals screen: root `paddingTop` 0, content in core `SafeAreaView` (pads 62/34). `ActivityIndicator` small, large, red small, blue
  large, large `animating={false}` (hidden); expo-linear-gradient card (3 colors, locations 0/0.3/1, diagonal) and pill (horizontal);
  15 color stripes (140 pt, radius 16) under four 82x80 `BlurView`s (radius 12): default (intensity 50), light, dark, systemMaterial (100).
  `LogBox.ignoreLogs` drops the RN 0.88 SafeAreaView deprecation warning (iOS Expo Go shows warnings only in the debugger).
- Refresh screen: FlatList of 30 rows (44 pt) with `RefreshControl`; pulling sets `refreshing` for good (header title "Refreshing").
  The refreshing band is 60.33 pt; its spinner ~28 pt, centred.
- Image screen uses a bundled PNG (`assets/gradient.png` 200x120) with resizeMode cover/contain and borderRadius 24.
