# Expo showcase on Android

Reference target for [issue #12](https://github.com/MitchellSPeterson/SimCore/issues/12). Capture is pending on the Android 17 (API 37) Pixel 10a emulator.

Start Metro for `examples/expo-showcase` with `npm ci` and `CI=1 npx expo start --port 8081`. Install Expo Go 58.0.5 for Android on the emulator and finish its onboarding. Use `adb reverse tcp:8081 tcp:8081` to expose Metro to the emulator at `exp://127.0.0.1:8081`.

Open the emulator with the T3 device tools and pass the returned command as `AGENT_DEVICE` when running `tests/fixtures/expo-android/capture.sh`. The script captures the home screen in light and dark, the controls before and after interaction, a scrolled list, images, and activity indicators.

Run `scripts/compare-expo-android.sh` to compare each state at the strict 2% threshold. It writes comparison images to `target/compare/expo-android`. Set `EXPO_ANDROID_METRO_URL` to use an already running instance of the showcase.
