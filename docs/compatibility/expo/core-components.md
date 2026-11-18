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
| ActivityIndicator | react-native | | ⬜ | ⬜ | | |
| RefreshControl | react-native | | ⬜ | ⬜ | | |
| SafeAreaView (core) | react-native | | ⬜ | ⬜ | | Use `react-native-safe-area-context` (below) |
| InputAccessoryView | react-native | | ⬜ | — | | iOS only |
