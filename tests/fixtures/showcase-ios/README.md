# showcase-ios ground-truth fixtures

Real SwiftUI (`examples/swiftui-showcase/Showcase.swift`) built with `scripts/build-ios-sim.sh examples/swiftui-showcase target/ios/Showcase.app`
and captured on a real iOS Simulator via agent-device. One `<name>.png` + `<name>.json` per state:
home, typography, shapes, todos, todos-toggled, todos-typed, controls, controls-changed, home-dark,
symbols, symbols-dark, keyboard-dark, keyboard-shift (recaptured 2026-11-17, see "Recapture notes" at the bottom);
alert, alert-dark, sheet, sheet-dark, sheet-medium, sheet-medium-dark, sheet-nested, sheet-nested-dark, dialog, dialog-dark, cover, cover-dark (new 2026-11-18, see "Alert and sheet"; `controls`/`controls-changed` recaptured the same day: Controls has a second section with "Show alert" / "Show sheet").

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

## Liquid Glass (iOS 26.5, captured 2026-11-17, measured on glass*.png, 3x px unless noted)
New fixtures: `home`/`home-dark` (recaptured, 6th row "Glass"), `glass`, `glass-dark`, `glass-pressed` (tap "Glass 0" -> label "Glass 1"; only the label differs, glass fill identical),
`typography-scrolled`, `typography-scrolled-dark`. `symbols-scrolled` SKIPPED: Symbols content (2 sections, 4 rows) does not overflow the screen, so it cannot scroll.
Dark PNG Dynamic Island is black on black, not verifiable (same as before); light ones verified (603,100)=black.

### Typography scroll offset (IMPORTANT caveat)
At the default text size the Typography content (ends y~501 of 874) does NOT overflow, so `scroll`/`swipe` do nothing (Large Title stays y=132, scrollbar "1 page").
To get a scrollable screen the *-scrolled fixtures were captured with Dynamic Type `accessibility-extra-extra-extra-large` (`agent-device settings text-size ...`, then restored to `large`).
So their layout (font sizes, text y positions) is NOT the standard one; use them only for the scroll-edge/nav-bar effect.
Procedure: open Typography, `scroll top`, `scroll down 0.06` (agent-device converts to a 52 px finger path), wait 3 s, snapshot, screenshot.
"Large Title" frame y (snapshot): before = 132.0, after = 92.0 -> scroll offset = 40.0 pt (typography-scrolled and -dark). Title: 184.7 -> 144.7 (frame heights differ at this text size: Large Title 71.7pt).
Reproducibility: 5 of 7 runs gave exactly 92.0; 2 gave 97.0 (offset 35, probably read mid-settle); fixtures were re-shot until 92.0. Other amounts: 0.02->125.7, 0.03->112.7, 0.04->111, 0.08->85, 0.15->30.3 (max scroll, offset 101.7). Offset is not linear in the amount (momentum).

### Frames (pt, from px edges; snapshot has only text/icon frames for glass content)
ZStack stripes ignore nothing: they fill the safe-area-inset content only: y 168..840 (px 504..2520), 5 bands of 134.4pt: orange 168-302.3, pink -436.7, purple -571, blue -705.7, green -840; white above/below.
Raw stripe RGB light: orange 255,141,40 pink 255,45,85 purple 203,48,224 blue 0,136,255 green 52,199,89; dark: 255,146,48 / 255,55,95 / 219,52,242 / 0,145,255 / 48,209,88.
| element | frame (x,y,w,h) pt | shape |
|---|---|---|
| "Liquid Glass" capsule | 99.3,333,203.3,57.3 (text 123.3,345 155x33.7 + padding 24/12) | capsule r=h/2 |
| star / heart / bell circles | (101|173|245), 414.7, 56x56 (centre y 442.7; heart/bell centre x 201/273) | circle |
| Card | 91,494.7,220x64 (text centre 201,526.7) | rounded rect r=20 |
| .glass button "Glass N" | 160.7,582.7,80.7x34.3 | capsule |
| .glassProminent "Prominent" | 150,641,102x34.3 | capsule |
| back button | 16,62,44x44 | circle |
Only the circles straddle a stripe boundary (pink/purple at y=436.7pt = px 1310); capsule/card/buttons sit in one stripe.

### Color transform (glass interior, light | dark). out = a*raw + (1-a)*C
| variant | light | dark |
|---|---|---|
| regular over pink / purple / blue | (255,167,184) / (234,168,243) / (144,208,255) | (164,47,71) / (143,45,157) / (19,99,160) |
| fit | out = 0.44*raw + 0.56*255, i.e. white veil alpha 0.56, NO saturation change (slope same on all channels, G intercept +4) | out = 0.57*raw + ~17 (black-ish veil alpha ~0.43 of gray ~40, i.e. darken to 57%) |
| .clear (bell, over purple; pink side = 109 in B) | (229,69,250) = raw + (26,21,26): additive ~ +24/255, slope 1 | (245,73,255) = raw + (26,21,13 clipped) |
| .tint(.red) (heart) | (239,38,41), independent of stripe (opaque, alpha~1) | (254,74,77) |
| prominent (accent blue fill) | (0,122,232) (~0.90 x blue 0,136,255) | (10,148,253) |
Interior is a FLAT colour (no gradient, no inner shading) except the blurred stripe boundary and rim band below.

### Blur
Star circle (regular): stripe boundary (true y=1310 px) seen inside the glass is a smooth step, 50% at ~1307-1309 (shift <=3 px), 16-84% width ~16-19 px -> Gaussian backdrop blur sigma ~ 8-10 px = ~3 pt (dark: same, 9-11 px). Outside the glass the edge is a 1 px hard step.
Clear (bell) shows the boundary at the same place (pink cap crossing ~1305-1309), blur not separately fitted; looks similar. Tinted heart hides the backdrop entirely.

### Lensing / refraction
Interior boundary position stays at ~1307 (+-2 px) for all |dx|<=56 px from the circle centre: NO interior displacement, so interior is not magnified.
Refraction is confined to a thin rim band: top ~2-4pt (6-12 px) of the star interior shows PURPLE (sampled from >=65 px = 22 pt away, below the boundary) instead of pink; bell (clear) bottom rim shows a ~3pt (8 px) PINK arc (255,77,121) sampled from >=95 px above. => outer ~3pt band samples the backdrop from the far side / displaced >=22 pt (inward, strongly), inner area undisplaced. Pink cap near the top of circles narrows to a "blob" (zero width within ~25-30px of the rim sides). Best simple model: rim band 3pt wide, source offset ~ +-(22..32)pt toward/through the centre.

### Rim highlight
Flat fill right up to the edge; the rim is one ~1 px (0.33 pt, maybe up to 2px) brighter line, no glow/gradient inside (3x px, inside direction):
light capsule left edge: x=297 (233,41,78 shadowed bg) | 298 (255,200,224) | 301+ (255,167,184) fill. Peak = fill blended ~35-40% towards white: ~(255,202,226) on pink fill; circles (249,198,252) over lavender fill (234,168,242); heart (249,121,128) over (239,39,41); bell (247,130,255) over (229,69,249).
dark: (222,55,108) over fill (164,47,71) -> the dark rim is a bright pink-ish 1px (backdrop-ish, ~ +58 R), i.e. white ~ 30% blend. Rim is identical on all sides (no top-only highlight) apart from the lens band above.
Bottom edge of circles: 1px (248,192,240) over (233,168,242).

### Shadow
Light, on pink (R channel darkening of raw 255): sides at rim 9.0% -> 7.1%@4pt(12px) -> 4.3%@12pt -> 2%@20pt -> ~0 by ~36pt (108 px). Top: 5.9% at rim, ~0 at 25pt. Bottom: 13% at rim, declining slowly (8.6% at 20pt, overlaps circles): y-offset ~+4..6pt, blur radius ~30pt (sigma ~12pt), peak alpha ~0.13 black (soft, unshaped; roughly gaussian). Dark: ~1-2% (invisible against saturated stripes): black shadow alpha <=0.02, extent same.
Each glass element has its own shadow (circles, buttons also).

### Back button glass circle (44pt @16,62)
Light on near-white bar (bg 252-255): fill ~(253,253,253) ~ the bg (white veil), 1 px rim (255), chevron (25,25,25) ~0.1 black. Shadow: bg 252 -> 248 at left edge, 247 at right; below the circle 244 (4.3%) fading to 253 by ~60 px; top only 0.8%: soft shadow y-offset ~+3pt, blur ~20pt, alpha ~0.04 black.
Dark: fill (25,25,25) = white at 10% over black, rim 1px (54,54,54), chevron (244), no visible shadow on black.
Over scrolled text: no text reaches under the button in the fixtures (Large Title starts at y>=107pt, button bottom 106pt), so not measured.

### Scroll-edge effect (typography-scrolled / -dark, symmetric)
No bar fill and no separator. Just under the bar's bottom edge it is mainly a veil with alpha toward the background colour (stems stay fairly sharp there). Correction (2026-11-17, deeper scrolls): further up, behind the inline title and the status bar, iOS also blurs and fades much harder (content is nearly invisible at the top of the screen); see "Long list" below. Overlay alpha derived from a glyph stem (v = alpha*bg for black text, same numbers in dark with white text):
pt: 107-118 (px 321-354): alpha ~0.40-0.44 plateau; 120: 0.27; 122: 0.16; 124: 0.13; 128: 0.095; 132: 0.063; 136: 0.04; 140: 0.024; 144: 0.016; ~148: 0. (Bar bottom = 116pt; the veil ends sharply after it then has a ~30pt tail; text above ~107pt is clipped by the glyph/frame top so the upper ramp is not observed.)
Bar background: light page gradient (252,252,252) at y<=84pt -> 253@88 -> 254@108 -> 255@128pt (3 levels, ~1.2%); dark: exactly 0 (nothing). Status bar area has the same tint.

### Long list and large-title collapse (captured 2026-11-17)
New showcase screen "Long list" (`LongListScreen`, 30 text rows, large title) so the large-title collapse and the
scroll edge can be captured at the default text size; adding its home row means `home` and `home-dark` were
recaptured the same day. Offsets are from `Row N` cell rects (cells are 52pt, Row 1 at 208.33 unscrolled).
- `long-list`: unscrolled (large title expanded, bar 62..168).
- `long-list-collapsed`: offset 52.0 = exactly the collapse distance; `scroll down --pixels 62 --duration-ms 1500`.
  iOS never rests between 0 and 52: a release there snaps to expanded or collapsed (bar 106 -> 54pt; the large title
  scrolls with the content and is hidden under the inline bar, the centred inline title takes over).
- `long-list-scrolled`: offset 775.66 (`scroll down 0.3`, fling stopped short of the 948pt maximum). Row 14's cell is
  at y 108.67 - use a row fully on screen: rects of rows clipped by the screen top are reported clipped (y = 0).
- At AX5 (accessibility-extra-extra-extra-large) the bar collapses after 73pt instead (large title 71.67pt tall, band
  = title + 1.33); the large title, section headers (Headline semibold) and chevrons (21.7x37.3) grow with Dynamic Type.
Scroll-edge model (fitted to typography-scrolled at 40pt, Typography at 337pt and AX5 home at 206pt): the measured veil
ramp holds up to 8pt above the bar's bottom, then rises to 0.95 alpha at the screen top; a blur (sigma 1.5pt) is mixed
in fully at the top, 0.55 at the bar's bottom, gone 7.75pt below it (`EDGE_*` in `simcore-render/src/glass.rs`).

## Alert and sheet (captured 2026-11-18; Controls second section "Show alert" / "Show sheet")
Captured on a dedicated iPhone 17 Pro / iOS 26.5 simulator ("SimCore iPhone 17 Pro"); its `home` matched `home.png` (0.01% of px differ by >24). Same procedure as above (build, install, agent-device, `--normalize-status-bar`, retake until island; dark PNGs cannot show the island).
New states: `controls` and `controls-changed` (recaptured, now with the extra section: cells "Show alert" 16,423.33 370x52 and "Show sheet" 16,475.33 370x52, second card starts y=423.33),
`alert` / `alert-dark` (Controls -> tap "Show alert", 3 s settle), `sheet` / `sheet-dark` (Controls -> tap "Show sheet", 4 s settle). Dark via `settings appearance dark`. Back-to-back shots were pixel-identical except the status-bar/island strip.
In the alert PNGs the Controls screen stays visible underneath; the alert snapshot has no divider nodes: iOS 26 alerts have two separate capsule buttons side by side, no lines.

### Alert (alert.json, pt x,y,w,h)
| element | frame / value |
|---|---|
| alert group "Reset volume?" (card) | 41,375, 320x152; centred x=201, centre y=451 (screen centre 437 + 14); corner radius ~34 (fit from the corner profile: card edge offset 13pt at 7pt below top) |
| title "Reset volume?" | 71,397, 260x20.33 (bold ~17pt, centred; 30 inset from card) |
| message "Volume goes back to 0." | 71,424.67, 260x18 (~13-14pt, secondary colour, centred) |
| Cancel button | 57,463, 140x48 (capsule r=24, hit frame; painted fill ~y 466..491 px-sampled at its text row) |
| Reset button (destructive) | 205,463, 140x48; gap between buttons 8, inset to card 16 left/right, 16 bottom (463+48=511, card bottom 527) |
| button dividers | none (buttons are filled capsules) |
| dimming | whole screen incl. status area and safe areas; covered cells are reported `covered` in the snapshot |
| card edge | 1px lighter rim (light 251,251,252 at the top edge; dark 52,52,54), a soft darker shadow just below the card (light 183-187 on dim 194; dark none visible on black) |

### Sheet (sheet.json, pt)
| element | frame / value |
|---|---|
| sheet top y | 62 (screen 874, so sheet is 812 tall, to the screen bottom); full width x 0..402 |
| corner radius (top) | ~40 (fit: left edge x=27.3/14.7/6.0 at 2/8/18pt below the top -> r~38-40), only top corners rounded |
| grabber | none (no pixel change along x=201 between y=62 and the first row at y=97) |
| presenting screen | NOT visible scaled back: above the sheet (y<62) it is just the dim (light 194,194,198 = grouped bg 242,242,247 x 0.8; dark 0,0,0); no inset/scale of the Controls screen, no nav bar visible |
| status bar / island | island at its usual 138.33,14 (black); status text stays on the dim |
| collection view | 0,62, 402x812 (List, inset grouped) |
| row "Volume is 3 of 10" | cell 16,97, 370x52 (text frame fills the cell; "3" bold, "of 10" secondary: `Text("Volume is ") + Text("3").bold() + Text(" of 10").foregroundStyle(.secondary)` style) |
| row "Done" | cell 16,149, 370x52 (blue button text, left aligned) |
| first row offset | 97 - 62 = 35 under the sheet top (no nav bar; the list has top section inset ~35) |

### Colors (sampled from the PNGs, RGB)
| thing | light | dark |
|---|---|---|
| dim overlay over the app (effective) | black alpha ~0.20 (grouped bg 242,242,247 -> 194,194,198; white row card 255 -> 204) | black alpha ~0.46 (row card 28,28,30 -> 15,15,16; bg stays 0,0,0) |
| alert card fill | 238,238,239 (title area 236,236,239) | 22,22,23 (19,19,19 near the top) |
| alert button fill (Cancel and Reset, same) | 209,209,210 | 47,47,48 |
| alert title / message text | 0,0,0 / ~107,107,108 | 255 / ~162 |
| Cancel text | 0,0,0 | 255,255,255 |
| Reset text (destructive) | 255,56,60 | ~255,66,69 |
| sheet background (List grouped) | 242,242,247 | 28,28,30 (#1C1C1E, elevated grouped bg, NOT black) |
| sheet row card | 255,255,255 | 44,44,46 (#2C2C2E) |
| area above sheet (dimmed presenter / status bar) | 194,194,198 | 0,0,0 |

### Round 2 (2026-11-18): Controls second section now has 5 buttons; half sheet, nested sheet, dialog, cover
All six earlier Controls/alert/sheet states were recaptured (background changed: the second card now holds "Show alert" 16,423.33 / "Show sheet" 475.33 / "Show half sheet" 527.33 / "Show dialog" 579.33 / "Show cover" 631.33, each 370x52; card 423.33..683.33).
New: `sheet-medium`, `sheet-medium-dark` (Show half sheet), `sheet-nested` (half sheet -> "Open another"), `dialog`, `dialog-dark` (Show dialog), `cover` (Show cover). Always set appearance BEFORE opening the presentation:
a popover switched to dark while open kept light styling (grey popover with black text), so `dialog-dark` and `alert-dark` were retaken after `settings appearance dark` + relaunch.

#### Half sheet (`.presentationDetents([.medium, .large])` + visible drag indicator), sheet-medium.json/png
| element | value |
|---|---|
| shape | NOT edge-to-edge: a floating card inset 8pt left/right: x 8..394 (w 386), y 415..866 (h ~451), 8pt gap to the screen bottom (874). Top corner radius ~36 (edge offset from the left at 1/2/4/8pt below top: 30/25/19.7/13.3 from x=8); bottom corners ~device-corner-like (card is narrower at y=850: x 11.3..390.3) |
| sheet top y | 415 (medium detent = about 52% of 874 less the inset); presenting Controls screen stays full size behind it (NOT scaled), dimmed |
| dim | same as sheet: black alpha ~0.20 light (242,242,247 -> 194,194,198, white card 255 -> 204); dark: bg stays 0, card 28,28,30 -> 15,15,16 |
| fill | translucent glass: blurred list rows of the presenter show through (blurred "Show ..." labels at x~25-160). light (236,236,236) at top -> (233,233,235) lower; dark (33,33,34) -> (30,30,30). 1px lighter rim at top edge (light 250,250,252; dark 70,70,70) |
| grabber (visual) | x 183.67..218.33 (w ~34.7), y 419.67..424.67 (h 5, capsule), 4.67 below the sheet top; light (177,177,180), dark (103,103,108) |
| grabber (snapshot button "Sheet Grabber") | 164.51,410.23 72.98x24 (hit frame) |
| content cards | text cell "Drag down to dismiss" 23.36,448.63 355.27x49.93; "Open another" 23.36,498.56 355.27x49.93 (370x52 at scale 0.96 = reported snapshot geometry; card visual x 24..378, y 448.5..547.6, one inset-grouped card, fill light (222,222,222)/dark ~(46,46,47)), separator at y~497.5 |
| "Open another" text | blue (0,136,255) |
| text runs ("Drag " + italic "down" + " to " + underlined "dismiss") | baseline y ~479.67 (ink bottom), text starts x 40.3, "down" italic (oblique, visible slant; approx x 79..118), "dismiss" underlined: underline x 141.3..196.7 (exactly the word width, 55.4pt), thickness 1.0pt (px rows 481.67-482.67), top of underline 2.0pt below the baseline, colour = text colour (0,0,0) |

#### Nested sheet (sheet-nested.json/png: half sheet -> "Open another")
| element | value |
|---|---|
| second sheet | standard (full) sheet: top y=72 (not 62; 10pt lower than a single presented sheet), to the screen bottom; full width; top corner radius ~38; List first row cell at y=107 (35 below top), rows "Volume is 7 of 10" 16,107 370x52 and "Done" 16,159 370x52 (note: shows 7 here, from the app's nested content) |
| first (half) sheet | NOT visible: the dim above the second sheet (y 51..72 and the corners, x<27) is plain (194,194,198), nothing scaled-back or peeking above it; Controls also not visible |
| dim | single level: (194,194,198), not compounded by the second sheet |
| colors | sheet bg (242,242,247), row card (255,255,255) as the single sheet |

#### Confirmation dialog (`.confirmationDialog("Volume preset", titleVisibility: .visible)` attached to the "Show dialog" Button; recaptured after moving it from the List onto the button): iOS 26 shows a POPOVER anchored to that button, not a bottom action sheet
| element | value |
|---|---|
| container | popover card "Volume preset" group 138,501, 240x208.33 (visual x 138..377, y 501..709, corner radius ~40). It sits to the RIGHT of the source row (the "Show dialog" row is 16,579.33 370x52), vertically centred on neither: card spans 501..709 around the button (centre y 605) |
| arrow | on the card's LEFT edge pointing LEFT toward the button text: tip at about (130, 605) (button/row centre y=605.3, label text ends x~124), base on the card edge x=138 |
| title | "Volume preset" 168,523 180x20.33, bold ~17pt, left aligned (30 inset from card) |
| message | "Pick a preset level." 168,551 180x18, ~14pt secondary (light 110,110,111; dark 161,162,162) |
| buttons | "Mute" (destructive, red text) 154,589.33 208x48; "Max" 154,645.33 208x48; capsule fills, gap 8, inset 16 from the card sides (138 -> 154), 16 below the last button (693.33 -> 709) |
| Cancel | NOT shown (popover style omits it; dismiss by tapping outside: snapshot element "dismiss popup" 0,0 402x874) |
| separators | none (filled capsule buttons) |
| dim | NONE: the Controls screen is not dimmed (white row card stays 255; dark card stays 28,28,30; bg 242,242,247 / 0). Only a soft shadow around the card (right of it, card 2 at x=370 reads 248 instead of 255 / dark 25,26,27 instead of 28,28,30) and the backdrop under the card is blurred (rows "Show half sheet" are cut by the card and hazy near it) |
| colors light | card fill (244,245,247) top -> (242,242,245) bottom; button fill (215,216,217); Mute (255,56,60); Max and title (0,0,0) |
| colors dark | card fill (20,23,23) top -> (16,18,17) bottom; button fill (46,49,49); Mute (255,66,69); Max and title (243,243,243) |
| tapped source | no longer highlighted (the earlier grey "More" was the wrong anchor; now "More" is still its normal gray prominent fill) |

#### Full screen cover (cover.png/json, cover-dark.png/json)
| element | value |
|---|---|
| background | (255,255,255) white edge to edge, incl. under status bar and home indicator area; covers the whole app, nothing behind visible, no dim, no nav bar |
| title "Full screen" | 115.5,405.5 171x40.67, large bold (~34pt), centred x=201 |
| Close | borderedProminent capsule 167.67,462.33 67x34.33, fill (0,136,255), white text (255,255,255) |
| layout | title+button stack spans y 405.5..496.7, centre y=451 = centre of the safe area (62..840), gap title->button 16 |
| status bar | normal (dark text on white), island present |
| cover-dark | background (0,0,0) pure black edge to edge; title (255,255,255); Close fill (0,145,255) with white text; frames identical to light |

#### Nested sheet, dark (sheet-nested-dark.png/json)
Same frames as `sheet-nested` (second sheet top y=72, first row 107, "Done" 159). Colors: sheet bg (28,28,30), row card (44,44,46); above the sheet (y<72, x outside the corners) plain (0,0,0) (dim over black unobservable); no first sheet visible. Also added the states `cover-dark` and `sheet-nested-dark` to the list above.
