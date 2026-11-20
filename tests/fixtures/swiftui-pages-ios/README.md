# SwiftUI page TabView references

Build the app against Apple SwiftUI with `scripts/build-ios-sim.sh examples/swiftui-pages target/ios/Pages.app dev.simcore.pages`. Capture on the iOS 26.5 iPhone 17 Pro in this directory and on the iOS 27.2 iPhone 18 Pro in `tests/fixtures/ios27/swiftui-pages-ios/`.

Use T3's agent-device in light appearance. Save a 3x normalized status-bar PNG and a JSON snapshot for each state. Retake the PNG until the Dynamic Island appears, as in the [showcase reference captures](../showcase-ios/README.md#dynamic-island-why-it-was-inconsistent-and-the-fix).

| State | Action |
| --- | --- |
| `pages-first` | Launch Pages. The first card is Sunrise. |
| `pages-programmatic` | Tap Next. The second card is Forest. |
| `pages-swiped` | Swipe left across Forest. The third card is Night. |
| `pages-inbox` | Tap Inbox in the bottom bar. |
