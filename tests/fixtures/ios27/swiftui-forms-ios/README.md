# SwiftUI forms and containers references

Captured from `examples/swiftui-forms` on the iPhone 18 Pro simulator running iOS 27.2. Screenshots are
1206 × 2622 px. The iOS 26.5 set (iPhone 17 Pro) is in `../../swiftui-forms-ios/`, captured the same way.

Build the reference with:

```sh
scripts/build-ios-sim.sh examples/swiftui-forms /tmp/OneSimForms.app dev.simcore.forms
```

Install the app on the pinned device and open it through agent-device. Start each capture with a relaunch.
Capture with `screenshot --pixel-density 3 --normalize-status-bar` and repeat until the Dynamic Island is
visible (light states). Dark states use `settings appearance dark`. "Scroll" is `scroll down 0.6` on the home
screen (0.5 for Boxes) so the lower links are on screen; taps by label use `press 'label="…"'`.

| State | Actions after launch |
| --- | --- |
| `forms` / `forms-dark` | Home |
| `forms-expanded` | Tap the Advanced row at (200, 437) |
| `grouped`, `plain`, `inset` | Tap the link of that name |
| `sidebar` / `sidebar-dark` | Tap Sidebar |
| `sidebar-expanded` | Tap Sidebar, then the Vegetables header at (200, 298) |
| `boxes` / `boxes-dark` | Scroll, tap Boxes |
| `columns` | Scroll, tap Columns |
| `empty` | Scroll, tap Empty |
| `empty-search` | Scroll, tap Empty, then Search |
| `outline` | Scroll, tap Outline |
| `outline-expanded` | Scroll, tap Outline, then Documents at (200, 177) |
| `edit` | Scroll, tap Edit |
| `edit-active` | Scroll, tap Edit, then the Edit button row at (200, 420) |
| `edit-delete` | As `edit-active`, then Anna's delete control at (47, 177) |
| `swipe-trailing` | Scroll, tap Swipe, `gesture pan 300 177 -150 0 600` |
| `swipe-leading` | Scroll, tap Swipe, `gesture pan 100 177 100 0 600` |
| `table` | Scroll, tap Table |
