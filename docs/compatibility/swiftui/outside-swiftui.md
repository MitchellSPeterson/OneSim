# Outside SwiftUI

Part of [SwiftUI API coverage](README.md): status legend, summary and how to add a row. Hand-maintained (not generated).

APIs SwiftUI apps use from other modules (Combine, Observation), reimplemented in OneSim's `SwiftUI` module.

| API | Since | iOS | Issue | Notes |
|---|---|---|---|---|
| `ObservableObject` | 13 | 🟠 | | `objectWillChange` is a stub |
| `Published` | 13 | ✅ | | Any set or mutation marks the view dirty |
| `Observable` (macro) | 17 | ⬜ | | Observation framework; pairs with `Bindable` |
