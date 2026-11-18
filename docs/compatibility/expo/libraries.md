# Navigation, animation and community libraries

Part of [Expo / React Native API coverage](README.md): status legend, summary and how to add a row.

| API | Package | Native | iOS | Android | Issue | Notes |
|---|---|---|---|---|---|---|
| Expo Router (Stack, NativeTabs) | expo-router | | ✅ | ⬜ | | `navigation.changed` events |
| Native stack | react-native-screens | `RNSModule` `RNSScreenStack` `RNSScreen` `RNSScreenStackHeaderConfig` `RNSScreenStackHeaderSubview` `RNSScreenContentWrapper` | 🟠 | ⬜ | | Top screen only; no push/pop transition animation |
| Native tabs | react-native-screens | `RNSTabsHostIOS` `RNSTabsScreenIOS` | ✅ | ⬜ | | Floating Liquid Glass tab bar |
| Safe area | react-native-safe-area-context | `RNCSafeAreaContext` `RNCSafeAreaProvider` `RNCSafeAreaView` | 🟡 | ⬜ | | Insets from the device profile |
| Reanimated / worklets | react-native-reanimated, react-native-worklets | `ReanimatedModule` `WorkletsModule` | 🟠 | ⬜ | | Worker runtimes, sensors, CSS animations, exiting/layout/shared-element animations unsupported |
| Gesture handler | react-native-gesture-handler | `RNGestureHandlerModule` `RNGestureHandlerDetector` `RNGestureHandlerButton` | 🟠 | ⬜ | [#10](https://github.com/MitchellSPeterson/SimCore/issues/10) | Tap, Pan, LongPress, native buttons (`GestureDetector`, hooks, legacy handlers, `RectButton`, `Pressable`, `ReanimatedSwipeable`), JS + worklet callbacks; one finger, no `waitFor`/`blocksHandlers`, `onTouches*`, Fling/Pinch/Rotation/Hover/Manual |
| SVG | react-native-svg | | ⬜ | ⬜ | | |
| Async storage | @react-native-async-storage/async-storage | | ⬜ | ⬜ | | |
| React Navigation (JS stack, bottom tabs) | @react-navigation/* | | ⬜ | ⬜ | | Untested |
