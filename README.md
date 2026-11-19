<h1 align="center">SimCore</h1>

<p align="center">
  <strong>A preview runtime for mobile UI</strong><br>
  Run SwiftUI, Expo / React Native, and Jetpack Compose apps from source.<br>
  Capture PNGs on a CPU or preview live with GPU acceleration.
</p>

<p align="center">
  <a href="https://github.com/MitchellSPeterson/SimCore/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/MitchellSPeterson/SimCore/actions/workflows/ci.yml/badge.svg"></a>
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-blue"></a>
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="docs/README.md">Documentation</a> ·
  <a href="docs/compatibility/README.md">Compatibility</a> ·
  <a href="docs/roadmap.md">Roadmap</a> ·
  <a href="CONTRIBUTING.md">Contributing</a>
</p>

<p align="center">
  <a href="docs/ios-comparison.md#comparison-gallery"><picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/ios-comparison/cmp-glass-dark.png">
    <img src="docs/ios-comparison/cmp-glass.png" width="680" alt="Liquid Glass rendered by the iOS Simulator and SimCore, with a pixel diff on the right">
  </picture></a><br>
  <sub>Liquid Glass in the iOS 26.5 Simulator and SimCore, followed by the pixel diff.</sub>
</p>

## Quick start

On macOS with Rust stable and the Swift toolchain from Xcode or Command Line Tools, clone the repository and run the counter.

```sh
git clone https://github.com/MitchellSPeterson/SimCore.git
cd SimCore
cargo run -p simcore-cli -- run \
  examples/swiftui-counter \
  --root-view CounterView \
  --click Increment \
  --screenshot counter.png --inspect
```

SimCore compiles the SwiftUI example, taps **Increment**, and saves `counter.png`. The printed UI tree contains `Count: 1`. This runs without an iOS Simulator, Chromium, or a GPU.

For Expo, Compose, browser builds, and CLI input options, see the [getting-started guide](docs/getting-started.md).

## Native window

```sh
cargo run -p simcore-cli --features window -- window examples/swiftui-showcase
cargo run -p simcore-cli --features window -- expo --url http://localhost:8081 --window
```

Use `--renderer cpu` to select the CPU presenter or Cmd+R / F5 to switch presenters in the same session. For headless GPU captures, build with `--features gpu` and use `--renderer gpu`. [Renderer migration and compatibility](docs/rendering.md) · [v0.3 benchmarks](docs/benchmarks/v0.3-gpu.md)

## Framework support

| Framework | Target | What you can run |
| --- | --- | --- |
| [SwiftUI](docs/compatibility/swiftui/README.md) | iOS 27.2 and 26.5, WebAssembly | Navigation, tabs, forms, sheets, Dynamic Type, SF Symbols, and Liquid Glass. |
| [Expo / React Native](docs/compatibility/expo/README.md) | iOS and Android preview | Metro bundles with Fabric and simulated device modules. Android core components are in early preview. |
| [Jetpack Compose](docs/compatibility/compose/README.md) | Android 17 preview | The AndroidX Compose runtime with Material 3 controls and layouts on a Pixel 10a profile. |

SimCore runs each framework’s supported UI code in a guest process and renders it on a shared Rust host. SwiftUI can also run in the browser, where the app and CPU renderer compile to one WebAssembly module and a second module enables WebGPU with automatic CPU fallback. [Architecture](docs/architecture.md) · [Browser guide](docs/web.md)

## Visual fidelity

SimCore tests its renderer against screenshots from the iOS Simulator and Android emulator. The comparison scripts check each state on the two supported iOS versions and on Android 17. See the [iOS results and known gaps](docs/ios-comparison.md) and the [Android reference fixtures](tests/fixtures/compose-android/README.md).

## Documentation

- [Run an app and capture a screenshot](docs/getting-started.md)
- [Build a SwiftUI app for the browser](docs/web.md)
- [Choose a renderer and check backend compatibility](docs/rendering.md)
- [Automate sessions and UI input](docs/api.md)
- [See how the renderer and guests fit together](docs/architecture.md)
- [Find every guide and reference](docs/README.md)

## Contributing

Start with the [contributor guide](CONTRIBUTING.md). For a missing API, check the [coverage tables](docs/compatibility/README.md) and open an [API request](https://github.com/MitchellSPeterson/SimCore/issues/new?template=api-request.yml).

## License

SimCore is [MIT-licensed](LICENSE). The bundled fonts and icons retain their [own licenses](docs/architecture.md#fonts-and-symbols).
