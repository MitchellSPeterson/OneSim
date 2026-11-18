# Expo SDK

Part of [Expo / React Native API coverage](README.md): status legend, summary and how to add a row.

| API | Package | Native | iOS | Android | Issue | Notes |
|---|---|---|---|---|---|---|
| Expo modules core | expo-modules-core | `ExpoModulesCore` | ✅ | ⬜ | | |
| expo/fetch | expo | `ExpoFetchModule` | ✅ | ⬜ | | |
| Asset | expo-asset | `ExpoAsset` | 🟠 | ⬜ | | `downloadAsync` unsupported (returns the URI) |
| Font | expo-font | `ExpoFontLoader` | 🟠 | ⬜ | | `loadAsync` unsupported: custom fonts fall back to system |
| Keep awake | expo-keep-awake | `ExpoKeepAwake` | ✅ | ⬜ | | No-op |
| Splash screen | expo-splash-screen | `ExpoSplashScreen` | ✅ | ⬜ | | |
| System UI | expo-system-ui | `ExpoSystemUI` | ✅ | ⬜ | | Root background color |
| Status bar | expo-status-bar | | 🟠 | ⬜ | | Over RN `StatusBar` (above) |
| Device | expo-device | `ExpoDevice` | 🟡 | ⬜ | | iPhone 17 Pro |
| Constants | expo-constants | `ExponentConstants` | ✅ | ⬜ | | |
| Linking | expo-linking | `ExpoLinking` | 🟠 | ⬜ | | No launch URL (Expo Router lands on `index`) |
| Image | expo-image | `ExpoImage` | ✅ | ⬜ | | GIF/WebP do not draw (as core Image) |
| Symbols | expo-symbols | `SymbolModule` | ✅ | — | | SF Symbols; Phosphor in fallback mode |
| Glass effect | expo-glass-effect | `ExpoGlassEffect` | ✅ | — | | Liquid Glass |
| Clipboard | expo-clipboard | `ExpoClipboard` | 🟡 | ⬜ | | `modules.simulate clipboard` |
| Haptics | expo-haptics | `ExpoHaptics` | 🟡 | ⬜ | | Emits `module.called` |
| Location | expo-location | `ExpoLocation` | 🟡 | ⬜ | | Foreground only (default Apple Park); background, geocoding, geofencing, heading unsupported |
| Web browser | expo-web-browser | `ExpoWebBrowser` | ⛔ | ⬜ | | Blocks `expo-auth-session` |
| Camera | expo-camera | `ExpoCamera` | ⛔ | ⬜ | | |
| File system | expo-file-system | `FileSystem`, `ExponentFileSystem` | 🟠 | ⬜ | [#8](https://github.com/MitchellSPeterson/SimCore/issues/8) | Per-session sandbox (Documents, Caches, empty Bundle), kept across reloads; downloads, uploads, pickers, preview, watcher unsupported |
| Secure store | expo-secure-store | `ExpoSecureStore` | 🟡 | ⬜ | [#9](https://github.com/MitchellSPeterson/SimCore/issues/9) | Per-session store, kept across reloads; `modules.simulate securestore` seeds keys (`null` deletes); no biometrics |
| SQLite | expo-sqlite | | ⬜ | ⬜ | | |
| Notifications | expo-notifications | | ⬜ | ⬜ | | |
| Image picker | expo-image-picker | | ⬜ | ⬜ | | |
| Document picker | expo-document-picker | | ⬜ | ⬜ | | |
| Media library | expo-media-library | | ⬜ | ⬜ | | |
| Audio | expo-audio | | ⬜ | ⬜ | | |
| Video | expo-video | | ⬜ | ⬜ | | |
| Sensors | expo-sensors | | ⬜ | ⬜ | | |
| Blur | expo-blur | | ⬜ | ⬜ | | |
| Linear gradient | expo-linear-gradient | | ⬜ | ⬜ | | |
| Localization | expo-localization | | ⬜ | ⬜ | | |
| Crypto | expo-crypto | | ⬜ | ⬜ | | |
| Application | expo-application | | ⬜ | ⬜ | | |
| Network | expo-network | | ⬜ | ⬜ | | |
| Battery | expo-battery | | ⬜ | ⬜ | | |
| Brightness | expo-brightness | | ⬜ | ⬜ | | |
| Screen orientation | expo-screen-orientation | | ⬜ | ⬜ | | |
| Sharing | expo-sharing | | ⬜ | ⬜ | | |
| Auth session | expo-auth-session | | ⬜ | ⬜ | | Needs expo-web-browser |
| Local authentication | expo-local-authentication | | ⬜ | ⬜ | | |
| Contacts | expo-contacts | | ⬜ | ⬜ | | |
| Calendar | expo-calendar | | ⬜ | ⬜ | | |
| Store review | expo-store-review | | ⬜ | ⬜ | | |
| Tracking transparency | expo-tracking-transparency | | ⬜ | — | | iOS only |
| Mail composer | expo-mail-composer | | ⬜ | ⬜ | | |
| SMS | expo-sms | | ⬜ | ⬜ | | |
| Speech | expo-speech | | ⬜ | ⬜ | | |
| Print | expo-print | | ⬜ | ⬜ | | |
| Maps | expo-maps | | ⬜ | ⬜ | | |
| Task manager / background task | expo-task-manager | | ⬜ | ⬜ | | |
| Updates | expo-updates | | ⬜ | ⬜ | | |
