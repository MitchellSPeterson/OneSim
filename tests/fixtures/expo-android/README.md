# Expo showcase on Android

Captured on the Android 17 (API 37) Pixel 10a emulator at 1080 × 2424 pixels and 2.625× density. The screenshots use Expo Go 58.0.5. [Issue #12](https://github.com/MitchellSPeterson/SimCore/issues/12)

Start Metro for `examples/expo-showcase` with `npm ci` and `CI=1 npx expo start --port 8081`. Install the pinned Expo Go version and finish its onboarding. Use `adb reverse tcp:8081 tcp:8081` to expose Metro to the emulator at `exp://127.0.0.1:8081`.

Open the emulator with the T3 device tools. On first launch, open Expo Go’s developer menu and disable **Tools button** to keep the floating control out of the screenshots. Start the capture from the showcase home screen. If Expo Go resumes Visuals, dismiss the showcase card in Android’s app switcher and reopen the project.

Pass the T3 device command as `AGENT_DEVICE` when running `tests/fixtures/expo-android/capture.sh`. The script captures home in light and dark, controls before and after interaction, a scrolled list, images, and the Visuals screen.

Run `scripts/compare-expo-android.sh` to compare each state at the strict 2% threshold. It writes comparison images to `target/compare/expo-android`. Set `EXPO_ANDROID_METRO_URL` to use an already running instance of the showcase.
