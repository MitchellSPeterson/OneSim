# Core components

Part of [Expo / React Native API coverage](README.md): status legend, summary and how to add a row.

| API | Package | Native | iOS | Android | Issue | Notes |
|---|---|---|---|---|---|---|
| View | react-native | `RCTView` | ✅ | 🟠 | | Core layout and touch targeting in the Android preview. |
| Text (nested spans, `numberOfLines`) | react-native | `RCTText` `RCTVirtualText` `RCTRawText` | ✅ | 🟠 | | Android uses Roboto. |
| Image | react-native | `RCTImageView` | 🟠 | 🟠 | | PNG/JPEG draw; GIF/WebP load (size, events) but do not draw; `tintColor`, `repeat` ignored |
| TextInput | react-native | `RCTSinglelineTextInputView` `RCTMultilineTextInputView` `AndroidTextInput` | 🟠 | 🟠 | | Tapping outside does not blur. Host typing and editing callbacks work on Android. The software keyboard is pending. |
| ScrollView / FlatList / SectionList | react-native | `RCTScrollView` `RCTScrollContentView` `AndroidHorizontalScrollView` `AndroidHorizontalScrollContentView` | ✅ | 🟠 | | Vertical and horizontal scrolling in the Android preview. |
| Switch | react-native | `RCTSwitch` `AndroidSwitch` | ✅ | 🟠 | | iOS 26 switch shared with SwiftUI `Toggle`. Android uses a native-style track and thumb. |
| Modal | react-native | `RCTModalHostView` | ✅ | 🟠 | | Android uses a full-screen overlay; window flags and back dismissal are pending. |
| Pressable / Touchable* / Button | react-native | | ✅ | 🟠 | | JS components over View + touch. Android native ripple is pending. |
| KeyboardAvoidingView | react-native | | ✅ | ⬜ | [#11](https://github.com/MitchellSPeterson/SimCore/issues/11) | Lands in its final layout (keyboard events carry `duration: 0`, no LayoutAnimation) |
| ActivityIndicator | react-native | `ActivityIndicatorView` `AndroidProgressBar` | 🟠 | 🟠 | | Small, large, `color`, `hidesWhenStopped`; static frame (no spin), spoke fade runs opposite to iOS; numeric `size` draws small. Android draws a static circular indicator. |
| RefreshControl | react-native | `PullToRefreshView` `AndroidSwipeRefreshLayout` | 🟠 | 🟠 | | `refreshing` spinner band, `tintColor`; pulling 100 pt past the top fires `onRefresh`; no rubber-band/pull-progress drawing, `title`/`titleColor`/`progressViewOffset` ignored. Android draws a static indicator over the scroll content while refreshing. |
| SafeAreaView (core) | react-native | `SafeAreaView` | ✅ | ⬜ | | Pads by the device insets it overlaps (portrait); deprecated in RN 0.88, prefer `react-native-safe-area-context` |
| InputAccessoryView | react-native | | ⬜ | — | | iOS only |
