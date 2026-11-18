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
| Views | `Text(_:)`, `Text(verbatim:)`, `VStack`, `HStack`, `ZStack`, `Spacer(minLength:)`, `Button(_:role:action:)`, `Button(role:action:label:)` (`ButtonRole` `.cancel`/`.destructive`), `Text + Text` (with `Text.bold()`, `.font`, `.fontWeight`, `.foregroundColor`, `.foregroundStyle` per run), `Color`, `Rectangle`, `EmptyView`, `TupleView` |
| Scenes | `WindowGroup { }` (first one launches) |
| Colors | `Color(red:green:blue:opacity:)`, `Color(white:opacity:)`, `.red .green .blue .orange .yellow .purple .pink .gray .black .white .clear .primary .secondary .accentColor` |
| Layout types | `HorizontalAlignment`, `VerticalAlignment`, `Alignment`, `Edge`, `Edge.Set`, `CGFloat` (Foundation) |
| Modifiers | `padding()`, `padding(_:)`, `padding(_ edges:, _ length:)`, `frame(width:height:alignment:)`, `frame(minWidth:maxWidth:minHeight:maxHeight:alignment:)` (`.infinity` -> `"inf"`), `foregroundColor`, `background(_ view)`, `cornerRadius`, `opacity` |
| State | `@State` (+ `$x` -> `Binding`), `Binding(get:set:)`, `.constant`, `Binding` dynamic-member (`$item.done`) and collection subscript (`$store.items[i]`) |
| Objects | `ObservableObject` (`objectWillChange` is a stub), `@Published` (any set/mutation marks dirty), `@StateObject` (slot in the state registry, autoclosure runs once), `@ObservedObject` (+ `$obj.prop` -> `Binding`) |
| Environment | `@Environment(\.colorScheme)`, `EnvironmentValues.colorScheme`, `ColorScheme`; follows `SIMCORE_APPEARANCE` and `environment` messages. `@Environment(\.dismiss)` (`DismissAction`, pops the enclosing `NavigationStack`), `@Environment(\.dynamicTypeSize)` (`DynamicTypeSize`, from `SIMCORE_DYNAMIC_TYPE` at launch, default `.large`). `@EnvironmentObject` + `.environmentObject(_:)` |
| Navigation | `NavigationStack { }`, `NavigationLink(_:destination:)`, `NavigationLink(destination:label:)`, `navigationTitle`, `navigationBarTitleDisplayMode`, `NavigationLink(_:value:)`, `NavigationLink(value:label:)`, `.navigationDestination(for:destination:)`, `NavigationStack(path:)` (`Binding<NavigationPath>` or `Binding<[T]>`), `NavigationPath` (`append`, `removeLast`, `count`, `isEmpty`) |
| Containers | `Group`, `List`, `List(data, rowContent:)` / `List(data, id:, rowContent:)` (Identifiable, key path, `Range<Int>`), `Section(_:content:)`, `Section(content:)`, `ForEach` (Range<Int>, `id:` key path, Identifiable), `ScrollView(_ axes:)`, `Divider` |
| Controls | `TextField(_:text:)`, `Toggle(_:isOn:)`, `Stepper(_:value:in:step:)` (Strideable), `.buttonStyle(.automatic/.plain/.bordered/.borderedProminent)`, `.disabled(_:)` |
| Shapes/styles | `Shape`, `Rectangle`, `Circle`, `Capsule`, `RoundedRectangle(cornerRadius:)`, `.fill(_:)`, `.stroke(_:lineWidth:)`, `ShapeStyle` (`Color`, `.primary/.secondary/.tertiary`), `foregroundStyle`, `Color.opacity(_:)` |
| More modifiers | `font` (`Font` text styles, `.system(size:weight:)`), `fontWeight`, `bold`, `strikethrough`, `multilineTextAlignment`, `overlay` (both forms), `shadow` |
| Lifecycle | `.onAppear { }`, `.onDisappear { }` |
| Liquid Glass (iOS 26) | `Glass` (`.regular .clear .identity`, `.tint(_:)`, `.interactive(_:)`), `.glassEffect()`, `.glassEffect(_:in:)` with `Shape` static members `.circle .capsule .ellipse .rect .rect(cornerRadius:)`, `Ellipse`, `GlassEffectContainer(spacing:content:)` (pass-through), `.buttonStyle(.glass)`, `.buttonStyle(.glassProminent)`. Emitted per docs/protocol.md F. |
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

### Presentation

- `.alert(_:isPresented:actions:message:)`, `.alert(_:isPresented:actions:)` (no actions: an OK button), `.alert(_:isPresented:presenting:actions:message:)` / `presenting:actions:`.
- `.confirmationDialog(_:isPresented:titleVisibility:actions:message:)`, without `message`, and the `presenting:` forms (rendered as the iPhone popover; a Cancel is added when the app has none, and tapping outside cancels).
- `.sheet(isPresented:onDismiss:content:)`, `.sheet(item:onDismiss:content:)`, `.fullScreenCover(isPresented:onDismiss:content:)`, `.fullScreenCover(item:onDismiss:content:)`.
- `.presentationDetents(_:)` (`.medium`, `.large`, `.fraction(_:)`, `.height(_:)`; starts at the smallest), `.presentationDragIndicator(_:)`, `.interactiveDismissDisabled(_:)`; `Visibility`.
- Sheets and covers can present further sheets/alerts (a stack). `@Environment(\.dismiss)` inside a sheet or cover clears its binding (a pushed screen inside it is popped first). `onDismiss` runs whenever the presentation goes away: `dismiss()`, a swipe, or app code clearing the binding.
- Swipe: a sheet follows a downward drag (dismisses past 25% of the way down, settles at the nearest detent otherwise) and an upward drag expands it; content scrolls first at the largest detent. See protocol section G for the gesture rules.

Presented content is evaluated in place (a sheet's `@State` lives at `<view path>/sheet` only while shown) and hoisted by the runtime above the whole tree as a `presentation` node (protocol section G). Any `Button` inside alert/dialog actions dismisses it after its own action. `Button(_:role:action:)` takes `ButtonRole` `.cancel` / `.destructive`.

Limits: no animations (settled states only; the sheet snaps to its detent on release), no `presentationBackground`, `presentationCornerRadius`, `presentationBackgroundInteraction`, `presentationCompactAdaptation`, `dialogSeverity`, `alert` with text fields or `Text` titles, `popover`, or `inspector`. Presented content is hoisted above the whole screen, not just its presenting view. The popover points at the first text of the view the dialog is attached to. `Text` runs: no underline color/pattern, `baselineOffset` or `kerning`; a concatenation's line height follows its largest run.

## Other Apple frameworks

SwiftUI apps reach for more than `SwiftUI`. Each of these will be reimplemented in Swift the same way, so `import <Framework>` compiles unchanged ([roadmap](roadmap.md#apple-frameworks-beyond-swiftui)).

| Framework | Status |
|---|---|
| Charts | planned |
| SpriteKit | planned |
| SceneKit | planned |
| RealityKit | planned |
| ARKit | planned |
| MapKit | planned |
| StoreKit | planned |
| MusicKit | planned |
| PencilKit | planned |
| WidgetKit | planned |
| UIKit | planned |
| CoreGraphics | planned (`CGFloat` only, via Foundation) |
| Metal | planned |
| Vision | planned |

## Known limitations

- Images: only SF Symbols (`Image(systemName:)`), rendered by the host in monochrome (foreground tint); no `Image("asset")`,
  `renderingMode`, `symbolRenderingMode`, `symbolVariant`, `Image(uiImage:)`. `aspectRatio` applies only to images (other views ignore it);
  `scaledToFill` crops to the proposed size. A `Label` inside a list row forces its icon to `imageScale(.large)`.
- Liquid Glass: `GlassEffectContainer` does not merge/morph shapes, `glassEffectID`/`glassEffectUnion`/`glassEffectTransition` are missing, `interactive` has no visual effect, `Ellipse` as a plain shape view draws as a capsule, and a non-square `.circle` is a capsule.
- Dynamic Type is a host setting (`--dynamic-type`) that scales text styles only; `@Environment(\.dynamicTypeSize)` is read once from `SIMCORE_DYNAMIC_TYPE` at launch. A session's size is fixed at `create_session` (no API changes it later), so that value stays correct; the web demo's `setDynamicType` re-renders on the host only and the guest is not told.
- Value navigation: destinations registered by `navigationDestination` are remembered per stack (only the top screen is evaluated, so a stack that starts with a non-empty `path` has no destinations until its root has been evaluated once). A value with no registered destination pushes nothing and logs SwiftUI's "no matching navigationDestination" warning to stderr (there is no guest-to-host error channel). Destination-style `NavigationLink`s in a `NavigationStack(path:)` push on top of the path-driven screens and back pops them first; a value pushed from such a screen goes on that screen stack rather than into the bound path.
- A modifier on a multi-child `Group` applies to each child, as in SwiftUI (other multi-root views are still wrapped in a vstack).
- `onAppear`/`onDisappear` fire per structural path after the evaluation pass (a screen covered by a push disappears, and appears again on pop); if they dirty state the root is re-evaluated, at most 8 times, before the tree is sent. `@EnvironmentObject`/`@Environment` are snapshotted when a view's `body` runs.
- Not implemented: animations, custom `Shape.path(in:)`,
  `.environment(_:_:)`; `Color.opacity` on semantic colors is ignored.
- `@State` only inside `View`s (not `App`); the `WindowGroup` content closure runs once.
- Structural identity is positional (`if`/`else` branches share a path; state is distinguished by view type).
- Whole-root re-evaluation on every state change.
- `Text` takes `String` (no `LocalizedStringKey`); `foregroundColor(nil)` maps to primary.
- `-swift-version 5` language mode is used for the module and apps. macOS/Darwin only for now (autorelease pool, Foundation).
- `-parse-as-library` is passed only when sources contain `@main`; `@main` must start its line.
