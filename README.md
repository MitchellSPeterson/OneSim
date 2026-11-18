# SimCore

Rust-powered preview runtime for mobile UI code. Runs real SwiftUI source (unchanged `import SwiftUI`) and unmodified Expo / React Native apps, and renders them headlessly, with no iOS Simulator, no Chromium, and no GPU required.

```
$ simcore run examples/swiftui-counter --root-view CounterView --click Increment --screenshot out.png
```

| Framework | iOS | Android |
|---|---|---|
| SwiftUI | ✅ v0.1: navigation, lists, form controls, shapes, text styles, SF Symbols, software keyboard, `@State`/`@Binding`/`ObservableObject` ([compatibility](docs/swiftui-compatibility.md)) | n/a |
| Expo / React Native | 🧪 v0.2 preview: core components, Fast Refresh, Expo Router native stack and tabs, Reanimated, simulated device modules ([API coverage](docs/expo-api-coverage.md)) | 🔜 coming ([API coverage](docs/expo-api-coverage.md)) |
| Jetpack Compose | n/a | 🔜 planned |

Beyond SwiftUI, we will reimplement the Apple frameworks apps reach for (Charts, SpriteKit, SceneKit, RealityKit, ARKit, MapKit, StoreKit, MusicKit, PencilKit, WidgetKit, UIKit, CoreGraphics, Metal, Vision); see the [roadmap](docs/roadmap.md#apple-frameworks-beyond-swiftui).

Milestones: [docs/roadmap.md](docs/roadmap.md).

Rendered output stays within 0.4–1.9% of the iOS 26 Simulator pixel-for-pixel across 13 states, including SF Symbols and the software keyboard, on the [showcase app](examples/swiftui-showcase/Showcase.swift). See [docs/ios-comparison.md](docs/ios-comparison.md).

![SimCore vs iOS Simulator](docs/ios-comparison/cmp-controls-changed.png)

## How it works

Your app compiles with `swiftc` against SimCore's own `SwiftUI` module, which shadows Apple's. The app runs as its own process and sends a normalized view tree to the host through a small Rust staticlib over a C ABI. The host lays the tree out with SwiftUI semantics, rasterizes it with tiny-skia and cosmic-text, hit-tests input, and routes button actions back to Swift closures. Details: [docs/architecture.md](docs/architecture.md), [docs/protocol.md](docs/protocol.md), [docs/swiftui-compatibility.md](docs/swiftui-compatibility.md).

## In the browser (WebAssembly)

The same unmodified SwiftUI source runs in a browser tab. The app and SimCore's SwiftUI module compile to WebAssembly together with the Rust renderer into one module, which draws to a `<canvas>` and needs only a static file server. Setup: [docs/web.md](docs/web.md).

```sh
scripts/build-web.sh examples/swiftui-showcase && python3 -m http.server -d target/web 8000   # or: ... swiftui-counter CounterView
node web/test/run.mjs --suite showcase   # headless check; scripts/compare-web.sh = pixel parity vs native
```

It is the full app, not a screenshot: pointer, wheel/drag scrolling, text fields with the on-canvas keyboard (and your physical keyboard), `@main App`, `?device=`/`?width=&height=&scale=`/`?dark=1`.

## Expo / React Native (preview)

SimCore loads an unmodified Expo app's bundle from Metro, runs React Native's own Fabric renderer in a Node guest, and implements the iOS native side (views, TurboModules, Expo modules) in the guest and the Rust host. Unsupported native APIs fail loudly (`SimCore: X is not supported`) instead of faking success.

```sh
cd examples/expo-counter && npm install && CI=1 npx expo start --port 8081 &
cargo run -p simcore-cli -- expo --url http://localhost:8081 --screenshot out.png
```

Which APIs work today, per platform, with an overall percentage: [docs/expo-api-coverage.md](docs/expo-api-coverage.md). Details: [frameworks/react-native/README.md](frameworks/react-native/README.md), [docs/protocol.md](docs/protocol.md) §H.

**Android is coming.** The same Expo apps will render as Android (Material) through an Android host; the coverage doc tracks every API for Android from zero so the work can be picked up row by row.

## Requirements

macOS with the Swift toolchain (Xcode or Command Line Tools) and Rust stable. Node.js for Expo / React Native apps.

## Build

```sh
cargo build --workspace
cargo test --workspace
frameworks/swiftui/Tests/wire_test.sh   # Swift side: build, drive 1100 actions, memory check
```

## Use

Headless driver:

```sh
cargo run -p simcore-cli -- run examples/swiftui-counter --root-view CounterView \
  --click Increment --click Increment --screenshot counter.png --inspect
cargo run -p simcore-cli -- run examples/swiftui-app-entry --dark --screenshot app.png   # @main App entry
cargo run -p simcore-cli -- run examples/swiftui-showcase --click Todos --click "New todo" --type "Buy milk" \
  --click Add --screenshot todos.png      # --click/--type/--key/--scroll run in order
```

JSON-lines API for tools such as DemoKit (protocol §A):

```sh
cargo run -p simcore-cli -- serve <<'EOF2'
{"id":1,"method":"hello","params":{"protocol_version":1}}
{"id":2,"method":"create_session","params":{"framework":"swiftui","app_path":"examples/swiftui-counter","root_view":"CounterView"}}
{"id":3,"method":"start_session","params":{"session_id":"s1"}}
{"id":4,"method":"send_pointer_event","params":{"session_id":"s1","event":{"kind":"click","x":196,"y":440}}}
{"id":5,"method":"capture_screenshot","params":{"session_id":"s1","path":"out.png"}}
{"id":6,"method":"shutdown"}
EOF2
```

Renderer only, no Swift (milestone 1):

```sh
cargo run -p simcore-render --example counter                                  # writes a PNG
cargo run -p simcore-render --example counter --features window -- --window    # native window
```

## Entry points

- `--root-view Name`: SimCore generates `main` for a source tree that only declares views.
- `@main struct MyApp: App { var body: some Scene { WindowGroup { ContentView() } } }`: used as-is.

## Layout

| Path | What |
|---|---|
| `crates/simcore-protocol` | Wire and API types (serde) |
| `crates/simcore-render` | Scene, SwiftUI layout, raster, hit testing, inspection |
| `crates/simcore-host` | Sessions, build, process-per-session supervision |
| `crates/simcore-cli` | `simcore` binary (`run`, `serve`) |
| `crates/simcore-runtime` | Guest staticlib, C ABI linked into the app |
| `frameworks/swiftui` | SimCore's `SwiftUI` module and C bridge |
| `frameworks/react-native` | Node guest for Expo / React Native: Fabric UIManager, native-module registry (`guest/modules.js`) |
| `frameworks/compose` | Jetpack Compose engine (not started) |

## Fonts and symbols fallback

On macOS the host uses the system SF Pro and SF Symbols. Elsewhere, or with `SIMCORE_FALLBACK=1`, it uses bundled open-licensed substitutes that look close to iOS: Inter ([SIL OFL 1.1](assets/fonts/OFL.txt)) and Phosphor Icons ([MIT](assets/symbols/phosphor/LICENSE)), mapped from SF Symbol names. `get_capabilities` reports `"fonts"` and `"symbols"`; see [docs/protocol.md](docs/protocol.md) section E. `SIMCORE_FALLBACK=1 scripts/compare-ios.sh` measures it.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Good first work: any ⬜ / 🟠 / ⛔ row in [docs/expo-api-coverage.md](docs/expo-api-coverage.md); open an [API request](https://github.com/MitchellSPeterson/SimCore/issues/new?template=api-request.yml) for it and link it in the row.

## License

MIT (bundled assets keep their own licenses, see above)
