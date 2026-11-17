# showcase-ios ground-truth fixtures

Real SwiftUI (`examples/swiftui-showcase/Showcase.swift`) built with `scripts/build-ios-sim.sh examples/swiftui-showcase target/ios/Showcase.app`
and captured on a real iOS Simulator via agent-device. One `<name>.png` + `<name>.json` per state:
home, typography, shapes, todos, todos-toggled, todos-typed, controls, controls-changed, home-dark.

- Device: iPhone 17 Pro simulator, iOS 26.5. Screen 402 x 874 pt, scale 3 -> PNGs are 1206 x 2622 px.
- PNG: `screenshot --pixel-density 3 --normalize-status-bar` (status bar 9:41, wifi, battery full/charging bolt; cellular shows no-service dots).
  The Dynamic Island pill is drawn in some PNGs only (typography, todos-toggled, controls*, ...) and absent in others (home, home-dark, shapes, todos-typed): screenshot artifact, mask it.
- JSON: raw `agent-device snapshot --json` (`data.nodes[].rect` in points, origin top-left, includes `depth`/`parentIndex`).
  Light appearance except home-dark. App state is not persisted across relaunch (todos reset).
- Note: `todos` is captured at launch state (item 3 open); `todos-toggled` has ALL three done.
  `controls` is Notifications on, volume 3; `controls-changed` Notifications off, volume 5 ("You will be notified" row removed).

## Safe area / chrome (pt)
| item | value | source |
|---|---|---|
| status bar / top safe area | 62 (0-62; Dynamic Island, time 9:41 centered at left) | nav bar y=62 |
| nav bar, large title | y=62, h=106 (bottom at 168) | NavigationBar rect |
| nav bar, inline title (Typography) | y=62, h=54 (bottom 116) | typography.json |
| back button | 44x44 at (16,62) | Button "Showcase" (id BackButton) |
| home indicator / bottom inset | 34 (standard for this device; NOT exposed in snapshot, not measured) | - |
| keyboard (todos-typed) | y=539 (suggestion bar 44) .. 874; keys from y=583, 233 tall; key 39.3x54 | todos-typed.json |

## Key frames (pt, x,y,w,h)
| element | frame |
|---|---|
| Large title "Showcase" label | 16,119.7, 164x40.7 (text ~34pt bold) |
| Large title "Todos"/"Controls"/"Shapes" | 16,119.7, 95.3/135.7/119 x 40.7 |
| List first section header "Screens" | 16,168, 370x40.3 (text frame spans full row width) |
| 2nd section header "Summary" | 16,434, 370x40.3 (card above ends 416.3 -> 17.7 gap) |
| First row "Typography" | 16,208.3, 370x52 ; label at 32,224.2 90.7x20.3 (17pt) |
| Row height / pitch | 52 (all rows); card inset 16 each side (width 370) |
| Chevron | 360.7,228.3 7x12 (right edge 367.7 = row right 386 - 18.3) |
| Trailing value "1" | 362.3,489.8 7.7x20.3 |
| Todos: TextField row cell | 16,168 370x52; TextField 32,183 299.3x22; "Add" button 339.3,183.8 30.7x20.3 |
| Todos: "Items" header | 16,237.7 370x40.3; rows at y=278/330/382, 370x52 |
| Todo row circle | 22x22 at x=32 (fill green when done, 1.5pt gray stroke); text starts x=66 |
| Controls: Toggle row | cell 16,168 370x52; Switch 309,180 63x28; label 32,183.8 |
| Stepper row | cell 16,220 370x52; Stepper 32,230 338x32 (- 276,230 47x32 / + 323,230 47x32) |
| Button row (Less/value/More) | cell 16,272 370x64.3; Less 32,287 59x34.3; value text 113.8,294; More 147,287 63.3x34.3 |
| "You will be notified" row | cell 16,336.3 370x52 |
| Typography text stack | Large Title 16,132 152x40.7; Title y184.7 h33.7; Headline y230.3 h20.3; Body y262.7 h42.3 (2 lines, 340 wide); Caption y317 h14.3; Bold y343.3; System22 y375.7 h26.3; blue text y414; Divider ~y446; centered text 39.5,458.7 323x42.3. VStack spacing 12, padding 16 |
| Shapes | text/overlay frames only in snapshot (Overlay 170.5,293.8; Shadow card 151.3,380 99.3x20.3; ZStack label 181,489.2); shape geometry read from PNG |

## Visual facts for SimCore (sampled from PNG, RGB)
| thing | light | dark |
|---|---|---|
| grouped background | 242,242,247 (#F2F2F7) | 0,0,0 |
| row / card background | 255,255,255 | 28,28,30 (#1C1C1E) |
| plain (non-List) screen bg (Typography, Shapes) | white 255 | (not captured) |
| separator (1px/3x ~ 0.33pt) | 232,232,232 on white | 56,56,59 |
| section header text | 133,133,139 (~secondaryLabel) | 141,141,147 |
| chevron | ~197,197,199 (tertiary gray, light weight) | ~90,90,94 |
| primary label | 0,0,0 | 255,255,255 |
| trailing value "1" (.secondary) | ~127,127,127 (AA extreme) | ~142 |
| Toggle on-colour | 52,199,89 (#34C759), knob white | - |
| Toggle off track | light gray ~ (200,200,202) w/ white knob (visual) | - |
| accent/blue (Button text, "More" fill, Add) | 0,136,255 (#0088FF, iOS 26 blue) | - |
| Less (bordered) fill | 233,233,235, blue text | - |
| Stepper background | 238,238,239 | - |
| back button glass | 251,251,255 circle, soft shadow, dark chevron | - |

- Section headers are NOT uppercased (iOS 26 inset-grouped): "Screens", "Summary", "Items" in sentence case, semibold ~13-14pt, secondaryLabel colour, left aligned at x=32 (text), 16pt-wide card margin + 16 inner.
- Separator inset: starts at x=32 (row text leading), ends x=370 (16pt in from card right edge 386); no separator after last row. In Todos rows the separator starts at x~66 (aligned with text after the circle), because the List aligns it to the first text.
- Cards (inset grouped) have very large corner radius ~26pt (iOS 26), no border, no shadow. First card y=208.3..416.3.
- Section header to card gap is 0 (header cell 40.3 high butts the card).
- Large title bold ~34pt, left x=16 (card edge); no nav bar background/blur at top at rest: iOS 26 scroll-edge effect, content shows through (bg continuous with grouped colour).
- Liquid Glass: back button is a 44x44 glass circle at (16,62) with chevron only (no "Showcase" label, no text), subtle drop shadow and a soft blurred halo (~up to 100pt wide) behind it; inline title (Typography) centered at x~201, bold ~17pt, no bar background.
- Buttons in List (todos rows): text shows accent-tinted even with `.foregroundStyle(.secondary)`: done rows render light-blue (~#7CC0FF) strikethrough text, not gray (Button label tinting in List). Circle fill green (52,199,89) with gray ring.
- Switch/stepper are iOS 26 controls: switch 63x28 (wider than iOS 17's 51x31), stepper is a rounded-capsule pair with divider (the +/- pair).
- Keyboard (todos-typed) is the iOS 26 floating-rounded keyboard with predictive bar; caret blue; "Add" turns blue (enabled).
