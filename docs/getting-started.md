[OneSim](../README.md) / [Documentation](README.md)

# Getting started

Run one of the example apps, interact with it by label, and save the resulting screen to a PNG.

## Run the SwiftUI counter

Install Rust stable and a Swift toolchain: Xcode or Command Line Tools on macOS, or the [swift.org toolchain](https://www.swift.org/install/) on Linux and Windows. On Windows, also install [Git for Windows](https://gitforwindows.org); OneSim runs its build scripts with its Bash (set `SIMCORE_BASH` to use another). From a checkout of OneSim, run the counter.

Apple's SF Pro and SF Symbols ship only with macOS. On Linux and Windows, OneSim renders with the bundled Inter and Phosphor fallbacks, the same as `SIMCORE_FALLBACK=1` on macOS.

```sh
cargo run -p onesim-cli -- run examples/swiftui-counter --root-view CounterView \
  --click Increment --screenshot counter.png --inspect
```

Open `counter.png` to see the updated counter. The UI tree printed by `--inspect` contains `Count: 1`.

Use `--root-view CounterView` when the source declares a view without an app entry point. For a source tree with an `@main App` entry, run it directly.

```sh
cargo run -p onesim-cli -- run examples/swiftui-app-entry --dark --screenshot app.png
```

## Script UI input

OneSim runs `--click`, `--type`, `--key`, and `--scroll` in the order you pass them. This example opens the SwiftUI showcase’s Todos screen, adds an item, and saves the resulting screen.

```sh
cargo run -p onesim-cli -- run examples/swiftui-showcase \
  --click Todos --click "New todo" --type "Buy milk" --click Add --screenshot todos.png
```

Use `--ios 26.5 --device iphone-17-pro` to match the older iOS reference device. The default is iOS 27.2 on an iPhone 18 Pro. See the [version matrix](compatibility/README.md#versions).

To see all CLI options, run `cargo run -p onesim-cli -- run --help`.

## Run an Expo app

Expo requires Node.js and npm. Start Metro from the Expo counter directory.

```sh
cd examples/expo-counter
npm ci
CI=1 npx expo start --port 8081
```

From the repository root in a second terminal, load the bundle from Metro.

```sh
cargo run -p onesim-cli -- expo --url http://localhost:8081 --screenshot expo-counter.png
```

To run the Android bundle on the Pixel 10a preview, use the same Metro server.

```sh
cargo run -p onesim-cli -- expo --platform android --url http://localhost:8081 \
  --click Increment --screenshot expo-counter-android.png
```

See [Expo / React Native compatibility](compatibility/expo/README.md) for supported components and device modules.

## Run a Compose app on Android

Install Java 17 or later. The first run downloads the pinned Kotlin compiler and Compose runtime jars.

```sh
cargo run -p onesim-cli -- run examples/compose-counter \
  --click Increment --screenshot compose-counter.png
```

The PNG uses the Pixel 10a profile. See the [Compose guest guide](../frameworks/compose/README.md) for build and runtime details.

## Preview an app in a native window

Build with the `window` feature and open the SwiftUI showcase.

```sh
cargo run -p onesim-cli --features window -- window examples/swiftui-showcase
```

For an Expo app, start Metro as above and pass `--window` to `expo`.

```sh
cargo run -p onesim-cli --features window -- expo --url http://localhost:8081 --window
```

The window tries the GPU first. Its title shows the active backend. Use `--renderer cpu` to start on the CPU, or press Cmd+R or F5 to switch during the session.

## Capture a screenshot with the GPU

Headless captures default to CPU rendering. Build with the `gpu` feature and pass `--renderer gpu` to require a usable adapter.

```sh
cargo run -p onesim-cli --features gpu -- run examples/swiftui-showcase \
  --renderer gpu --screenshot showcase.png
```

Use `--renderer auto` to allow CPU fallback. See the backend selection guide for diagnostics and recovery. [Rendering backends](rendering.md)

## Preview SwiftUI in a browser

Build and serve the complete browser bundle. OneSim tries WebGPU first and falls back to Canvas 2D when needed. Open the preview with `?diag` to see the active backend. [Browser guide](web.md)

## Drive OneSim from another process

Use the [JSON-RPC API](api.md) for WebSocket clients or the [v1 stdio API](protocol.md#a-external-api-client--host) for newline-delimited JSON. This stdio session starts the SwiftUI counter on the iPhone 18 Pro profile, taps **Increment** at its center in points, and saves `out.png`.

```sh
cargo run -p onesim-cli -- serve <<'EOF2'
{"id":1,"method":"hello","params":{"protocol_version":1}}
{"id":2,"method":"create_session","params":{"framework":"swiftui","app_path":"examples/swiftui-counter","root_view":"CounterView","device":"iphone-18-pro","os_version":"27.2"}}
{"id":3,"method":"start_session","params":{"session_id":"s1"}}
{"id":4,"method":"send_pointer_event","params":{"session_id":"s1","event":{"kind":"click","x":201,"y":469}}}
{"id":5,"method":"capture_screenshot","params":{"session_id":"s1","path":"out.png"}}
{"id":6,"method":"shutdown"}
EOF2
```

<details>
<summary>Run the Rust renderer on its own</summary>

The renderer example writes a counter PNG without a Swift guest. To open a native window instead, build it with the `window` feature.

```sh
cargo run -p onesim-render --example counter
cargo run -p onesim-render --example counter --features window -- --window
```

</details>
