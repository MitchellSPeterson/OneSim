# SwiftUI effects references

Build Apple's SwiftUI version of the shared example with
`scripts/build-ios-sim.sh examples/swiftui-effects /tmp/OneSimEffects.app dev.onesim.effects`.
Install and open the app with the T3 Device panel's agent-device command.
Capture `effects.png` and `effects-dark.png` in light and dark appearance with
`screenshot --pixel-density 3 --normalize-status-bar`. Use the iPhone 17 Pro on
iOS 26.5 here and the iPhone 18 Pro on iOS 27.2 in `../ios27/swiftui-effects-ios/`.
Retake the light capture until the Dynamic Island is visible.

The screen includes a gradient fill, gradient stroke border, elliptical gradient,
mesh with a radial overlay, Canvas fill and stroke, rotation and reflection with
blur, an alpha mask, an isolated multiply blend, and a geometry-dependent transform.
