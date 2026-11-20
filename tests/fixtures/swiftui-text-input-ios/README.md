# SwiftUI text input reference

Build the shared example on the simulator.

```sh
scripts/build-ios-sim.sh examples/swiftui-text-input /tmp/OneSimTextInput.app dev.onesim.text-input
```

Install the app and open it through the T3 Device panel's agent-device command.
Use light appearance and `screenshot --pixel-density 3 --normalize-status-bar`.
Retake each state until the Dynamic Island is visible.

- `text-input.png` shows the initial view.
- `text-input-focused.png` shows the keyboard after tapping Full name.

Use iPhone 17 Pro on iOS 26.5 here and iPhone 18 Pro on iOS 27.2 in
`../ios27/swiftui-text-input-ios/`. The screen shows a rounded field with a custom
prompt, a plain secure field with masked text, and a multiline editor.
