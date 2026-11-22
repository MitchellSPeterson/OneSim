# Expo SDK

Part of [Expo / React Native API coverage](README.md): status legend, summary and how to add a row.

| API | Package | Native | iOS | Android | Issue | Notes |
|---|---|---|---|---|---|---|
| Expo modules core | expo-modules-core | `ExpoModulesCore` | ✅ | ⬜ | | |
| Expo UI | @expo/ui | `ExpoUI` | 🟠 | 🟠 | | Host, Text, Column, Row, Button, Switch, TextInput, Slider, RNHostView and ScrollView. Basic modifiers and React callbacks. Android text fields use host keyboard events. |
| expo/fetch | expo | `ExpoFetchModule` | ✅ | ⬜ | | |
| Asset | expo-asset | `ExpoAsset` | ✅ | ⬜ | [#50](https://github.com/MitchellSPeterson/OneSim/issues/50) | `downloadAsync` downloads into Caches (`ExponentAsset-<md5>.<type>`, reused while the md5 matches); `file:` URIs pass through |
| Font | expo-font | `ExpoFontLoader` `ExpoFontUtils` | ✅ | ⬜ | [#50](https://github.com/MitchellSPeterson/OneSim/issues/50) | `loadAsync` (single files, `useFonts`) and font families (`loadFontFamilyAsync`, weight / style per face) register TTF / OTF / TTC files; `fontFamily` takes the alias, or a family or PostScript name in a loaded file, on `Text`, nested spans and text inputs (variable fonts follow `fontWeight`). `getLoadedFonts`. `renderToImageAsync` draws the string in the host (a loaded family by name, else the system font; `size`, `color`, `lineHeight`) into a PNG in the app's tmp dir |
| Keep awake | expo-keep-awake | `ExpoKeepAwake` | ✅ | ⬜ | | No-op |
| Splash screen | expo-splash-screen | `ExpoSplashScreen` | ✅ | ⬜ | | |
| System UI | expo-system-ui | `ExpoSystemUI` | ✅ | ⬜ | | Root background color |
| Status bar | expo-status-bar | | 🟠 | 🟠 | | Over RN `StatusBar` (above) |
| Device | expo-device | `ExpoDevice` | 🟡 | 🟡 | | Model and OS of the session's device profile and iOS release. Android simulates the Pixel 10a. |
| Constants | expo-constants | `ExponentConstants` | ✅ | ✅ | | |
| Linking | expo-linking | `ExpoLinking` | 🟡 | ⬜ | [#50](https://github.com/MitchellSPeterson/OneSim/issues/50) | `getLinkingURL` is the launch URL, then the last URL opened (`onURLReceived` / `useLinkingURL`, host `open_url`); `clearInitialURL`. Expo Router starts at the launch URL's route |
| Image | expo-image | `ExpoImage` | ✅ | ⬜ | [#50](https://github.com/MitchellSPeterson/OneSim/issues/50) | PNG, JPEG, GIF and WebP draw; animated GIF / WebP play (`autoplay={false}` holds the first frame; `startAnimating` / `stopAnimating` resume and hold in place). `blurhash:/` and `thumbhash:/` sources and `placeholder` (decoded by the host, as expo-image's loaders do), `cacheKey`; `Image.loadAsync` (`maxWidth` / `maxHeight`) gives an `ImageRef` usable as a source; `generateBlurhashAsync` (bit-identical to iOS) / `generateThumbhashAsync`; `prefetch` (with headers), `getCachePathAsync`, `writeToCacheAsync`, `readFromCacheAsync`, `clearMemoryCache`, `clearDiskCache`; `lockResourceAsync` / `unlockResourceAsync`, `reloadAsync`; `onLoad` reports `isAnimated` and `mediaType` |
| Symbols | expo-symbols | `SymbolModule` | ✅ | — | | SF Symbols; Phosphor in fallback mode |
| Glass effect | expo-glass-effect | `ExpoGlassEffect` | ✅ | — | | Liquid Glass |
| Clipboard | expo-clipboard | `ExpoClipboard` | 🟡 | ⬜ | | `modules.simulate clipboard` |
| Haptics | expo-haptics | `ExpoHaptics` | 🟡 | ⬜ | | Emits `module.called` |
| Location | expo-location | `ExpoLocation` | 🟡 | ⬜ | | Foreground only (default Apple Park); background, geocoding, geofencing, heading unsupported |
| Web browser | expo-web-browser | `ExpoWebBrowser` | ⛔ | ⬜ | | Blocks `expo-auth-session` |
| Camera | expo-camera | `ExpoCamera` | ⛔ | ⬜ | | |
| File system | expo-file-system | `FileSystem`, `ExponentFileSystem` | 🟠 | ⬜ | [#8](https://github.com/MitchellSPeterson/OneSim/issues/8) | Per-session sandbox (Documents, Caches, empty Bundle), kept across reloads; downloads, uploads, pickers, preview, watcher unsupported |
| Secure store | expo-secure-store | `ExpoSecureStore` | 🟡 | ⬜ | [#9](https://github.com/MitchellSPeterson/OneSim/issues/9) | Per-session store, kept across reloads; `modules.simulate securestore` seeds keys (`null` deletes); no biometrics |
| SQLite | expo-sqlite | | ⬜ | ⬜ | | |
| Notifications | expo-notifications | | ⬜ | ⬜ | | |
| Image picker | expo-image-picker | | ⬜ | ⬜ | | |
| Document picker | expo-document-picker | | ⬜ | ⬜ | | |
| Media library | expo-media-library | | ⬜ | ⬜ | | |
| Audio | expo-audio | | ⬜ | ⬜ | | |
| Video | expo-video | | ⬜ | ⬜ | | |
| Sensors | expo-sensors | | ⬜ | ⬜ | | |
| Blur | expo-blur | `ExpoBlur` | 🟠 | 🟠 | | Fitted to iOS 26.5 for `default`/`light`/`dark`/`systemMaterial`. Android draws the tint used by the default blur method. Target-based blur is pending. |
| Linear gradient | expo-linear-gradient | `ExpoLinearGradient` | ✅ | 🟠 | | `colors`, `locations`, `start`/`end`. Android supports uniform corner rounding. |
| Localization | expo-localization | `ExpoLocalization` | ✅ | ⬜ | | Fixed en-US / USD / US units / gregorian; time zone from the host `TZ` (else America/Los_Angeles); no change events |
| Crypto | expo-crypto | `ExpoCrypto` | ✅ | ⬜ | | Digest (SHA-1/256/384/512, MD5), random bytes, UUID |
| Crypto AES | expo-crypto/aes | `ExpoCryptoAES` | ⛔ | ⬜ | | The classes exist so `expo-crypto` imports; every call rejects as unsupported |
| Application | expo-application | `ExpoApplication` | 🟡 | ⬜ | | Name, id, version and build from the app config; installation time is session start; fixed vendor id |
| Network | expo-network | `ExpoNetwork` | 🟡 | ⬜ | | Always Wi-Fi, connected, 192.168.1.2, airplane mode off; no `simulate` key or change events yet |
| Battery | expo-battery | `ExpoBattery` | 🟡 | ⬜ | | Full at 100%; `simulate battery {level, state, lowPowerMode}` updates it and emits the change events |
| Brightness | expo-brightness | `ExpoBrightness` | 🟡 | ⬜ | | Value kept in session state (default 0.5); permission always granted; no effect on rendering |
| Screen orientation | expo-screen-orientation | `ExpoScreenOrientation` | 🟡 | ⬜ | | Portrait only: `DEFAULT`, `PORTRAIT`, `PORTRAIT_UP` locks resolve, any other lock rejects as unsupported; no orientation change events |
| Sharing | expo-sharing | | ⬜ | ⬜ | | |
| Auth session | expo-auth-session | | ⬜ | ⬜ | | Needs expo-web-browser |
| Local authentication | expo-local-authentication | `ExpoLocalAuthentication` | 🟡 | ⬜ | | Face ID enrolled; `simulate localauth {success, error?}` sets the next result; `authenticateAsync` emits `module.called` |
| Contacts | expo-contacts | | ⬜ | ⬜ | | |
| Calendar | expo-calendar | | ⬜ | ⬜ | | |
| Store review | expo-store-review | `ExpoStoreReview` | 🟡 | ⬜ | | Available; `requestReview` emits `module.called` and shows nothing |
| Tracking transparency | expo-tracking-transparency | `ExpoTrackingTransparency` | 🟡 | — | | iOS only. Permission key `tracking` (`simulate permissions {tracking}`); advertising id is the zero UUID (JS returns null) |
| Mail composer | expo-mail-composer | | ⬜ | ⬜ | | |
| SMS | expo-sms | | ⬜ | ⬜ | | |
| Speech | expo-speech | | ⬜ | ⬜ | | |
| Print | expo-print | | ⬜ | ⬜ | | |
| Maps | expo-maps | | ⬜ | ⬜ | | |
| Task manager / background task | expo-task-manager | | ⬜ | ⬜ | | |
| Updates | expo-updates | | ⬜ | ⬜ | | |
