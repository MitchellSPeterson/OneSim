# Core components

Part of [Expo / React Native API coverage](README.md): status legend, summary and how to add a row.

| API | Package | Native | iOS | Android | Issue | Notes |
|---|---|---|---|---|---|---|
| View | react-native | `RCTView` | ✅ | 🟠 | | Core layout and touch targeting in the Android preview. |
| Text (nested spans, `numberOfLines`) | react-native | `RCTText` `RCTVirtualText` `RCTRawText` | ✅ | 🟠 | | Android uses Roboto. |
| Image | react-native | `RCTImageView` | 🟠 | 🟠 | [#50](https://github.com/MitchellSPeterson/OneSim/issues/50) | PNG, JPEG, GIF and WebP draw; animated GIF / WebP play from when the view appears, their loop count times (forever for 0), frames under 20 ms showing for 100 ms as on iOS; `tintColor`. Gap: `resizeMode="repeat"` draws as `cover` |
| TextInput | react-native | `RCTSinglelineTextInputView` `RCTMultilineTextInputView` `AndroidTextInput` | 🟠 | 🟠 | | Tapping outside does not blur. Host typing and editing callbacks work on Android. The software keyboard is pending. |
| ScrollView / FlatList / SectionList | react-native | `RCTScrollView` `RCTScrollContentView` `AndroidHorizontalScrollView` `AndroidHorizontalScrollContentView` | ✅ | 🟠 | | Vertical and horizontal scrolling in the Android preview. |
| Switch | react-native | `RCTSwitch` `AndroidSwitch` | ✅ | 🟠 | | iOS 26 switch shared with SwiftUI `Toggle`. Android uses a native-style track and thumb. |
| Modal | react-native | `RCTModalHostView` | ✅ | 🟠 | | Android uses a full-screen overlay; window flags and back dismissal are pending. |
| Pressable / Touchable* / Button | react-native | | ✅ | 🟠 | | JS components over View + touch. Android native ripple is pending. |
| KeyboardAvoidingView | react-native | | ✅ | ⬜ | [#11](https://github.com/MitchellSPeterson/OneSim/issues/11) | Lands in its final layout (keyboard events carry `duration: 0`, so it configures no LayoutAnimation) |
| ActivityIndicator | react-native | `ActivityIndicatorView` `AndroidProgressBar` | ✅ | 🟠 | [#50](https://github.com/MitchellSPeterson/OneSim/issues/50) | Small, large, `color`, `hidesWhenStopped`; spins clockwise one spoke every 125 ms from when it appears (`--wait` steps it; live viewers keep ticking); a numeric `size` draws small, as on iOS. Android draws a static circular indicator. |
| RefreshControl | react-native | `PullToRefreshView` `AndroidSwipeRefreshLayout` | 🟠 | 🟠 | | `refreshing` spinner band, `tintColor`; pulling 100 pt past the top fires `onRefresh`; no rubber-band/pull-progress drawing, `title`/`titleColor`/`progressViewOffset` ignored. Android draws a static indicator over the scroll content while refreshing. |
| SafeAreaView (core) | react-native | `SafeAreaView` | ✅ | ⬜ | | Pads by the device insets it overlaps (portrait); deprecated in RN 0.88, prefer `react-native-safe-area-context` |
| InputAccessoryView | react-native | | ⬜ | — | | iOS only |
