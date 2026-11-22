# React Native modules

Part of [Expo / React Native API coverage](README.md): status legend, summary and how to add a row.

| API | Package | Native | iOS | Android | Issue | Notes |
|---|---|---|---|---|---|---|
| Platform | react-native | `PlatformConstants` | ✅ | ✅ | | Reports the session's iOS release (`osVersion`: 27.2 or 26.5). Android reports API 37 and release 17. |
| Dimensions / PixelRatio | react-native | `DeviceInfo` | 🟡 | 🟡 | | From the session viewport |
| Appearance / useColorScheme | react-native | `Appearance` | 🟡 | 🟡 | | From the session's dark flag |
| I18nManager | react-native | `I18nManager` | ✅ | ⬜ | | LTR, `en_US` |
| StatusBar | react-native | `StatusBarManager` | 🟠 | 🟠 | | Calls accepted and logged; the host draws its own status bar. Android applies icon style and visibility. |
| AppState | react-native | `AppState` | 🟠 | ⬜ | | Always `active`; no background/foreground transitions |
| Settings | react-native | `SettingsManager` | ✅ | — | | iOS only |
| AccessibilityInfo | react-native | `AccessibilityInfo` | ✅ | ⬜ | | All settings off |
| Keyboard | react-native | `KeyboardObserver` | ✅ | 🟠 | [#11](https://github.com/MitchellSPeterson/OneSim/issues/11) | `keyboardWill/Did` `Show`/`Hide`/`ChangeFrame` with the host keyboard's frame, `dismiss`, `isVisible`, `metrics`; `duration: 0`. Android emits `keyboardDidShow` and `keyboardDidHide` with a simulated frame. |
| fetch / XMLHttpRequest | react-native | `Networking` | 🟠 | 🟠 | | No cookie jar (`clearCookies` returns false) |
| Blob / FileReader | react-native | `BlobModule` `FileReaderModule` | 🟠 | ⬜ | | Minimal; `sendOverSocket` unsupported |
| WebSocket | react-native | `WebSocketModule` | ✅ | ⬜ | | |
| Image.getSize / prefetch | react-native | `ImageLoader` | ✅ | 🟠 | | Shared download cache |
| Linking | react-native | `LinkingManager` `IntentAndroid` | 🟡 | 🟠 | [#50](https://github.com/MitchellSPeterson/OneSim/issues/50) | iOS: `getInitialURL` is the launch URL (`create_session` `launch_url`, CLI `--launch-url`); host `open_url` (`--open-url`) fires `url`. `openURL` of the app's own scheme comes straight back as `url`; system URLs (http(s), tel, sms, mailto, FaceTime, Maps, App Store, `openSettings`) are handed to the simulated system (`module.called` `Linking.openURL`, the app stays in front); `canOpenURL` is true for both; any other scheme is false / rejects `Unable to open URL`, as is a string that is not a URL. Android: no launch URL, `openURL` unsupported |
| Clipboard (deprecated core) | react-native | `Clipboard` | 🟡 | ⬜ | | `modules.simulate clipboard` |
| Animated | react-native | | 🟠 | ⬜ | | No native driver: JS-driven, one tree per frame |
| LayoutAnimation | react-native | | ✅ | ⬜ | [#50](https://github.com/MitchellSPeterson/OneSim/issues/50) | `configureNext` / `create` / presets animate the next commit on the host: `update` moves and resizes, `create` / `delete` fade or scale (`opacity`, `scaleXY`, `scaleX`, `scaleY`) inserted and removed views; types `linear`, `easeIn`, `easeOut`, `easeInEaseOut`, `spring` (Fabric's interpolation, `springDamping`), `keyboard`; per-part `duration` / `delay`. The end callback runs once the longest part has played (guest clock). Headless screenshots show the animation's start unless `--wait` |
| Alert | react-native | `AlertManager` | 🟠 | ⬜ | [#7](https://github.com/MitchellSPeterson/OneSim/issues/7) | `Alert.alert` drawn as the iOS 26 alert (SwiftUI `.alert` rendering), button roles, taps run `onPress`; `Alert.prompt` unsupported |
| ActionSheetIOS | react-native | `ActionSheetManager` | ✅ | — | [#50](https://github.com/MitchellSPeterson/OneSim/issues/50) | `showActionSheetWithOptions`: the iOS 26+ iPhone card (title, message, options in order with the cancel button last, `destructiveButtonIndex(es)`, `disabledButtonIndices`, `tintColor`, `cancelButtonTintColor`, `userInterfaceStyle`; a tap outside picks `cancelButtonIndex`), a popover at `anchor`. `showShareActionSheetWithOptions`: the share sheet (`url` / `message`; Copy puts it on the clipboard, other activities report `module.called` `ShareSheet`; `excludedActivityTypes` drops cells; dismissing calls back `completed: false`). `dismissActionSheet` |
| Share | react-native | `ShareModule` | ⛔ | ⬜ | | |
| Vibration | react-native | `Vibration` | 🟡 | ⬜ | | Calls emit `module.called`; no haptic output |
| PushNotificationIOS | react-native | `PushNotificationManager` | ⛔ | — | | |
| Image store / editor | react-native | `ImageStoreManager` `ImageEditor` | ⛔ | ⬜ | | |
| Accessibility settings writes | react-native | `AccessibilityManager` | ⛔ | — | | |
| Dev tools settings | react-native | `DevToolsSettingsManager` | ⛔ | ⬜ | | |
| Runtime internals | react-native | `SourceCode` `ExceptionsManager` `LogBox` `DevSettings` `DevLoadingView` `DevMenu` `RedBox` `Timing` `NativePerformanceCxx` `NativeMicrotasksCxx` `NativeReactNativeFeatureFlagsCxx` `NativeDOMCxx` `EventDispatcher` | ✅ | ⬜ | | Errors go to `get_errors`; no redbox UI |
| Fabric UIManager | react-native | | 🟠 | 🟠 | | `findNodeAtPoint` and most view commands unsupported. Android shares the Fabric host interface. |
