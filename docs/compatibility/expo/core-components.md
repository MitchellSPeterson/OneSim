# Core components

Part of [Expo / React Native API coverage](README.md): status legend, summary and how to add a row.

| API | Package | Native | iOS | Android | Issue | Notes |
|---|---|---|---|---|---|---|
| View | react-native | `RCTView` | ✅ | ⬜ | | |
| Text (nested spans, `numberOfLines`) | react-native | `RCTText` `RCTVirtualText` `RCTRawText` | ✅ | ⬜ | | |
| Image | react-native | `RCTImageView` | 🟠 | ⬜ | | PNG/JPEG draw; GIF/WebP load (size, events) but do not draw; `tintColor`, `repeat` ignored |
| TextInput | react-native | `RCTSinglelineTextInputView` `RCTMultilineTextInputView` | 🟠 | ⬜ | | Tapping outside does not blur |
| ScrollView / FlatList / SectionList | react-native | `RCTScrollView` `RCTScrollContentView` | ✅ | ⬜ | | |
| Switch | react-native | `RCTSwitch` | ✅ | ⬜ | | iOS 26 switch shared with SwiftUI `Toggle` |
| Modal | react-native | `RCTModalHostView` | ✅ | ⬜ | | |
| Pressable / Touchable* / Button | react-native | | ✅ | ⬜ | | JS components over View + touch |
| KeyboardAvoidingView | react-native | | ✅ | ⬜ | [#11](https://github.com/MitchellSPeterson/SimCore/issues/11) | Lands in its final layout (keyboard events carry `duration: 0`, no LayoutAnimation) |
| ActivityIndicator | react-native | `ActivityIndicatorView` | 🟠 | ⬜ | | Small, large, `color`, `hidesWhenStopped`; static frame (no spin), spoke fade runs opposite to iOS; numeric `size` draws small |
| RefreshControl | react-native | `PullToRefreshView` | 🟠 | ⬜ | | `refreshing` spinner band, `tintColor`; pulling 100 pt past the top fires `onRefresh`; no rubber-band/pull-progress drawing, `title`/`titleColor`/`progressViewOffset` ignored |
| SafeAreaView (core) | react-native | `SafeAreaView` | ✅ | ⬜ | | Pads by the device insets it overlaps (portrait); deprecated in RN 0.88, prefer `react-native-safe-area-context` |
| InputAccessoryView | react-native | | ⬜ | — | | iOS only |
