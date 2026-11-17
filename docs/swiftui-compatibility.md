# SwiftUI compatibility (guest side)

SimCore ships its own Swift module named `SwiftUI` (`frameworks/swiftui/Sources/SwiftUI`). User code is unmodified.

## How `import SwiftUI` is redirected

`scripts/build-swift-app.sh` compiles our module to `target/swiftui-cache/SwiftUI.{swiftmodule,o}` and builds the app with:

```
swiftc -swift-version 5 -O [-parse-as-library] -module-name SimCoreApp \
  -I target/swiftui-cache -I frameworks/swiftui/Sources/SimCoreBridge \
  <app sources> [generated main.swift] target/swiftui-cache/SwiftUI.o target/release/libsimcore_runtime.a -o <exe>
```

`-I` directories are searched before the SDK's frameworks, so our module wins; Apple's SwiftUI.framework is never loaded or linked
(`otool -L` shows only Foundation/CoreFoundation/libSystem/Swift runtime; `wire_test.sh` asserts this). `SimCoreBridge` is a Clang
module exposing `crates/simcore-runtime/include/simcore_runtime.h`. The SwiftUI module is cached; it is rebuilt when any file under
`frameworks/swiftui/Sources` or the runtime header is newer than `SwiftUI.o`.

## Entry mechanisms

- `@main struct X: App { var body: some Scene { WindowGroup { RootView() } } }` - `App.main()` launches the first `WindowGroup`.
- No `@main`: pass `root_view` (3rd script arg); a `main.swift` calling `_simcoreRun(RootView())` is generated.
- A user-supplied `main.swift` is used as is.

## Evaluation model

Views are walked into ViewNode JSON (docs/protocol.md B). Ids are structural paths (`0`, `0/1`, `0/1/0`; background `.../bg`).
`@State` lives in a registry keyed by `path#ViewType.label`; before a composite's `body` runs, its `@State` properties (found via
`Mirror`) are bound to their slot. Slots not touched in an evaluation are dropped. Button closures get incrementing u64 ids in a table
rebuilt every evaluation. After an action, if state changed, the whole root is re-evaluated and a tree with `version+1` is sent.
An `environment` message stores appearance/viewport and re-sends a tree.

## Supported API

| Area | Supported |
|---|---|
| Protocols | `View`, `App`, `Scene`, `Never: View`, `@ViewBuilder` (up to 10 children, `if`/`if-else`, optionals), `@SceneBuilder` (single scene) |
| Views | `Text(_:)`, `Text(verbatim:)`, `VStack`, `HStack`, `ZStack`, `Spacer(minLength:)`, `Button(_:action:)`, `Button(action:label:)`, `Color`, `Rectangle`, `EmptyView`, `TupleView` |
| Scenes | `WindowGroup { }` (first one launches) |
| Colors | `Color(red:green:blue:opacity:)`, `Color(white:opacity:)`, `.red .green .blue .orange .yellow .purple .pink .gray .black .white .clear .primary .secondary .accentColor` |
| Layout types | `HorizontalAlignment`, `VerticalAlignment`, `Alignment`, `Edge`, `Edge.Set`, `CGFloat` (Foundation) |
| Modifiers | `padding()`, `padding(_:)`, `padding(_ edges:, _ length:)`, `frame(width:height:alignment:)`, `frame(minWidth:maxWidth:minHeight:maxHeight:alignment:)` (`.infinity` -> `"inf"`), `foregroundColor`, `background(_ view)`, `cornerRadius`, `opacity` |
| State | `@State` (+ `$x` -> `Binding`), `Binding(get:set:)`, `.constant`, `Binding` dynamic-member (`$item.done`) and collection subscript (`$store.items[i]`) |
| Objects | `ObservableObject` (`objectWillChange` is a stub), `@Published` (any set/mutation marks dirty), `@StateObject` (slot in the state registry, autoclosure runs once), `@ObservedObject` (+ `$obj.prop` -> `Binding`) |
| Environment | `@Environment(\.colorScheme)`, `EnvironmentValues.colorScheme`, `ColorScheme`; follows `SIMCORE_APPEARANCE` and `environment` messages |
| Navigation | `NavigationStack { }`, `NavigationLink(_:destination:)`, `NavigationLink(destination:label:)`, `navigationTitle`, `navigationBarTitleDisplayMode` |
| Containers | `List`, `Section(_:content:)`, `Section(content:)`, `ForEach` (Range<Int>, `id:` key path, Identifiable), `ScrollView(_ axes:)`, `Divider` |
| Controls | `TextField(_:text:)`, `Toggle(_:isOn:)`, `Stepper(_:value:in:step:)` (Strideable), `.buttonStyle(.automatic/.plain/.bordered/.borderedProminent)`, `.disabled(_:)` |
| Shapes/styles | `Shape`, `Rectangle`, `Circle`, `Capsule`, `RoundedRectangle(cornerRadius:)`, `.fill(_:)`, `.stroke(_:lineWidth:)`, `ShapeStyle` (`Color`, `.primary/.secondary/.tertiary`), `foregroundStyle`, `Color.opacity(_:)` |
| More modifiers | `font` (`Font` text styles, `.system(size:weight:)`), `fontWeight`, `bold`, `strikethrough`, `multilineTextAlignment`, `overlay` (both forms), `shadow` |

| SF Symbols | `Image(systemName:)`, `.resizable()`, `.scaledToFit()`, `.scaledToFill()`, `.aspectRatio(contentMode:)`, `.imageScale(_:)`, `Label(_:systemImage:)`, `Label(title:icon:)`, `Font.weight(_:)` / `Font.bold()` on any `Font` |

### Navigation, lists, input

`NavigationStack` keeps its path in the state registry. Only the top screen is evaluated and emitted as `navigation` (title and
display mode lifted from the screen's `navigationTitle` / `navigationBarTitleDisplayMode`, which is stripped from the modifiers;
`back` pops, labelled with the previous screen's title or "Back"). The root is at `<stack>/0`, pushed screens at `<stack>/s<depth>`,
so `@State` persists while a screen is on the stack (hidden screens' slots are kept alive) and is dropped when popped.
`List`/`Section`/`ForEach`/`if` flatten into `list` sections and rows; `ForEach` puts the element id in the path.
`disabled` and `buttonStyle` are evaluation-context values (not wire modifiers): disabled buttons/toggles/steppers emit
`disabled: true` and action `0` (never registered). `TextField` allocates a `field` id per evaluation; a `text_input` message
(version-gated) sets the bound String via `simcore_text_cb`.

## Known limitations

- Images: only SF Symbols (`Image(systemName:)`), rendered by the host in monochrome (foreground tint); no `Image("asset")`,
  `renderingMode`, `symbolRenderingMode`, `symbolVariant`, `Image(uiImage:)`. `aspectRatio` applies only to images (other views ignore it);
  `scaledToFill` crops to the proposed size. A `Label` inside a list row forces its icon to `imageScale(.large)`.
- Not implemented: `Group`, `List(data)`, `NavigationStack(path:)`, `navigationDestination`, `onAppear`, animations, custom `Shape.path(in:)`,
  `Text` concatenation, `.environment(_:_:)`; `Color.opacity` on semantic colors is ignored.
- `@State` only inside `View`s (not `App`); the `WindowGroup` content closure runs once.
- Structural identity is positional (`if`/`else` branches share a path; state is distinguished by view type).
- Whole-root re-evaluation on every state change.
- `Text` takes `String` (no `LocalizedStringKey`); `foregroundColor(nil)` maps to primary.
- `-swift-version 5` language mode is used for the module and apps. macOS/Darwin only for now (autorelease pool, Foundation).
- `-parse-as-library` is passed only when sources contain `@main`; `@main` must start its line.
