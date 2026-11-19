# rn-router-ios ground-truth fixtures

Expo Router apps in **Expo Go 58** on the iPhone 17 Pro simulator, iOS 26.5 (402 x 874 pt, scale 3, PNGs 1206 x 2622).
Compared by `scripts/compare-ios.sh` (RN router blocks) against `onesim expo`.
- `tabs-*`: `examples/expo-default` (unmodified create-expo-app template: NativeTabs Home / Explore)
- `stack-*`: `examples/expo-router-stack` (Stack: "Fruits" large-title list, `item/[name]` detail)

## Capture steps (same method as `../rn-showcase-ios/README.md`)
1. Metro per app: `cd examples/<app> && CI=1 npx expo start --port 813x` (8130 default, 8131 stack).
2. Light appearance (`agent-device settings appearance light`). Open the app once with
   `agent-device open host.exp.Exponent "exp://127.0.0.1:PORT?__expo_disable_fab=1&__expo_disable_auto_launch=1&__expo_disable_onboarding=1"`
   (Tools button is already off; no status-bar back-link appeared). Wait >= 3 s after every launch/tap (splash, push, tab switch).
3. Each PNG: `agent-device screenshot <name>.png --pixel-density 3 --normalize-status-bar`, retaken until the Dynamic Island is drawn (1-10 tries; status bar 9:41).
   Taps are `agent-device press X Y` in points.
4. Dark: `agent-device settings appearance dark`, wait 3-5 s, screenshot, then back to light.

## States and OneSim equivalents (`onesim expo --url http://localhost:PORT ...`)
Tabs (default template, light unless noted):
| state | iOS capture | OneSim |
|---|---|---|
| tabs-home | launch, wait 3 s | (none) |
| tabs-explore | press Explore tab (244,822) | `--click Explore` (or `--click 244,822`) |
| tabs-explore-expanded | from Explore, press first collapsible header "File-based routing" (100,389) | `--click Explore --click "File-based routing"` (or `--click 244,822 --click 100,389`) |
| tabs-home-dark | dark appearance, Home tab | `--dark` |
| tabs-explore-dark | dark appearance, press Explore (244,822) | `--dark --click Explore` |

Stack:
| state | iOS capture | OneSim |
|---|---|---|
| stack-list | launch | (none) |
| stack-detail | press Apple row (200,192) | `--click Apple` (or `--click 200,192`) |
| stack-back | from detail, press header back button "Fruits" (64,84) | `--click Apple --click Fruits` (or `--click 200,192 --click 64,84`); identical to stack-list (0.00% diff) |
| stack-detail-dark | dark appearance, press Apple | `--dark --click Apple` |

## Facts OneSim must match (points, from accessibility snapshots and PNGs)
Tabs (iOS 26 NativeTabs, Liquid Glass):
- Tab bar is a floating glass capsule, not edge-to-edge: pill x~107..294 (w~188), y~791..852 (h~61); snapshot TabBar frame is 0,791,402x83 (container).
  Items: Home button 111,795 94x54; Explore 197,795 94x54 (tap centers (158,822) and (244,822)).
  Selected item has a gray (light ~#E9E9EB / dark ~#3A3A3C) rounded capsule inside the pill, blue (#0088FF-ish) SF symbol, label 11-12pt below the icon (black/white, not blue). Unselected: black/white symbol.
  Symbols: Home = filled house, Explore = stacked rectangles ("square.stack"-style). Pill shadow soft, white (light) / ~#1c1c1e with 1pt lighter outline (dark).
- No nav bar / header on tabs; content starts at y=62 inset. Home: logo 64-ish pt blue squircle with glow, "Welcome to Expo" 48pt bold centered (frame 48,314 306x104), "GET STARTED" monospace 12pt at y=534, grouped card x=24..378,y~494..? (rows at y 588/632/673). Splash end state: the template's splash overlay has finished (no splash visible after 3 s); the app shows the logo, no animation in flight.
- Explore: ScrollView y=62..791 (under the tab bar glass); title "Explore" 40pt bold centered at y=126; subtitle y=186 (gray, 2 lines); "Expo documentation" pill link 102.7,250.7 197x46.3 with arrow.up.right.square symbol; collapsible rows at y=377,433,489,545,601 (frame x=24,w=354,h=24, 56pt pitch) with circular chevron.down badge (gray-tint circle) + label; expanded row shows content card x=48..378 y~410..575 with chevron.up, text, blue "Learn more", rows below shift by +~130 pt and the last row slides under the tab bar glass.
- Dark: pure black background, white text, cards ~#1c1c1e; the Dynamic Island is black on black (not visible).
Stack (expo-router Stack, iOS 26 native-stack):
- List: large title "Fruits" bold ~34pt, left x=16, frame 16,119.7 90.7x40.7; NavigationBar 0,62 402x106 (large-title header, no background at rest, no back button, no centered small title). Rows `Link` 402x48.7 at y=168,216.7,265.3,314,362.7 (Apple..Peach), text 20pt-ish at x~20, 1px hairline between rows including after the last; bg #fff.
- Detail: header height 54 (frame 0,62 402x54), centered inline title "Apple" bold 17pt at (177.7,73.7); back button is a Liquid Glass pill "< Fruits" (chevron + previous title as back title) at 16,62 95.7x44 (visible pill ~ x 16..112, y 47..84 in pt... see PNG), soft shadow halo ~100pt wide; text black. Body centered: "You picked Apple" 22pt bold frame 107,392 187x29; blue "Go back" button 148.3,441.3 105x40.3 (#007AFF fill, white 17pt semibold, radius ~10). Android is irrelevant.
- The app hard-codes `#fff` content and does not use a theme, so **dark appearance does not change the stack app at all** (stack-detail-dark is identical to stack-detail); native header/back button also stay light.
- Expo Go adds no chrome with Tools button off. A 1-px dark speck can appear at the very top-left pixel of some light captures (transient status-bar artifact; ignore).
