# React Native modules

Part of [Expo / React Native API coverage](README.md): status legend, summary and how to add a row.

| API | Package | Native | iOS | Android | Issue | Notes |
|---|---|---|---|---|---|---|
| Platform | react-native | `PlatformConstants` | ✅ | ⬜ | | Reports the session's iOS release (`osVersion`: 27.2 or 26.5) |
| Dimensions / PixelRatio | react-native | `DeviceInfo` | 🟡 | ⬜ | | From the session viewport |
| Appearance / useColorScheme | react-native | `Appearance` | 🟡 | ⬜ | | From the session's dark flag |
| I18nManager | react-native | `I18nManager` | ✅ | ⬜ | | LTR, `en_US` |
| StatusBar | react-native | `StatusBarManager` | 🟠 | ⬜ | | Calls accepted and logged; the host draws its own status bar |
| AppState | react-native | `AppState` | 🟠 | ⬜ | | Always `active`; no background/foreground transitions |
| Settings | react-native | `SettingsManager` | ✅ | — | | iOS only |
| AccessibilityInfo | react-native | `AccessibilityInfo` | ✅ | ⬜ | | All settings off |
| Keyboard | react-native | `KeyboardObserver` | ✅ | ⬜ | [#11](https://github.com/MitchellSPeterson/SimCore/issues/11) | `keyboardWill/Did` `Show`/`Hide`/`ChangeFrame` with the host keyboard's frame, `dismiss`, `isVisible`, `metrics`; `duration: 0` |
| fetch / XMLHttpRequest | react-native | `Networking` | 🟠 | ⬜ | | No cookie jar (`clearCookies` returns false) |
| Blob / FileReader | react-native | `BlobModule` `FileReaderModule` | 🟠 | ⬜ | | Minimal; `sendOverSocket` unsupported |
| WebSocket | react-native | `WebSocketModule` | ✅ | ⬜ | | |
| Image.getSize / prefetch | react-native | `ImageLoader` | ✅ | ⬜ | | Shared download cache |
| Linking | react-native | `LinkingManager` | 🟠 | ⬜ | | No launch URL; `openURL` unsupported |
| Clipboard (deprecated core) | react-native | `Clipboard` | 🟡 | ⬜ | | `modules.simulate clipboard` |
| Animated | react-native | | 🟠 | ⬜ | | No native driver: JS-driven, one tree per frame |
| LayoutAnimation | react-native | | ⛔ | ⬜ | | |
| Alert | react-native | `AlertManager` | 🟠 | ⬜ | [#7](https://github.com/MitchellSPeterson/SimCore/issues/7) | `Alert.alert` drawn as the iOS 26 alert (SwiftUI `.alert` rendering), button roles, taps run `onPress`; `Alert.prompt` unsupported |
| ActionSheetIOS | react-native | `ActionSheetManager` | ⛔ | — | | |
| Share | react-native | `ShareModule` | ⛔ | ⬜ | | |
| Vibration | react-native | `Vibration` | 🟡 | ⬜ | | Calls emit `module.called`; no haptic output |
| PushNotificationIOS | react-native | `PushNotificationManager` | ⛔ | — | | |
| Image store / editor | react-native | `ImageStoreManager` `ImageEditor` | ⛔ | ⬜ | | |
| Accessibility settings writes | react-native | `AccessibilityManager` | ⛔ | — | | |
| Dev tools settings | react-native | `DevToolsSettingsManager` | ⛔ | ⬜ | | |
| Runtime internals | react-native | `SourceCode` `ExceptionsManager` `LogBox` `DevSettings` `DevLoadingView` `DevMenu` `RedBox` `Timing` `NativePerformanceCxx` `NativeMicrotasksCxx` `NativeReactNativeFeatureFlagsCxx` `NativeDOMCxx` `EventDispatcher` | ✅ | ⬜ | | Errors go to `get_errors`; no redbox UI |
| Fabric UIManager | react-native | | 🟠 | ⬜ | | `findNodeAtPoint` and most view commands unsupported |
