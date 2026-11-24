# SwiftUI popovers references

Captured from `examples/swiftui-popovers` on the iPhone 18 Pro simulator running iOS 27.2. Screenshots are
1206 × 2622 px. The iOS 26.5 (iPhone 17 Pro) set is in `../../swiftui-popovers-ios/`, captured the same way.

Build the reference with:

```sh
scripts/build-ios-sim.sh examples/swiftui-popovers target/ios/OneSimPopovers.app dev.simcore.popovers
```

Install it with `agent-device install dev.simcore.popovers target/ios/OneSimPopovers.app`. Start each capture with
`open dev.simcore.popovers --relaunch` (light appearance; `-dark` states use `settings appearance dark`, reset
afterwards). The pasteboard is emptied (`clipboard write ""`) before every state except `pasted`. After the actions,
wait 1.5 s (2.5 s for the context menus, 6 s for the share sheet so its app row loads), save `snapshot --json` as
`<name>.json`, then capture `screenshot --pixel-density 3 --normalize-status-bar` and repeat until the Dynamic Island is
visible (light states; dark PNGs cannot show it). Taps are by point coordinates; long presses hold 1000 ms.

| State | Actions after launch |
| --- | --- |
| `popovers` / `popovers-dark` | Home |
| `popover-sheet` | Tap Popover as sheet at (201, 264) |
| `popover-bubble` / `popover-bubble-dark` | Tap Popover bubble at (201, 305) |
| `styled-sheet` | Tap Styled sheet at (201, 345) |
| `context-menu` / `context-menu-dark` | Long-press Hold for menu at (201, 401) |
| `context-preview` | Long-press Hold for preview at (201, 457) |
| `share` | Tap the first ShareLink (Share…) at (201, 499) |
| `share-link` | Tap Share link at (201, 541) |
| `pasted` | `clipboard write "Copied text"`, relaunch, tap Paste at (201, 590); no permission prompt appeared |

Island tries (light states): popovers 225, popover-sheet 135, popover-bubble 93, styled-sheet 19, context-menu 24,
context-preview 56, share 14, pasted 67.

`scripts/compare-ios.sh` runs these states in OneSim (`pasted` with `--pasteboard "Copied text"`).
