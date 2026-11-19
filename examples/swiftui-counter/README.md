# swiftui-counter

The PRD counter. `CounterView.swift` is plain SwiftUI source; `import SwiftUI` resolves to OneSim's module.

```sh
scripts/build-swift-app.sh examples/swiftui-counter/CounterView.swift out/counter CounterView
./out/counter   # speaks guest wire (docs/protocol.md B) on stdin/stdout
```

Try it by hand:

```sh
(sleep 1; echo '{"type":"action","id":1,"version":1}'; sleep 1; echo '{"type":"shutdown"}') | ./out/counter
```

Via the host: `create_session` with `app_path` = this file and `root_view` = `CounterView`.
The third argument is needed because the file has no `@main`; see `../swiftui-app-entry` for the `@main App` form.
