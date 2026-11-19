[OneSim](../README.md) / Documentation

# OneSim documentation

OneSim runs SwiftUI, Expo / React Native, and Jetpack Compose UI code. Open a native preview, run SwiftUI in a browser, or drive sessions from the CLI or API.

| I want to… | Start here |
| --- | --- |
| Run an example, send input, and save a PNG | [Getting started](getting-started.md) |
| Open a native preview window | [Native window quick start](getting-started.md#preview-an-app-in-a-native-window) |
| Preview SwiftUI in a browser tab | [Browser guide](web.md) |
| Choose a renderer or diagnose a fallback | [Rendering backends](rendering.md) |
| Check whether OneSim supports an API | [Compatibility and supported versions](compatibility/README.md) |
| Compare OneSim’s output with iOS or Android | [iOS comparison results](ios-comparison.md) · [Android reference fixtures](../tests/fixtures/compose-android/README.md) |
| Drive sessions from a test runner or another tool | [JSON-RPC API](api.md) · [TypeScript SDK](../sdk/typescript/README.md) · [DemoKit integration example](../examples/demokit-integration/README.md) |

## Frameworks

| Framework | Implementation | API coverage |
| --- | --- | --- |
| SwiftUI | [SwiftUI guest](compatibility/swiftui/guest.md) | [Views, modifiers, and other SwiftUI APIs](compatibility/swiftui/README.md) |
| Expo / React Native | [React Native guest](../frameworks/react-native/README.md) | [Core components, native modules, and Expo APIs](compatibility/expo/README.md) |
| Jetpack Compose | [Compose guest](../frameworks/compose/README.md) | [Composables, modifiers, and other Compose APIs](compatibility/compose/README.md) |

## Internals and development

- [Architecture](architecture.md) describes the Rust host, framework guests, and shared renderer.
- [Protocol reference](protocol.md) defines the stdio API, guest messages, and C ABI.
- [Contributing](../CONTRIBUTING.md) covers local checks, compatibility updates, and reference screenshots.
- [Roadmap](roadmap.md) records milestones and planned framework support.
