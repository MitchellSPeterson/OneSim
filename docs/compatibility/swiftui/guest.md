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
| Environment | `@Environment(\.colorScheme)`, `EnvironmentValues.colorScheme`, `ColorScheme`; follows `SIMCORE_APPEARANCE` and `environment` messages. `@Environment(\.dismiss)` (`DismissAction`, pops the enclosing `NavigationStack`), `@Environment(\.dynamicTypeSize)` (`DynamicTypeSize`, from `SIMCORE_DYNAMIC_TYPE` at launch, default `.large`). `@EnvironmentObject` + `.environmentObject(_:)`. `EnvironmentKey` + `EnvironmentValues` subscripts, `.environment(_:_:)`, `\.isEnabled` (follows `.disabled`), `\.horizontalSizeClass` / `\.verticalSizeClass` (`UserInterfaceSizeClass`, from the viewport), `\.openURL` (`OpenURLAction`), `\.scenePhase` (`ScenePhase`, always `.active`). `@Observable` (Observation, re-exported), `@Bindable`, `@Environment(Model.self)` + `.environment(_ object:)` |
| Navigation | `NavigationStack { }`, `NavigationLink(_:destination:)`, `NavigationLink(destination:label:)`, `navigationTitle`, `navigationBarTitleDisplayMode`, `NavigationLink(_:value:)`, `NavigationLink(value:label:)`, `.navigationDestination(for:destination:)`, `NavigationStack(path:)` (`Binding<NavigationPath>` or `Binding<[T]>`), `NavigationPath` (`append`, `removeLast`, `count`, `isEmpty`) |
| Containers | `Group`, `List`, `List(data, rowContent:)` / `List(data, id:, rowContent:)` (Identifiable, key path, `Range<Int>`), `List(data, children:rowContent:)` / `List(data, id:children:rowContent:)`, `.listStyle(.automatic/.insetGrouped/.grouped/.plain/.inset/.sidebar)`, `Section(_:content:)`, `Section(content:)`, `Section(content:header:)`, `Section(content:footer:)`, `Section(content:header:footer:)`, `Section(header:footer:content:)`, `Section(_:isExpanded:content:)`, `Section(isExpanded:content:header:)`, `ForEach` (Range<Int>, `id:` key path, Identifiable; `DynamicViewContent`), `.onDelete(perform:)`, `.onMove(perform:)`, `.swipeActions(edge:allowsFullSwipe:content:)`, `EditButton`, `EditMode`, `\.editMode`, `remove(atOffsets:)`, `move(fromOffsets:toOffset:)`, `DisclosureGroup` (`isExpanded:`, title and label forms), `OutlineGroup` (data or root, `children:`), `OutlineSubgroupChildren`, `GroupBox` (title, label and label-less forms), `ControlGroup` (content, `content:label:`, title, `systemImage:`), `LabeledControlGroupContent`, `ContentUnavailableView` (label/description/actions, `title:systemImage:description:`, `.search`, `.search(text:)`), `Table(_:columns:)` with `TableColumn(_:value:)`, `TableColumn(_:content:)`, `TableColumn(_:value:comparator:)`, `@TableColumnBuilder`, `ScrollView(_ axes:)`, `Divider` |
| Controls | `Toggle(_:isOn:)`, `Stepper(_:value:in:step:)` (Strideable), `.buttonStyle(.automatic/.plain/.bordered/.borderedProminent)`, `.disabled(_:)` |
| Tabs | `TabView { }`, `TabView(selection:)` with `.tag(_:)`, `.tabItem { Label(_:systemImage:) }` / `.tabItem { Image(systemName:); Text(_:) }`, iOS 18 `Tab(_:systemImage:content:)` / `Tab(_:systemImage:value:content:)`, `.badge(_:)` (`Int`, `StringProtocol`, or plain text from `Text`). Pass `0` or an optional `nil` to hide the badge. `.tabViewStyle` accepts `.automatic`, `.tabBarOnly`, `.sidebarAdaptable` (iPhone tab bar), and `.page`. Page indices support `.page(indexDisplayMode:)` and `.indexViewStyle(.page(backgroundDisplayMode:))`. Bar styles evaluate the selected tab and preserve the other tabs' state. Page style evaluates all pages (protocol section J). |
| Text input | `TextField(_:text:prompt:axis:)`, `SecureField(_:text:prompt:)`, view-builder label variants, `TextEditor(text:)`, `.textFieldStyle` (automatic, plain, bordered, rounded border, custom), `.textInputBorderShape`, `.textEditorStyle` (automatic, plain, custom), `@FocusState` with `.focused(_:)` and `.focused(_:equals:)`, `.keyboardType`, `.textInputAutocapitalization`, `.autocorrectionDisabled` and their deprecated counterparts, `.textContentType`, `.onSubmit(of:_:)`, `.submitLabel`, `.submitScope` |
| Forms | `Form { }` (inset-grouped list), `.formStyle(.automatic/.grouped/.columns/custom FormStyle)`, `Form(_: FormStyleConfiguration)`, `LabeledContent(_:value:)`, `LabeledContent(_:value:format:)` (native targets), `LabeledContent(_:content:)`, `LabeledContent(content:label:)`, `LabeledContent(_: LabeledContentStyleConfiguration)`, `.labeledContentStyle`, `.groupBoxStyle`, `.disclosureGroupStyle`, `.controlGroupStyle(.automatic/.navigation/.menu/.compactMenu/.palette/custom)`, `Picker(_:selection:content:)` / `Picker(selection:content:label:)` with `.tag(_:)`, `.pickerStyle(.automatic/.menu/.segmented)`, `Slider(value:in:)`, `Slider(value:in:step:)` (+ `label:` variants; `BinaryFloatingPoint`), `ProgressView()`, `ProgressView(_:)`, `ProgressView(value:total:)`, `ProgressView(_:value:total:)`, `ProgressView(label:)` |
| Menus, links and control styles | `Menu(_:content:)`, `Menu(_:systemImage:content:)`, `Menu(content:label:)`, `Menu(content:label:primaryAction:)`, `Menu(_: MenuStyleConfiguration)`, `.menuStyle(.automatic/.button/.borderlessButton/custom MenuStyle)`, `Link(_:destination:)`, `Link(destination:label:)`, `OpenURLAction(handler:)` (`.handled`, `.discarded`, `.systemAction`, `.systemAction(_:)`), `@Environment(\.openURL)`, `.environment(\.openURL, _)`, `Button(_:systemImage:role:action:)`, `.buttonStyle(.borderless)`, `ButtonStyle` / `PrimitiveButtonStyle` (`makeBody(configuration:)`, `configuration.label`, `.role`, `.isPressed`, `.trigger()`), `.toggleStyle(.automatic/.switch/.button/custom ToggleStyle)`, `Toggle(_: ToggleStyleConfiguration)`, `.pickerStyle(.inline/.wheel/.navigationLink/.palette)`, `.labelsHidden()`, `Slider(value:in:label:minimumValueLabel:maximumValueLabel:onEditingChanged:)` (+ `step:` and `onEditingChanged:` on every slider init), `ProgressView(value:total:label:currentValueLabel:)`, `ProgressView(_: ProgressViewStyleConfiguration)`, `.progressViewStyle(.automatic/.linear/.circular/custom)`, `Gauge(value:in:label:)` (+ `currentValueLabel:`, `minimumValueLabel:maximumValueLabel:`), `.gaugeStyle(.automatic/.linearCapacity/.accessoryLinear/.accessoryLinearCapacity/.accessoryCircular/.accessoryCircularCapacity/custom GaugeStyle)` |
| Shapes/styles | `Shape` (custom `path(in:)`, `role`, `layoutDirectionBehavior`, `sizeThatFits`), `InsettableShape` (`inset(by:)`, `.strokeBorder`), `RoundedRectangularShape`, `ShapeView` (`FillShapeView`, `StrokeShapeView`, `StrokeBorderShapeView`, chainable `.fill`/`.stroke`/`.strokeBorder`), `Path` (builders, arcs, `description`/`init?(_:)`, `boundingRect`, `contains`, `trimmedPath`, `strokedPath`, boolean ops, `cgPath` on Apple platforms), `Rectangle`, `Circle`, `Ellipse`, `Capsule(style:)`, `RoundedRectangle(cornerRadius:/cornerSize:style:)`, `UnevenRoundedRectangle`, `ContainerRelativeShape`, `ConcentricRectangle` (with `Edge.Corner.Style`), `AnyShape`, `OffsetShape`/`.offset`, `RotatedShape`/`.rotation`, `ScaledShape`/`.scale`, `TransformedShape`/`.transform`, `.trim`, `.size`, `.stroke(style:/lineWidth:)` (shape), `.union`/`.intersection`/`.subtracting`/`.symmetricDifference`/`.lineIntersection`/`.lineSubtraction`, `.containerShape(_:)`, `StrokeStyle`, `FillStyle`, `RoundedCornerStyle`, `RectangleCornerRadii`, `RoundedRectangularShapeCorners`, `ProposedViewSize`, `ShapeRole`, `LayoutDirection`, `LayoutDirectionBehavior`, `ShapeStyle` (`Color`, `ForegroundStyle`, `.primary/.secondary/.tertiary`), `foregroundStyle`, `Color.opacity(_:)`. Protocol section N. |

| More modifiers | `font` (`Font` text styles, `.system(size:weight:design:)`), `fontWeight`, `bold`, `strikethrough`, `multilineTextAlignment`, `overlay` (both forms), `shadow` |

| Text styling | `kerning(_:)`, `tracking(_:)`, `baselineOffset(_:)`, `fontDesign(_:)`, `fontWidth(_:)`, `monospaced(_:)`, `monospacedDigit()`, `textScale(_:)`, `textCase(_:)`, `truncationMode(_:)`, `lineLimit` (count, `reservesSpace`, and ranges), `minimumScaleFactor(_:)`, `allowsTightening(_:)`, `underline(_:pattern:color:)`, `strikethrough(_:pattern:color:)`. A run's kerning and tracking add to the parent's. Protocol section O |

| Images | `renderingMode(_:)`, `symbolRenderingMode(_:)`, `symbolColorRenderingMode`, `symbolVariableValueMode`, `interpolation(_:)`, `antialiased(_:)`, `allowedDynamicRange`, `AsyncImage(url:)` (a file URL loads during the first evaluation; nil stays empty). Protocol section O |

| Lifecycle | `.onAppear { }`, `.onDisappear { }`, `.onChange(of:initial:_:)` (`{ old, new in }` and `{ }`), `.onChange(of:perform:)` |
| Gestures | `TapGesture(count:)`, `SpatialTapGesture(count:coordinateSpace:)`, `LongPressGesture(minimumDuration:maximumDistance:)`, `DragGesture(minimumDistance:coordinateSpace:)`, `MagnifyGesture(minimumScaleDelta:)`, `RotateGesture(minimumAngleDelta:)`, deprecated `MagnificationGesture` / `RotationGesture`, `SimultaneousGesture` / `SequenceGesture` / `ExclusiveGesture` (+ `simultaneously(with:)`, `sequenced(before:)`, `exclusively(before:)`), `AnyGesture`, custom `Gesture` bodies, `onChanged`, `onEnded`, `map`, `updating(_:body:)` (`GestureStateGesture`), `@GestureState` (`wrappedValue:`, `reset:`), `.gesture(_:including:)`, `.gesture(_:isEnabled:)`, `.simultaneousGesture`, `.highPriorityGesture`, `GestureMask`, `.onTapGesture(count:perform:)`, `.onTapGesture(count:coordinateSpace:perform:)`, `.onLongPressGesture(minimumDuration:maximumDistance:perform:onPressingChanged:)` (and `pressing:perform:`), `.offset(_:)` / `.offset(x:y:)`; `Angle`, `UnitPoint`, `CoordinateSpace` (`.local`, `.global`, `.named`), `Transaction`. Protocol section L. |
| Storage | `@AppStorage("key")` and `@SceneStorage("key")` for Bool/Int/Double/String and `RawRepresentable` (String/Int raw), `$x` -> `Binding`; `@ScaledMetric` (`relativeTo:` a `Font.TextStyle`) |
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
`disabled: true` and action `0` (never registered). Text controls allocate a `field` id per evaluation. A version-gated
`text_input` message sets the bound String via `simcore_text_cb`. Focus and submit callbacks use the same action
mechanism as buttons. Return runs the submit action if one is registered. Otherwise, Return inserts a newline in multiline controls. `Slider` also uses `text_input`. The guest clamps the value and snaps it to `step`.
A `Picker` registers one action per tagged option (protocol section I).

`NavigationView { ... }` uses a compact stack. `NavigationSplitView` supports two or three columns with
`preferredCompactColumn:` as an optional binding. The visible column has its own stack and keeps its state
when hidden. Back writes the previous column to the binding. Regular-width layout, split visibility changes,
and selection-driven column routing are pending.

`.toolbar` accepts view content and `ToolbarContent` builders. `ToolbarItem` and `ToolbarItemGroup`
place content at the top leading edge, principal position, top trailing edge, or bottom of the navigation
container. `.toolbarVisibility`, `.toolbarBackground`, and `.toolbarBackgroundVisibility` control the
navigation and bottom bars. `.toolbarTitleDisplayMode` supports inline and large titles.
`.toolbarTitleMenu` opens a menu from the title. Toolbar customization, overflow, keyboard and tab bar
placements, and automatic title-mode inheritance are pending.

`.searchable(text:placement:prompt:)` and its `isPresented:` overload add a search drawer to navigation
chrome. The active field binds its text and supplies `\.isSearching` and `\.dismissSearch` to descendants.
Dismissing clears the query and ends presentation. Token and suggestion overloads, localized resources,
and placement-specific behavior outside the drawer are pending.

### Forms and containers

Lists and forms (protocol section T):

- `.listStyle` picks the `list` style. `.automatic` and every grouped `Form` are inset grouped. Section headers and
  footers may be any view; a text-only one is sent as text and drawn in the list's header or footer style.
  `Section(_:isExpanded:)` sends no rows while collapsed, and its header toggles the binding.
- Each NavigationStack screen, and the root, has its own `\.editMode`, unless an ancestor sets it with
  `.environment(\.editMode, _)`. `EditButton` toggles it (Edit / Done). While editing, rows from a `ForEach` with
  `.onDelete` get a delete control (it opens the row's Delete action) and rows with `.onMove` a reorder handle; a drag
  on the handle moves the element by whole rows of the dragged row's height. Swipe actions are off while editing.
- `.swipeActions` collects buttons per edge, outermost first (several modifiers on one edge add up). Without
  trailing actions, `.onDelete` supplies a Delete action. A horizontal drag moves the row, opens an edge past 30 pt,
  and runs the outermost action past 55% of the row's width when `allowsFullSwipe`. A tap on an open row, an
  action, or opening another row closes it. Vertical drags scroll the list. On a row holding a `Button` or
  `NavigationLink`, a tap runs the control and a horizontal drag swipes the row. A disabled list or `ForEach` has no
  delete control action, Delete action, reorder or swipe.
- `DisclosureGroup` in a list is a row with a chevron; its content rows follow, indented 20 pt. Elsewhere it is an
  accent button above its content. Without `isExpanded:` it keeps its own state. `OutlineGroup` and
  `List(_:children:)` nest disclosure groups per element.
- `.formStyle(.columns)` lays labels (trailing-aligned) beside their controls and section headers over the controls,
  centred in the screen when it fits; a `Toggle` puts its label on the left and its switch on the right.
- Custom `FormStyle`, `LabeledContentStyle`, `DisclosureGroupStyle`, `GroupBoxStyle` and `ControlGroupStyle` get their
  configurations; `Form(configuration)`, `LabeledContent(configuration)`, `DisclosureGroup(configuration)`,
  `GroupBox(configuration)` and `ControlGroup(configuration)` draw the automatic look inside one. `.menu` and
  `.compactMenu` control groups are menus labelled by the group's label (or "…").
- `Table` shows its first column as a plain list, as on every iPhone (compact width). Column headers, sorting,
  selection, `TableRow` / row builders and multi-column layouts (regular width) are not implemented.

Limits: list selection (`List(selection:)`), `.listRow*` / `.listSection*` modifiers, `onInsert`, swipe-action icons
other than a `Label`'s symbol beside its title, and the reorder drag's live shuffling of the other rows are gaps.
The drag lands on whole rows of the dragged row's height.

### Date and color selection

- `DatePicker(selection:displayedComponents:label:)` takes a `Binding<Date>`, an optional closed or one-sided
  inclusive range, and the `.date` and `.hourAndMinute` components. Use a string title in place of the label closure.
  `.datePickerStyle(_ :)` accepts `.automatic`, `.compact`, `.graphical`, `.wheel`, and custom `DatePickerStyle`
  implementations. A custom style can reuse the selection, label and bounds through `DatePicker(configuration)`.
- `MultiDatePicker(selection:label:)` takes a `Binding<Set<DateComponents>>`. Its range overloads take a
  half-open range, an inclusive lower bound or an exclusive upper bound. A tap toggles a Gregorian civil day.
- `ColorPicker(selection:supportsOpacity:label:)` takes a `Binding<Color>`, or a `Binding<CGColor>` on native targets.
  Grid, spectrum, RGB, hex, opacity and session favorites update the binding. `supportsOpacity: false` hides
  the opacity control and writes opaque colors.

The pickers use `\.calendar`, `\.timeZone` and `\.locale`. Calendar arithmetic is Gregorian. Native guests
use Foundation locale formatting and time zones. WASI uses English text and fixed GMT offsets.
Localized title-key and title-resource overloads, wheel momentum, the eyedropper, and persistent color
favorites are incomplete. Calendar width still follows the viewport in layouts with narrower parents.

### Presentation

- `.alert(_:isPresented:actions:message:)`, `.alert(_:isPresented:actions:)` (no actions: an OK button), `.alert(_:isPresented:presenting:actions:message:)` / `presenting:actions:`.
- `.confirmationDialog(_:isPresented:titleVisibility:actions:message:)`, without `message`, and the `presenting:` forms (rendered as the iPhone popover; a Cancel is added when the app has none, and tapping outside cancels).
- `.sheet(isPresented:onDismiss:content:)`, `.sheet(item:onDismiss:content:)`, `.fullScreenCover(isPresented:onDismiss:content:)`, `.fullScreenCover(item:onDismiss:content:)`.
- `.presentationDetents(_:)` (`.medium`, `.large`, `.fraction(_:)`, `.height(_:)`; starts at the smallest), `.presentationDragIndicator(_:)`, `.interactiveDismissDisabled(_:)`; `Visibility`.
- Sheets and covers can present further sheets/alerts (a stack). `@Environment(\.dismiss)` inside a sheet or cover clears its binding (a pushed screen inside it is popped first). `onDismiss` runs whenever the presentation goes away: `dismiss()`, a swipe, or app code clearing the binding.
- `.popover(isPresented:attachmentAnchor:arrowEdge:content:)` and `.popover(item:…)`: a sheet on iPhone, or with `.presentationCompactAdaptation(.popover)` / `.none` a glass bubble whose arrow points at the attachment anchor (`.rect(.bounds)`, `.rect(.rect(_:))`, `.point(_:)`); `.fullScreenCover` / `.sheet` adaptations switch kinds. A tap outside the bubble dismisses it.
- `.presentationBackground(_:)` / `(alignment:content:)`, `.presentationCornerRadius(_:)`, `.presentationBackgroundInteraction(_:)` (no dim and a live screen below up through the detent), `.presentationContentInteraction(_:)`; `.presentationSizing(_:)` and `.presentationPlacement(_:)` compile and, as on iPhone, change nothing. Presentation modifiers work on any view of the presented content.
- `.contextMenu(menuItems:)`, `(menuItems:preview:)`, the deprecated `contextMenu(_: ContextMenu?)` and `(forSelectionType:menu:primaryAction:)` (per `ForEach` row of the selection type). A 0.5 s long press opens it; a `Button` it is attached to still taps.
- `ShareLink` (every initializer) opens the share sheet for text or links; Copy puts the items' text on the pasteboard, other activities report a `module.called` event. `PasteButton(payloadType:onPaste:)` and `PasteButton(supportedContentTypes:…)` paste it through the type's transfer representations (CoreTransferable is declared in the module: Proxy, Codable, Data and File representations). The pasteboard follows the host clipboard in `onesim window` and the browser clipboard on the web; `--pasteboard TEXT` fixes it (protocol section U).
- Swipe: a sheet follows a downward drag (dismisses past 25% of the way down, settles at the nearest detent otherwise) and an upward drag expands it; content scrolls first at the largest detent. See protocol section G for the gesture rules.

Presented content is evaluated in place (a sheet's `@State` lives at `<view path>/sheet` only while shown) and hoisted by the runtime above the whole tree as a `presentation` node (protocol section G). Any `Button` inside alert/dialog actions dismisses it after its own action. `Button(_:role:action:)` takes `ButtonRole` `.cancel` / `.destructive`.

Limits: no animations (settled states only; the sheet snaps to its detent on release; context menus appear without the lift), no `dialogSeverity`, `alert` with text fields or `Text` titles, or `inspector`. Image and file items share their text form only. Presented content is hoisted above the whole screen, not just its presenting view. The popover points at the first text of the view the dialog is attached to. `Text` runs carry kerning, tracking, baseline offset, font design and width, and underline or strikethrough pattern and color. A concatenation's line height follows its largest run. `Text` is still `String` only.

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

- Text input: edits append at the caret at the end of the string. The coverage rows list gaps in field and editor overloads, styles, and keyboard behavior.
- Form controls (protocol section I): picker options are the tagged children's first text (no images in options). The menu is a static card (no open animation, no scroll for long lists). `Slider` has no tint or ticks; it moves on a tap or a whole `drag` gesture, not live while dragging, so `onEditingChanged` fires `true` then `false` around each change. `ProgressView()` draws one static spinner frame. `LabeledContent` styles its content `.secondary` (also non-text content), no `labeledContentStyle`. `Form` has no `formStyle`.
- Controls (protocol section Q): custom styles (`ButtonStyle`, `PrimitiveButtonStyle`, `ToggleStyle`, `ProgressViewStyle`, `GaugeStyle`, `MenuStyle`) are evaluated in place of the control with that style cleared, so the controls in their body draw unstyled. `ButtonStyleConfiguration.isPressed` is always false and `ToggleStyleConfiguration.isMixed` always false. `.palette` pickers draw as segmented controls (their look outside a menu); `.inline` is checkmark rows inside a `List`/`Form` (no label row) and a wheel elsewhere; `.navigationLink` without a `NavigationStack` is a menu picker; a wheel shows the selection and three rows each side and selects the row you tap (no spinning), so further options take several taps. No `.tabs` picker style. A `Menu` opens on a tap and its items are its buttons (text, first symbol, destructive role, dimmed when disabled), toggles (a checkmark when on) and pickers (their options inline between separators); a custom `ButtonStyle` does not style a `Menu`'s label (built-in styles do); a `Divider` or `Section` puts a separator between items (no section headers); a nested `Menu` opens its own card; with `primaryAction` the menu is unreachable (iOS opens it with a long press). `NavigationLink` outside a `List`/`Form` is an ordinary accent button (the chevron row only inside lists). `Link` runs `\.openURL`; the system action is reported as a `module.called` event (there is no browser), and on the web build a `Link` does nothing. `.circular` progress is always the spinner, as on iOS; `.linear` without a value also draws the spinner. Gauges have no `markedValueLabels` and accessory styles draw in the primary color (no `tint`).
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
- `@AppStorage` uses `UserDefaults.standard` (persisted, per process name) on the native build and an in-memory store on the web/WASI build; no `store:`, optional or `URL`/`Data`/`Date` values, and changes made outside the wrapper (other `UserDefaults` writes) show up on the next evaluation only. `@SceneStorage` keeps one value per key for the session in memory (one scene); it is not restored after a relaunch.
- `@Observable` objects: every property read while the tree is evaluated is tracked (Observation's `withObservationTracking`); the next change to any of them re-evaluates the whole root. `.environment(\.colorScheme, _)` and other built-in overrides change what `@Environment` reads below, not how the host draws the subtree; `\.isEnabled` is the exception (it disables controls like `.disabled`). `\.openURL` without a handler (or a handler returning `.systemAction`) sends the guest event `module.called` `{"module":"OpenURLAction","method":"openURL","args":[url]}` and calls the completion with `true`; nothing opens and `scenePhase` stays `.active`. `OpenURLAction` needs Foundation's `URL`, so it is missing from the web build. `@ScaledMetric` uses Apple's Dynamic Type point sizes for its text style (the launch size, like `\.dynamicTypeSize`).
- Tabs: `.sidebarAdaptable` renders the compact iPhone tab bar and `isTabViewSidebarAvailable` returns `false`. Tablet and desktop sidebar layouts, custom styles, `TabSection`, search tabs, tab bar minimization, bottom accessories and customization are unsupported. Reselecting a tab preserves its navigation stack. Page changes snap on release or when the binding changes. Page indices support tapping individual dots. Dragging across dots and the `.interactive` background are unsupported. Badges render as plain text on tab items. List badges and localized resource overloads are unsupported.
- Lazy stacks, grids, geometry and scroll targets (protocol section M): `LazyVStack` and `LazyHStack` build every child. Section headers pin inside a scroll view; section footers do not. `LazyVGrid` and `LazyHGrid` build every child and ignore `pinnedViews`. `Grid` sizes columns to their cells and does not stretch them; default spacing is 8. `ViewThatFits` builds every child and shows the first that fits. `GeometryReader` gets one correction pass: `safeAreaInsets` are the device insets from `SIMCORE_INSETS` (leading and trailing stay 0), `frame(in: .local)` is the origin, and `.global` and a named space are the host screen frame. Scroll indicators are never drawn, including `.visible` and `showsIndicators: true`. `.scrollTargetBehavior` recognizes `.paging` and `.viewAligned`; a custom `updateTarget` is not called, `ViewAlignedScrollTargetBehavior.limitBehavior` is ignored, and `AnyScrollTargetBehavior` does not emit those keys. `.contentMargins` applies its length to the given edges of a `ScrollView` only; `ContentMarginPlacement` does not change the insets. `.scrollPosition(id:)` is the iOS 17 binding for `String` and `Int`, written back when the scroll settles. `.id` names a view for `scrollTo` and does not reset `@State`. `ScrollViewProxy.scrollTo` takes a unit-point anchor and does not animate.
- A modifier on a multi-child `Group` applies to each child, as in SwiftUI (other multi-root views are still wrapped in a vstack).
- `onAppear`/`onDisappear` fire per structural path after the evaluation pass (a screen covered by a push disappears, and appears again on pop); if they dirty state the root is re-evaluated, at most 8 times, before the tree is sent. `@EnvironmentObject`/`@Environment` are snapshotted when a view's `body` runs.
- Animation & transitions (protocol section P): `onesim_render::Animator` interpolates `bounds`, `opacity`, `fill`, `stroke`, `radius`, and text/symbol/glyph colors on the host; `PhaseAnimator`, `KeyframeAnimator`, `TimelineView`, and `withAnimation` completion callbacks advance via guest `schedule` / host `tick` messages (CLI `--wait MS`). Custom `Transition` bodies extract `opacity` and `offset`; `AnyTransition.modifier(active:identity:)` is not implemented. `BlurReplaceTransition` animates opacity and scale without Gaussian blur. `CustomAnimation` is sampled over up to 2 s (`shouldMerge` is not called). Custom `Animatable` views do not interpolate `animatableData` per frame outside `KeyframeAnimator`. `.contentTransition` supports cross-fade and vertical numeric roll without per-glyph morphing.
- Not implemented: `Color.opacity` on semantic colors is ignored.
- `@State` only inside `View`s (not `App`); the `WindowGroup` content closure runs once.
- Structural identity is positional (`if`/`else` branches use distinct `/T` and `/F` paths; state is distinguished by path and view type).
- Whole-root re-evaluation on every state change.
- `Text` takes `String` (no `LocalizedStringKey`); `foregroundColor(nil)` maps to primary.
- `-swift-version 5` language mode is used for the module and apps. macOS/Darwin only for now (autorelease pool, Foundation).
- `-parse-as-library` is passed only when sources contain `@main`; `@main` must start its line.

### Gradients, Canvas and visual effects

| API | Supported entry points |
| --- | --- |
| Gradient views and styles | `LinearGradient(gradient:/colors:/stops:, startPoint:endPoint:)`, `RadialGradient(gradient:/colors:/stops:, center:startRadius:endRadius:)`, `EllipticalGradient(gradient:/colors:/stops:, center:startRadiusFraction:endRadiusFraction:)`, `AngularGradient(gradient:/colors:/stops:, center:startAngle:endAngle:)` and `AngularGradient(gradient:/colors:/stops:, center:angle:)` |
| Gradient data and erasure | `Gradient(colors:)`, `Gradient(stops:)`, `Gradient.Stop(color:location:)`, `Gradient.colorSpace(_:)`, `AnyGradient(_:)`, `AnyShapeStyle(_:)`, `MeshGradient(width:height:locations:colors:background:smoothsColors:colorSpace:)` and its point, Bézier and resolved-color overloads |
| Canvas | `Canvas(opaque:colorMode:rendersAsynchronously:renderer:)` and `Canvas(opaque:colorMode:rendersAsynchronously:renderer:symbols:)` |
| GraphicsContext | `fill(_:with:style:)`, `stroke(_:with:style:)`, `stroke(_:with:lineWidth:)`, `resolve(_:)`, `resolveSymbol(id:)`, `draw(_:at:anchor:)`, `draw(_:in:)`, `drawLayer(content:)`, `clip(to:style:options:)`, `clipToLayer(opacity:options:content:)`, `addFilter(_:options:)` |
| View effects | `blur(radius:opaque:)`, `blendMode(_:)`, `mask(alignment:_:)`, `mask(_:)`, `rotationEffect(_:anchor:)`, `scaleEffect(_:anchor:)`, `scaleEffect(x:y:anchor:)`, `transformEffect(_:)`, `projectionEffect(_:)`, `visualEffect(_:)`, `compositingGroup()`, `drawingGroup(opaque:colorMode:)` |

- Use linear, radial, angular and elliptical gradients as views, shape fills or
  strokes, and foreground or background paints. The gradient, color and stop
  initializers and the corresponding shape-style factories share the same paint.
  Mesh gradients accept point grids or Bézier controls and resolved or environment
  colors. Apple color-space interpolation and folded mesh patches are approximate.
- `AnyShapeStyle` erases supported styles. `.foreground`, `.background` and `.tint`
  inherit their paint. The two- and three-style foreground modifiers supply
  explicit secondary and tertiary styles. System fill levels and selection colors are approximate.
  Gradient symbol palettes, per-run gradient text, safe-area extension for style
  backgrounds and full tint inheritance for controls are gaps.
- `Canvas` draws paths, strokes, text, images and tagged symbols using the
  host-reported size. Context copies share their drawing destination and keep their
  clips, transforms, opacity, filters and blend mode independently. Supported
  filters include blur, projection, hue, saturation, brightness, contrast,
  inversion, grayscale, color multiply, luminance to alpha and alpha threshold.
  The guest runs the renderer synchronously. Resolved metrics, tiled images,
  shaders, shadow and matrix filters, linear filter options, blur dithering and
  Core Graphics access are gaps. Clip-layer bounds use the recorded drawing
  geometry. Text, filters and inverse clips can make those bounds conservative.
- View effects include affine transforms, projection, blur, blend mode, alpha
  masking, clipping, color filters, and compositing and drawing groups.
  `visualEffect` reads local size and the global frame after layout. Effects use
  raster layers in device color space. Hit testing stays in layout coordinates,
  and transformed content can be clipped by the viewport before transformation.
  Linear and extended-linear group backing, transform animation and native
  scroll coordinate spaces are gaps.
