# SwiftUI popovers references

Captured from `examples/swiftui-popovers` on the iPhone 17 Pro simulator running iOS 26.5. Screenshots are
1206 × 2622 px. The iOS 27.2 (iPhone 18 Pro) set is in `../ios27/swiftui-popovers-ios/`, captured the same way.

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
| `pasted` | `clipboard write "Copied text"`, relaunch, tap Paste at (201, 590); no permission prompt appeared |

`SIMCORE_IOS=26.5 scripts/compare-ios.sh` runs these states in OneSim.
