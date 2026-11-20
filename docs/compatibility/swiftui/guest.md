# SwiftUI guest: build, evaluation model and limits

OneSim ships its own Swift module named `SwiftUI` (`frameworks/swiftui/Sources/SwiftUI`). User code is unmodified.

## How `import SwiftUI` is redirected

`scripts/build-swift-app.sh` compiles our module to `target/swiftui-cache/SwiftUI.{swiftmodule,o}` and builds the app with:

```
swiftc -swift-version 5 -O [-parse-as-library] -module-name OneSimApp \
  -I target/swiftui-cache -I frameworks/swiftui/Sources/OneSimBridge \
  <app sources> [generated main.swift] target/swiftui-cache/SwiftUI.o target/release/libonesim_runtime.a -o <exe>
```

`-I` directories are searched before the SDK's frameworks, so our module wins; Apple's SwiftUI.framework is never loaded or linked
(`otool -L` shows only Foundation/CoreFoundation/libSystem/Swift runtime; `wire_test.sh` asserts this). `OneSimBridge` is a Clang
module exposing `crates/onesim-runtime/include/simcore_runtime.h`. The SwiftUI module is cached; it is rebuilt when any file under
`frameworks/swiftui/Sources` or the runtime header is newer than `SwiftUI.o`.

## Entry mechanisms

- `@main struct X: App { var body: some Scene { WindowGroup { RootView() } } }` - `App.main()` launches the first `WindowGroup`.
- No `@main`: pass `root_view` (3rd script arg); a `main.swift` calling `_onesimRun(RootView())` is generated.
- A user-supplied `main.swift` is used as is.

## Evaluation model

Views are walked into ViewNode JSON (docs/protocol.md B). Ids are structural paths (`0`, `0/1`, `0/1/0`; background `.../bg`).
`@State` lives in a registry keyed by `path#ViewType.label`; before a composite's `body` runs, its `@State` properties (found via
`Mirror`) are bound to their slot. Slots not touched in an evaluation are dropped. Button closures get incrementing u64 ids in a table
rebuilt every evaluation. After an action, if state changed, the whole root is re-evaluated and a tree with `version+1` is sent.
An `environment` message stores appearance/viewport and re-sends a tree.

## Supported API

Per-API status, with what is still missing: [README.md](README.md).

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
| Tabs | `TabView { }`, `TabView(selection:)` with `.tag(_:)`, `.tabItem { Label(_:systemImage:) }` / `.tabItem { Image(systemName:); Text(_:) }`, iOS 18 `Tab(_:systemImage:content:)` / `Tab(_:systemImage:value:content:)`, `.badge(_:)` (`Int`, 0 hides it; `StringProtocol`). Only the selected tab is evaluated; each tab's `@State` and `NavigationStack` path survive switching (protocol section J). |
| Forms | `Form { }` (inset-grouped list), `SecureField(_:text:)`, `LabeledContent(_:value:)`, `LabeledContent(_:content:)`, `LabeledContent(content:label:)`, `Picker(_:selection:content:)` / `Picker(selection:content:label:)` with `.tag(_:)`, `.pickerStyle(.automatic/.menu/.segmented)`, `Slider(value:in:)`, `Slider(value:in:step:)` (+ `label:` variants; `BinaryFloatingPoint`), `ProgressView()`, `ProgressView(_:)`, `ProgressView(value:total:)`, `ProgressView(_:value:total:)`, `ProgressView(label:)` |
| Shapes/styles | `Shape` (custom `path(in:)`, `role`, `layoutDirectionBehavior`, `sizeThatFits`), `InsettableShape` (`inset(by:)`, `.strokeBorder`), `RoundedRectangularShape`, `ShapeView` (`FillShapeView`, `StrokeShapeView`, `StrokeBorderShapeView`, chainable `.fill`/`.stroke`/`.strokeBorder`), `Path` (builders, arcs, `description`/`init?(_:)`, `boundingRect`, `contains`, `trimmedPath`, `strokedPath`, boolean ops, `cgPath` on Apple platforms), `Rectangle`, `Circle`, `Ellipse`, `Capsule(style:)`, `RoundedRectangle(cornerRadius:/cornerSize:style:)`, `UnevenRoundedRectangle`, `ContainerRelativeShape`, `ConcentricRectangle` (with `Edge.Corner.Style`), `AnyShape`, `OffsetShape`/`.offset`, `RotatedShape`/`.rotation`, `ScaledShape`/`.scale`, `TransformedShape`/`.transform`, `.trim`, `.size`, `.stroke(style:/lineWidth:)` (shape), `.union`/`.intersection`/`.subtracting`/`.symmetricDifference`/`.lineIntersection`/`.lineSubtraction`, `.containerShape(_:)`, `StrokeStyle`, `FillStyle`, `RoundedCornerStyle`, `RectangleCornerRadii`, `RoundedRectangularShapeCorners`, `ProposedViewSize`, `ShapeRole`, `LayoutDirection`, `LayoutDirectionBehavior`, `ShapeStyle` (`Color`, `ForegroundStyle`, `.primary/.secondary/.tertiary`), `foregroundStyle`, `Color.opacity(_:)`. Protocol section N. |

| More modifiers | `font` (`Font` text styles, `.system(size:weight:design:)`), `fontWeight`, `bold`, `strikethrough`, `multilineTextAlignment`, `overlay` (both forms), `shadow` |

| Text styling | `kerning(_:)`, `tracking(_:)`, `baselineOffset(_:)`, `fontDesign(_:)`, `fontWidth(_:)`, `monospaced(_:)`, `monospacedDigit()`, `textScale(_:)`, `textCase(_:)`, `truncationMode(_:)`, `lineLimit` (count, `reservesSpace`, and ranges), `minimumScaleFactor(_:)`, `allowsTightening(_:)`, `underline(_:pattern:color:)`, `strikethrough(_:pattern:color:)`. A run's kerning and tracking add to the parent's. Protocol section O |

| Images | `renderingMode(_:)`, `symbolRenderingMode(_:)`, `symbolColorRenderingMode`, `symbolVariableValueMode`, `interpolation(_:)`, `antialiased(_:)`, `allowedDynamicRange`, `AsyncImage(url:)` (a file URL loads during the first evaluation; nil stays empty). Protocol section O |

| Lifecycle | `.onAppear { }`, `.onDisappear { }`, `.onChange(of:initial:_:)` (`{ old, new in }` and `{ }`), `.onChange(of:perform:)` |
| Gestures | `TapGesture(count:)`, `SpatialTapGesture(count:coordinateSpace:)`, `LongPressGesture(minimumDuration:maximumDistance:)`, `DragGesture(minimumDistance:coordinateSpace:)`, `MagnifyGesture(minimumScaleDelta:)`, `RotateGesture(minimumAngleDelta:)`, deprecated `MagnificationGesture` / `RotationGesture`, `SimultaneousGesture` / `SequenceGesture` / `ExclusiveGesture` (+ `simultaneously(with:)`, `sequenced(before:)`, `exclusively(before:)`), `AnyGesture`, custom `Gesture` bodies, `onChanged`, `onEnded`, `map`, `updating(_:body:)` (`GestureStateGesture`), `@GestureState` (`wrappedValue:`, `reset:`), `.gesture(_:including:)`, `.gesture(_:isEnabled:)`, `.simultaneousGesture`, `.highPriorityGesture`, `GestureMask`, `.onTapGesture(count:perform:)`, `.onTapGesture(count:coordinateSpace:perform:)`, `.onLongPressGesture(minimumDuration:maximumDistance:perform:onPressingChanged:)` (and `pressing:perform:`), `.offset(_:)` / `.offset(x:y:)`; `Angle`, `UnitPoint`, `CoordinateSpace` (`.local`, `.global`, `.named`), `Transaction`. Protocol section L. |
| Storage | `@AppStorage("key")` for Bool/Int/Double/String and `RawRepresentable` (String/Int raw), `$x` -> `Binding` |
| Lazy containers | `LazyVStack`, `LazyHStack` (`alignment:spacing:pinnedViews:content:`; children are built eagerly; section headers pin, footers do not), `LazyVGrid`, `LazyHGrid` (eager; `pinnedViews` ignored), `PinnedScrollableViews`, `GridItem` (`.fixed`, `.flexible`, `.adaptive`) |
| Grids and fitting | `Grid(alignment:horizontalSpacing:verticalSpacing:)`, `GridRow`, `.gridCellColumns(_:)`. Columns size to their cells (default spacing 8). `ViewThatFits` builds every child and shows the first that fits |
| Geometry | `GeometryReader`, `GeometryProxy` (`size`, `safeAreaInsets`, `frame(in:)`), `EdgeInsets`. One correction pass. `safeAreaInsets` are the device insets from `SIMCORE_INSETS`. `frame(in: .local)` is the origin; `.global` and a named space are the host screen frame |
| Scroll targets | `ScrollViewReader`, `ScrollViewProxy.scrollTo` (unit-point anchor, no animation), `.id` (names a view for `scrollTo`; does not reset `@State`), `.scrollPosition(id:)` (`String` and `Int`; written back when scrolling settles), `.scrollTargetBehavior` (`.paging`, `.viewAligned`), `.scrollTargetLayout`, `.scrollIndicators` (never drawn), `.contentMargins` (length and edges on `ScrollView`), `ScrollIndicatorVisibility`, `PagingScrollTargetBehavior`, `ViewAlignedScrollTargetBehavior`, `AnyScrollTargetBehavior`. Protocol section M |
| Liquid Glass (iOS 26) | `Glass` (`.regular .clear .identity`, `.tint(_:)`, `.interactive(_:)`), `.glassEffect()`, `.glassEffect(_:in:)` with `Shape` static members `.circle .capsule .ellipse .rect .rect(cornerRadius:)`, `Ellipse`, `GlassEffectContainer(spacing:content:)` (pass-through), `.buttonStyle(.glass)`, `.buttonStyle(.glassProminent)`. Emitted per docs/protocol.md F. |
| SF Symbols | `Image(systemName:)`, `.resizable()`, `.scaledToFit()`, `.scaledToFill()`, `.aspectRatio(contentMode:)`, `.imageScale(_:)`, `Label(_:systemImage:)`, `Label(title:icon:)`, `Font.weight(_:)` / `Font.bold()` on any `Font` |
| Animation & transitions | `Animation` (`.default`, `.linear`, `.easeIn`, `.easeOut`, `.easeInOut`, `.timingCurve`, `.spring`, `.interactiveSpring`, `.interpolatingSpring`, `.smooth`, `.snappy`, `.bouncy`, `.delay`, `.speed`, `.repeatCount`, `.repeatForever`, `CustomAnimation`), `Spring`, `UnitCurve`, `Transaction` (`animation`, `disablesAnimations`, `isContinuous`, `tracksVelocity`, `TransactionKey`, `addAnimationCompletion`), `withAnimation` (+ `completion:`), `withTransaction`, `.animation(_:value:)`, `.animation(_:)`, `.transaction`, `Transition`, `AnyTransition` (`.identity`, `.opacity`, `.scale`, `.slide`, `.move`, `.offset`, `.push`, `.blurReplace`, `.asymmetric`, `.combined`, `.animation`), `.transition`, `@Namespace`, `MatchedGeometryProperties`, `.matchedGeometryEffect`, `ContentTransition` (`.identity`, `.interpolate`, `.opacity`, `.numericText`, `.symbolEffect`), `.contentTransition`, `PhaseAnimator` / `.phaseAnimator`, `KeyframeAnimator` / `.keyframeAnimator` (`KeyframeTimeline`, `KeyframeTrack`, `LinearKeyframe`, `CubicKeyframe`, `SpringKeyframe`, `MoveKeyframe`), `TimelineView` (`PeriodicTimelineSchedule`, `AnimationTimelineSchedule`, `EveryMinuteTimelineSchedule`, `ExplicitTimelineSchedule`), `PlaceholderContentView`, `Animatable`, `VectorArithmetic`, `AnimatablePair`, `EmptyAnimatableData`. Protocol section P. |

### Navigation, lists, input

`NavigationStack` keeps its path in the state registry. Only the top screen is evaluated and emitted as `navigation` (title and
display mode lifted from the screen's `navigationTitle` / `navigationBarTitleDisplayMode`, which is stripped from the modifiers;
`back` pops, labelled with the previous screen's title or "Back"). The root is at `<stack>/0`, pushed screens at `<stack>/s<depth>`,
so `@State` persists while a screen is on the stack (hidden screens' slots are kept alive) and is dropped when popped.
`List`/`Section`/`ForEach`/`if` flatten into `list` sections and rows; `ForEach` puts the element id in the path.
`disabled` and `buttonStyle` are evaluation-context values (not wire modifiers): disabled buttons/toggles/steppers emit
`disabled: true` and action `0` (never registered). `TextField` allocates a `field` id per evaluation; a `text_input` message
(version-gated) sets the bound String via `simcore_text_cb`. `SecureField` and `Slider` reuse this: a slider's new value arrives as `text_input`
on its `field` and is clamped and snapped to `step` in the guest. A `Picker` registers one action per tagged option (protocol section I).

### Presentation

- `.alert(_:isPresented:actions:message:)`, `.alert(_:isPresented:actions:)` (no actions: an OK button), `.alert(_:isPresented:presenting:actions:message:)` / `presenting:actions:`.
- `.confirmationDialog(_:isPresented:titleVisibility:actions:message:)`, without `message`, and the `presenting:` forms (rendered as the iPhone popover; a Cancel is added when the app has none, and tapping outside cancels).
- `.sheet(isPresented:onDismiss:content:)`, `.sheet(item:onDismiss:content:)`, `.fullScreenCover(isPresented:onDismiss:content:)`, `.fullScreenCover(item:onDismiss:content:)`.
- `.presentationDetents(_:)` (`.medium`, `.large`, `.fraction(_:)`, `.height(_:)`; starts at the smallest), `.presentationDragIndicator(_:)`, `.interactiveDismissDisabled(_:)`; `Visibility`.
- Sheets and covers can present further sheets/alerts (a stack). `@Environment(\.dismiss)` inside a sheet or cover clears its binding (a pushed screen inside it is popped first). `onDismiss` runs whenever the presentation goes away: `dismiss()`, a swipe, or app code clearing the binding.
- Swipe: a sheet follows a downward drag (dismisses past 25% of the way down, settles at the nearest detent otherwise) and an upward drag expands it; content scrolls first at the largest detent. See protocol section G for the gesture rules.

Presented content is evaluated in place (a sheet's `@State` lives at `<view path>/sheet` only while shown) and hoisted by the runtime above the whole tree as a `presentation` node (protocol section G). Any `Button` inside alert/dialog actions dismisses it after its own action. `Button(_:role:action:)` takes `ButtonRole` `.cancel` / `.destructive`.

Limits: no animations (settled states only; the sheet snaps to its detent on release), no `presentationBackground`, `presentationCornerRadius`, `presentationBackgroundInteraction`, `presentationCompactAdaptation`, `dialogSeverity`, `alert` with text fields or `Text` titles, `popover`, or `inspector`. Presented content is hoisted above the whole screen, not just its presenting view. The popover points at the first text of the view the dialog is attached to. `Text` runs carry kerning, tracking, baseline offset, font design and width, and underline or strikethrough pattern and color. A concatenation's line height follows its largest run. `Text` is still `String` only.

## Other Apple frameworks

SwiftUI apps reach for more than `SwiftUI`. Each of these will be reimplemented in Swift the same way, so `import <Framework>` compiles unchanged ([roadmap](../../roadmap.md#apple-frameworks-beyond-swiftui)).

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
| CoreGraphics | re-exported by `SwiftUI` on macOS (`CGFloat`, `CGPoint`, `CGSize`); stand-ins on the web build |
| Metal | planned |
| Vision | planned |

## Known limitations

- Form controls (protocol section I): picker options are the tagged children's first text (no images in options); only menu and segmented picker styles (`.inline`, `.wheel`, `.navigationLink`, `.palette` are not available). The menu is a static card (no open animation, no scroll for long lists). `Slider` has no `onEditingChanged`, `minimumValueLabel`/`maximumValueLabel` or tint; it moves on a tap or a whole `drag` gesture, not live while dragging. `ProgressView()` draws one static spinner frame; no `progressViewStyle`, `.circular` with a value, or `currentValueLabel`. `LabeledContent` styles its content `.secondary` (also non-text content), no `labeledContentStyle`. `SecureField` has no reveal toggle or `prompt:`; `Form` has no `formStyle`.
- Images: SF Symbols and a bitmap file (`AsyncImage` sends `bitmap`). The default symbol is a monochrome mask tinted with the foreground. Hierarchical, palette, multicolor, and gradient are AppKit color bitmaps; the Phosphor fallback stays a mask. Palette is the foreground at full, 0.6, and 0.3 alpha. `symbolVariableValueMode` has no variable value. `allowedDynamicRange` is parsed and drawn in standard range. No `Image("asset")`, `symbolVariant`, or `Image(uiImage:)`. `AsyncImage` loads a file URL during the first evaluation; HTTP arrives on a later frame. The completion writes a wake pipe that `simcore_runtime_run` polls beside stdin, because the guest does not service Dispatch. The transaction is ignored, and the iOS 27 `URLRequest` initializers are omitted. A nil URL is an empty view. `aspectRatio` applies only to images (other views ignore it);
  `scaledToFill` crops to the proposed size. A `Label` inside a list row forces its icon to `imageScale(.large)`.
- Text styling (protocol section O): kerning and tracking both add point spacing to the fitted tracking. A positive `baselineOffset` lengthens the line downward and leaves the glyphs at the top. `fontDesign` uses `.New York`, `.SF NS Rounded`, or `.SF NS Mono` when that family is installed. Condensed `fontWidth` is 0.80 of the standard face and compressed is 0.70; expanded stays the standard face. `textScale(.secondary)` draws glyphs at 0.86 of the current size inside the unscaled line box. `allowsTightening` pulls tracking by up to 5% of the point size, then `minimumScaleFactor` scales, then truncation. `textCase` folds the stored label. `Font.Width` statics are -0.3, -0.2, 0, and 0.3.

- Liquid Glass: `GlassEffectContainer` does not merge/morph shapes, `glassEffectID`/`glassEffectUnion`/`glassEffectTransition` are missing, `interactive` has no visual effect, and a non-square `.circle` is a capsule.

- Shapes and paths (protocol section N): custom `Shape.path(in:)` is evaluated by the guest after the host reports laid-out bounds via `layout`; `Shape.sizeThatFits(_:)` probes fixed, square, aspect-ratio and ideal sizes (general proposal-dependent formulas fall back to greedy). `.containerShape(_:)` supports built-in `InsettableShape` / `RoundedRectangularShape` containers (custom or transformed containers fall back to `Rectangle`). `ContainerRelativeShape` and `ConcentricRectangle` resolve concentrically against the enclosing `.containerShape`; uncontained `ConcentricRectangle` has no device display-bezel radius, and `.inset(by:)` or shape combinators on them fall back to `path(in:)`. `Shape.layoutDirectionBehavior` mirrors when RTL, though the host layout direction is currently fixed to LTR.

- Dynamic Type is a host setting (`--dynamic-type`) that scales text styles only; `@Environment(\.dynamicTypeSize)` is read once from `SIMCORE_DYNAMIC_TYPE` at launch. A session's size is fixed at `create_session` (no API changes it later), so that value stays correct; the web demo's `setDynamicType` re-renders on the host only and the guest is not told.
- Value navigation: destinations registered by `navigationDestination` are remembered per stack (only the top screen is evaluated, so a stack that starts with a non-empty `path` has no destinations until its root has been evaluated once). A value with no registered destination pushes nothing and logs SwiftUI's "no matching navigationDestination" warning to stderr (there is no guest-to-host error channel). Destination-style `NavigationLink`s in a `NavigationStack(path:)` push on top of the path-driven screens and back pops them first; a value pushed from such a screen goes on that screen stack rather than into the bound path.
- `onChange` values must be `Equatable`; the last value is kept per structural path and compared after each evaluation pass (callbacks run after `onAppear`; if they dirty state the root re-evaluates, within the same 8-pass cap). A view that leaves the tree forgets its value, so `initial: false` does not fire when it returns.
- `onTapGesture` with `count: 1` emits a plain-style `button` node (the host's button hit testing, no accent tint): it behaves as a button for accessibility and swallows taps meant for an enclosing button/row. Other counts, and the `coordinateSpace:` form, are gestures.
- Gestures (protocol section L): no timers in the guest, so a gesture never waits for another one to fail: with a double tap and a single tap on one view (or `TapGesture(count: 2).exclusively(before: TapGesture())`), the first tap of a double tap also fires the single tap. `CoordinateSpace.named` and `.coordinateSpace(name:)` are the screen (global). `Transaction` carries no animation (animations are not simulated) and `resetTransaction` is ignored. `onChanged` does not require `Equatable` values. `predictedEndLocation` projects 0.25 s of the release velocity, not UIKit's deceleration curve. A parent's gesture covers its subviews moved by `.offset` only within the parent's frame of the moved child. The native window and the web page have one finger: pinch and rotate come from the API/CLI (`pinch`, `rotate` pointer kinds). Views without gestures, like the rest of SwiftUI's hit testing, are transparent to touches; `contentShape` and `allowsHitTesting` are not implemented.
- `@AppStorage` uses `UserDefaults.standard` (persisted, per process name) on the native build and an in-memory store on the web/WASI build; no `store:`, optional or `URL`/`Data`/`Date` values, and changes made outside the wrapper (other `UserDefaults` writes) show up on the next evaluation only.
- Tabs: no `TabSection`, `Tab(role: .search)`, `.tabViewStyle` (always the iPhone floating glass bar; `.page` is missing), `tabBarMinimizeBehavior`, `tabViewBottomAccessory`, customization, or `ForEach` of tabs (tabs must be listed). `Tab` is a `View`, so it shares `@ViewBuilder` with `.tabItem` children. Re-selecting the current tab does not pop its `NavigationStack`. Symbols always draw the `.fill` variant when one exists, like the iOS tab bar. `.badge` elsewhere (list rows) is ignored.
- Lazy stacks, grids, geometry and scroll targets (protocol section M): `LazyVStack` and `LazyHStack` build every child. Section headers pin inside a scroll view; section footers do not. `LazyVGrid` and `LazyHGrid` build every child and ignore `pinnedViews`. `Grid` sizes columns to their cells and does not stretch them; default spacing is 8. `ViewThatFits` builds every child and shows the first that fits. `GeometryReader` gets one correction pass: `safeAreaInsets` are the device insets from `SIMCORE_INSETS` (leading and trailing stay 0), `frame(in: .local)` is the origin, and `.global` and a named space are the host screen frame. Scroll indicators are never drawn, including `.visible` and `showsIndicators: true`. `.scrollTargetBehavior` recognizes `.paging` and `.viewAligned`; a custom `updateTarget` is not called, `ViewAlignedScrollTargetBehavior.limitBehavior` is ignored, and `AnyScrollTargetBehavior` does not emit those keys. `.contentMargins` applies its length to the given edges of a `ScrollView` only; `ContentMarginPlacement` does not change the insets. `.scrollPosition(id:)` is the iOS 17 binding for `String` and `Int`, written back when the scroll settles. `.id` names a view for `scrollTo` and does not reset `@State`. `ScrollViewProxy.scrollTo` takes a unit-point anchor and does not animate.
- A modifier on a multi-child `Group` applies to each child, as in SwiftUI (other multi-root views are still wrapped in a vstack).
- `onAppear`/`onDisappear` fire per structural path after the evaluation pass (a screen covered by a push disappears, and appears again on pop); if they dirty state the root is re-evaluated, at most 8 times, before the tree is sent. `@EnvironmentObject`/`@Environment` are snapshotted when a view's `body` runs.
- Animation & transitions (protocol section P): `onesim_render::Animator` interpolates `bounds`, `opacity`, `fill`, `stroke`, `radius`, and text/symbol/glyph colors on the host; `PhaseAnimator`, `KeyframeAnimator`, `TimelineView`, and `withAnimation` completion callbacks advance via guest `schedule` / host `tick` messages (CLI `--wait MS`). Custom `Transition` bodies extract `opacity` and `offset`; `AnyTransition.modifier(active:identity:)` is not implemented. `BlurReplaceTransition` animates opacity and scale without Gaussian blur. `CustomAnimation` is sampled over up to 2 s (`shouldMerge` is not called). Custom `Animatable` views do not interpolate `animatableData` per frame outside `KeyframeAnimator`. `.contentTransition` supports cross-fade and vertical numeric roll without per-glyph morphing.
- Not implemented: `.environment(_:_:)`; `Color.opacity` on semantic colors is ignored.
- `@State` only inside `View`s (not `App`); the `WindowGroup` content closure runs once.
- Structural identity is positional (`if`/`else` branches use distinct `/T` and `/F` paths; state is distinguished by path and view type).
- Whole-root re-evaluation on every state change.
- `Text` takes `String` (no `LocalizedStringKey`); `foregroundColor(nil)` maps to primary.
- `-swift-version 5` language mode is used for the module and apps. macOS/Darwin only for now (autorelease pool, Foundation).
- `-parse-as-library` is passed only when sources contain `@main`; `@main` must start its line.
