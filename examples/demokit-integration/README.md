# DemoKit-style integration

Drives an Expo app through SimCore's public API (`@simcore/sdk`) only: create a session, log events,
tap by label, read the UI tree, save screenshots (`before.png`, `after.png`, `dark.png` in `OUT_DIR`, default cwd).

```sh
(cd examples/expo-counter && CI=1 npx expo start --port 8081)          # 1. Metro
simcore serve --listen 127.0.0.1:3030                                   # 2. SimCore API (cargo run -p simcore-cli -- serve ...)
node examples/demokit-integration/demo.ts                               # 3. the demo (env: SIMCORE_URL, METRO_URL, OUT_DIR)
```
