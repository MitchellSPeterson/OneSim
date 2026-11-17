# showcase-ios ground-truth fixtures

Real SwiftUI (`examples/swiftui-showcase/Showcase.swift`) built with `scripts/build-ios-sim.sh examples/swiftui-showcase target/ios/Showcase.app`
and captured on a real iOS Simulator via agent-device. One `<name>.png` + `<name>.json` per state:
home, typography, shapes, todos, todos-toggled, todos-typed, controls, controls-changed, home-dark,
symbols, symbols-dark, keyboard-dark, keyboard-shift (recaptured 2026-11-17, see "Recapture notes" at the bottom).

- Device: iPhone 17 Pro simulator, iOS 26.5. Screen 402 x 874 pt, scale 3 -> PNGs are 1206 x 2622 px.
- PNG: `screenshot --pixel-density 3 --normalize-status-bar` (status bar 9:41, wifi, battery full/charging bolt; cellular shows no-service dots).
  The Dynamic Island pill is now present in EVERY PNG (see Recapture notes; in the dark PNGs it is black on black bg so invisible but still there).
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

## Recapture notes (2026-11-17)
Capture: `scripts/build-ios-sim.sh` -> `simctl install`; everything else through agent-device (appearance, relaunch, press, type, snapshot, screenshot).
Order: light: home, Typography, back, Shapes, back, Todos (todos), tap "Ship the SwiftUI subset" (todos-toggled), tap field at (150,194) + type "Buy milk" (todos-typed, no relaunch);
relaunch, Controls (controls), tap switch (340,194) + More twice (178,304) (controls-changed), back, Symbols (symbols).
dark: relaunch, home-dark, Symbols (symbols-dark), back, Todos, tap field, type "Buy milk" (keyboard-dark). light: relaunch, Todos, tap field, no typing (keyboard-shift).
(`press 'type=TextField'` did not focus the field; a coordinate press did.)

### Dynamic Island: why it was inconsistent, and the fix
- Plain `screenshot --pixel-density 3` (no `--normalize-status-bar`) NEVER shows the island (0 of 6 samples). With `--normalize-status-bar` it appears in a random ~50% of captures:
  back-to-back identical calls on an idle app gave island/no-island in no pattern (1,1,1,1,0,0 / 0,0,0,0,0,1 / with 4 s sleeps 0,0,1,0). `--no-stabilize`, a pause, relaunch and the 1x vs 3x density made no difference.
  Cause is outside the app: a race in the simulator status-bar override / framebuffer capture; the app content is identical either way.
- Fix: always pass `--normalize-status-bar` and re-shoot until the island pixel is black (check pixel (603,100) px is <12 and differs from (300,100)), up to 30 tries, then snapshot JSON is taken once before. Typical 1-5 tries. All 13 PNGs verified to contain the island.
  Dark PNGs: island is (0,0,0) on a (0,0,0) background so it cannot be verified by pixels; it is not visible and does not need to be drawn there (a renderer may draw a black pill in all modes).
- The snapshot JSON does not contain the island; use the measured frame below.

### Dynamic Island (light PNG, 3x px measured, pt = px/3)
| item | value |
|---|---|
| frame | x=138.33, y=14.0, w=125.33, h=36.67 (px 415..790 x 42..151); centred on x=201, i.e. ~125x37 pill at (138.5,14), corner radius = h/2 (full capsule) |
| color | (0,0,0) |
| status bar content | time 9:41 left of island, wifi + battery right; island occupies y 14..50.7 inside the 62pt top inset |

### Keyboard (iOS 26, iPhone 17 Pro, light/dark) - measured on keyboard-shift / keyboard-dark
| item | value |
|---|---|
| inputView (whole keyboard incl. suggestion bar) | 0,539, 402x335 (bottom = 874, screen bottom) |
| suggestion bar | 0,539, 402x44; 3 candidates "I" (0,539,134x44), "The" (134,539,134x44), "I\u2019m" (268,539,134x44); 1-px (0.67pt) separators at x=134 and 268, color (205,206,213) light / (50,50,52) dark |
| key area (UIKeyboardLayoutStar) | 0,583, 402x233 (to y=816); emoji/mic row below at y~805..874 |
| panel background | light (223,224,230) ; dark (23,23,23). Above-panel app bg is not tinted by it. |
| panel shape | floating-rounded: top corners radius ~28pt (fit from the corner profile), spans full width (inset 0); bottom corners follow device corner (outside pixels = app bg). Top edge has a 1px lighter line (light 248,250,253). No outer shadow measured. |
| key cap color | light: ALL keys (letters and shift/delete/123/space/return) (255,255,255); dark: ALL keys (61,61,61). No separate darker function-key color in this capture. |
| key cap geometry (visual) | e.g. Q: x 6.67..40.0 (w 33.3), y 591..634 (h 43). Hit frames below are bigger (cell 39.3x54): cap = hit frame inset ~2pt horizontally (gap ~6pt... see x pitch) and 1pt top, 11pt bottom. No visible shadow under the caps (pixel below cap = panel bg). |
| key corner radius | ~7pt (fit: offset at depth 0/1/2/3/5pt = 6.3/3.7/2.3/1.3/0.3pt) |
| key label | letters: regular weight, cap-height of "E" = 15.33pt -> ~22pt SF (E glyph 9.0 wide); color light (0,0,0) / dark (255,255,255); "123" and return glyph, emoji, mic same colors |
| letter case / shift | shift ON (keyboard-shift, keyboard-dark, todos-typed - see surprise): labels UPPERCASE (snapshot labels "Q","W"...), shift glyph solid filled arrow, `selected:true`. Shift OFF (checked by tapping shift; not saved as fixture): labels lowercase "q","w"..., shift glyph hollow outline arrow, selected absent. Return/123/space labels never change. |

Key hit frames (pt, x,y,w,h; identical in light and dark):
| key | frame |
|---|---|
| Q W E R T Y U I O P | y=590 h=54; x = 4.67, 44, 83.67, 123.0, 162.67, 202, 241.67, 281, 320.67, 360 ; w alternates 39.33/39.67 |
| A S D F G H J K L | y=644 h=54; x = 24.33, 63.67, 103.33, 142.67, 182.33, 221.67, 261.33, 300.67, 340.33 ; w 39.33/39.67 |
| shift | 4.67,698, 51.33x54 |
| Z X C V B N M | y=698; x = 63.67, 103.33, 142.67, 182.33, 221.67, 261.33, 300.67 ; w 39.67/39.33 |
| delete (backspace) | 348,698, 51.67x54 |
| 123 ("numbers", id `more`) | 4.67,752, 98.67x54 |
| space | 103.33,752, 197.33x54 |
| return | 300.67,752, 99x54 |
| Emoji button | 8.48,805.7, 68.09x68.3 (glyph ~ 22pt emoji smiley centred ~ (42,840)) |
| Dictate (mic) button | 325.44,805.37, 68.09x68.63 |
Row pitch 54, key pitch 39.5 (10 keys across 395 = 402 - 2*4.67 -ish), visual caps 33.3 wide with ~6pt gap, 43 tall with ~11pt gap.

### Shadow card (shapes screen, `.shadow(radius: 6)`, .cornerRadius(12), white card on white bg)
Card geometry from pixels: x 135.33..266.67 (w 131.33), y 364..416.33 (h 52.33), corner radius ~12. Screen bg on Shapes is pure white (255).
Profile is IDENTICAL on right, bottom, top and left edges => shadow offset (0,0). Fit: shadow = black, alpha 0.33 * (1 - Phi(d/sigma)), **sigma = 5.9pt (~radius 6), peak alpha 0.33 (default SwiftUI shadow color)**, residual err tiny. Extends visibly ~14pt (43px) from the edge.
Gray channel value leaving the card edge (3 px = 1pt per step, d = distance from edge in pt, bg 255):
| d(pt) | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12 | 13 | 14 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| right edge | 214 | 223 | 228 | 233 | 236 | 240 | 243 | 246 | 248 | 250 | 251 | 252 | 253 | 254 | 254 |
| bottom edge | 214 | 223 | 228 | 233 | 238 | 241 | 245 | 247 | 249 | 250 | 251 | 252 | 253 | 253 | 254 |
(first px values at 1/3pt steps, right edge: 214,216,218,219,221,223,225,227,228,230,232,233,235,236,238,239,240,241,243,244,245,246,246,247,248,249,249,250,251,251,252,252,252,253,253,253,253,254,254,254,254,254,254,255; bottom nearly identical)
alpha = (255 - v)/255: 0.161 at the edge (= 0.33/2), 0.04 at 7pt, ~0.004 at 14pt.

### Back button halo (Liquid Glass, 44x44 at 16,62)
Glass fill (251,251,255) on grouped bg (242,242,247); on white bg (255) it is (253,253,253). Halo is wide, soft and offset DOWN (offset y ~ +4pt, blur sigma ~ 20pt, black alpha ~ 0.06): values on grouped-light bg (242,242,247) (rgb R channel; B is +5):
| distance from button edge (pt) | 0 | 2 | 4 | 8 | 12 | 16 | 20 | 24 | 28 | 32 |
|---|---|---|---|---|---|---|---|---|---|---|
| right (row y=84, edge x=60) | 235 | 235 | 236 | 237 | 239 | 240 | 240 | 241 | 241 | 242 |
| below (col x=38, edge y=106) | 231 | 232 | 233 | 234 | 236 | 237 | 238 | n/a | n/a | n/a |
(below column sampled only to y=126 (20pt), still 4 under bg there, so it extends further than the right side). Left of the button (x 0..16): 239 at x=0 -> 235 at x=15; above (button top y=62 going up to y=40): 237 at y=61 -> 241 at y=40.
On white (shapes): right 247 at edge -> 255 at ~32pt; below 244 at edge -> 251 at 20pt. Halo is strongest below/right-below, symmetric-ish horizontally, i.e. offset roughly (0,+3..4), a very blurry shadow (not a hard ring). Fit with sigma ~15-20pt, alpha ~0.07 if needed.
Dark: back button is a dark glass circle (visible in symbols-dark), no sampled halo.

### Symbols screen (symbols.json; frames pt)
| element | frame |
|---|---|
| "Labels" header | 16,168, 370x40.33 |
| row Favorites (cell) | 16,208.33, 370x52; image star.fill 32,223 24x22.67; text frame 16,208.33 370x52 |
| row Settings | 16,260.33, 370x52; image gear 32.33,275 23.33x23 |
| row Trash | 16,312.33, 370x52; image trash 33.67,326.33 20.67x24 |
| label icon column | icons start at x~32 (cell inset 16 + 16 pad), icon box ~24 wide, centre x~44; label text starts at x=64.33 (Messages text x=64.33 in the inline row, icon 34,573 20.3x14.3) |
| "Images" header | 16,382, 370x40.33; card row 16,422.33, 370x74 |
| heart.fill | 34,451.67 16.67x15.33 |
| bell | 75.67,445.33 26x28.33 |
| checkmark.circle.fill | 126.67,448.67 21.67x21.67 |
| paperplane | 172.33,444.67 29.67x29.67 |
| cloud.sun.fill | 225.33,443.67 44x31.33 |
| "Inline" header | 16,513.67, 370x40.33 |
| row Messages | 16,554, 370x52; envelope 34,573 20.33x14.33; text "Messages" 64.33,569.83 77.33x20.33; value "12" 335,569.83 17.33x20.33; chevron.right 362.67,574.33 6.67x11.67 |
Row pitch 52; the trailing section rows first at 208.33.
Icon tint colors (most frequent pixel in icon frame), light / dark:
| symbol | light | dark |
|---|---|---|
| star.fill (Favorites) | (0,136,255) | (0,145,255) |
| gear (Settings) | (0,136,255) | (0,145,255) |
| trash (Trash, red label text too) | (255,56,60) | (255,66,69) |
| heart.fill | (255,45,85) | (255,55,95) |
| bell | (0,0,0) | (255,255,255) |
| checkmark.circle.fill (green, white check) | (52,199,89) | (48,209,88) |
| paperplane | (0,0,0) | (255,255,255) |
| cloud.sun.fill | (0,0,0) monochrome | (255,255,255) |
| envelope (inline) | (0,0,0) | (255,255,255) |
| chevron.right (inline) | (191,191,191) | ~ dark gray |
Dark accent is (0,145,255), system red (255,66,69), pink-red (255,55,95), green (48,209,88). Dark card bg (28,28,30), screen bg (0,0,0).
