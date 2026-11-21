#if os(WASI)
import WASILibc
#else
import Foundation
#endif

// MARK: - Strings

/// The key text of a localized string. No bundle lookup: the key (including interpolations) is the value.
public struct LocalizedStringKey: ExpressibleByStringInterpolation, Hashable, Sendable {
    public var key: String
    public init(_ key: String) { self.key = key }
    public init(stringLiteral value: String) { key = value }
    public init(stringInterpolation: StringInterpolation) { key = stringInterpolation.storage }
    public struct StringInterpolation: StringInterpolationProtocol {
        var storage = ""
        public init(literalCapacity: Int, interpolationCount: Int) { storage = "" }
        public mutating func appendLiteral(_ literal: String) { storage += literal }
        public mutating func appendInterpolation<T>(_ value: T) { storage += String(describing: value) }
    }
}

func _plain(_ t: Text) -> String { t.runs.map(\.text).joined() }

#if !os(WASI)
func _res(_ r: LocalizedStringResource) -> String { String(localized: r) }
#endif

// MARK: - Supporting types

public struct AccessibilityTraits: OptionSet, Hashable, Sendable {
    public let rawValue: UInt
    public init(rawValue: UInt) { self.rawValue = rawValue }
    public static let isButton = Self(rawValue: 1 << 0)
    public static let isHeader = Self(rawValue: 1 << 1)
    public static let isSelected = Self(rawValue: 1 << 2)
    public static let isLink = Self(rawValue: 1 << 3)
    public static let isSearchField = Self(rawValue: 1 << 4)
    public static let isImage = Self(rawValue: 1 << 5)
    public static let playsSound = Self(rawValue: 1 << 6)
    public static let isKeyboardKey = Self(rawValue: 1 << 7)
    public static let isStaticText = Self(rawValue: 1 << 8)
    public static let isSummaryElement = Self(rawValue: 1 << 9)
    public static let updatesFrequently = Self(rawValue: 1 << 10)
    public static let startsMediaSession = Self(rawValue: 1 << 11)
    public static let allowsDirectInteraction = Self(rawValue: 1 << 12)
    public static let causesPageTurn = Self(rawValue: 1 << 13)
    public static let isModal = Self(rawValue: 1 << 14)
    public static let isToggle = Self(rawValue: 1 << 15)
    public static let isTabBar = Self(rawValue: 1 << 16)
    func names() -> [String] {
        let all: [(Self, String)] = [
            (.isButton, "isButton"), (.isHeader, "isHeader"), (.isSelected, "isSelected"), (.isLink, "isLink"),
            (.isSearchField, "isSearchField"), (.isImage, "isImage"), (.playsSound, "playsSound"),
            (.isKeyboardKey, "isKeyboardKey"), (.isStaticText, "isStaticText"), (.isSummaryElement, "isSummaryElement"),
            (.updatesFrequently, "updatesFrequently"), (.startsMediaSession, "startsMediaSession"),
            (.allowsDirectInteraction, "allowsDirectInteraction"), (.causesPageTurn, "causesPageTurn"),
            (.isModal, "isModal"), (.isToggle, "isToggle"), (.isTabBar, "isTabBar"),
        ]
        return all.compactMap { contains($0.0) ? $0.1 : nil }
    }
}

public struct AccessibilityTechnologies: OptionSet, Hashable, Sendable {
    public let rawValue: UInt8
    public init(rawValue: UInt8) { self.rawValue = rawValue }
    public static let voiceOver = Self(rawValue: 1)
    public static let switchControl = Self(rawValue: 2)
    func names() -> [String] {
        var o: [String] = []
        if contains(.voiceOver) { o.append("voiceOver") }
        if contains(.switchControl) { o.append("switchControl") }
        return o
    }
}

public struct AccessibilityDirectTouchOptions: OptionSet, Hashable, Sendable {
    public let rawValue: UInt8
    public init(rawValue: UInt8) { self.rawValue = rawValue }
    public static let silentOnTouch = Self(rawValue: 1)
    public static let requiresActivation = Self(rawValue: 2)
    func names() -> [String] {
        var o: [String] = []
        if contains(.silentOnTouch) { o.append("silentOnTouch") }
        if contains(.requiresActivation) { o.append("requiresActivation") }
        return o
    }
}

public enum AccessibilityHeadingLevel: UInt, Hashable, Sendable {
    case unspecified = 0, h1, h2, h3, h4, h5, h6
}

public struct AccessibilityChildBehavior: Hashable, Sendable {
    let wire: String
    public static let ignore = Self(wire: "ignore")
    public static let contain = Self(wire: "contain")
    public static let combine = Self(wire: "combine")
}

public enum AccessibilityAdjustmentDirection: Hashable, Sendable {
    case increment, decrement
}

public struct AccessibilityActionKind: Hashable, Sendable {
    let name: String
    private init(name: String) { self.name = name }
    public static let `default` = Self(name: "default")
    public static let escape = Self(name: "escape")
    public static let magicTap = Self(name: "magicTap")
    /// macOS. Present so shared code compiles; iOS VoiceOver does not use it.
    public static let delete = Self(name: "delete")
    /// macOS. Present so shared code compiles; iOS VoiceOver does not use it.
    public static let showMenu = Self(name: "showMenu")
    public init(named name: Text) { self.name = _plain(name) }
}

public struct AccessibilityActionCategory: Hashable, Sendable {
    let name: String
    private init(name: String) { self.name = name }
    public static let `default` = Self(name: "default")
    public static let edit = Self(name: "edit")
    public init(_ name: Text) { self.name = _plain(name) }
    public init(_ nameKey: LocalizedStringKey) { self.name = nameKey.key }
    @_disfavoredOverload public init<S: StringProtocol>(_ name: S) { self.name = String(name) }
    #if !os(WASI)
    @_disfavoredOverload public init(_ nameResource: LocalizedStringResource) { self.name = _res(nameResource) }
    #endif
}

public struct AccessibilityTextContentType: Hashable, Sendable {
    let wire: String
    public static let plain = Self(wire: "plain")
    public static let console = Self(wire: "console")
    public static let fileSystem = Self(wire: "fileSystem")
    public static let messaging = Self(wire: "messaging")
    public static let narrative = Self(wire: "narrative")
    public static let sourceCode = Self(wire: "sourceCode")
    public static let spreadsheet = Self(wire: "spreadsheet")
    public static let wordProcessing = Self(wire: "wordProcessing")
}

public enum AccessibilityLabeledPairRole: Hashable, Sendable {
    case label, content
    var wire: String { self == .label ? "label" : "content" }
}

public struct AccessibilityZoomGestureAction: Hashable, Sendable {
    public enum Direction: Hashable, Sendable { case zoomIn, zoomOut }
    public let direction: Direction
    public let location: UnitPoint
    public let point: CGPoint
    public init(direction: Direction, location: UnitPoint, point: CGPoint) {
        self.direction = direction
        self.location = location
        self.point = point
    }
}

public struct AccessibilitySystemRotor: Hashable, Sendable {
    public let name: String
    public static func links(visited: Bool) -> Self { Self(name: visited ? "links:visited" : "links:unvisited") }
    public static var links: Self { Self(name: "links") }
    public static func headings(level: AccessibilityHeadingLevel) -> Self {
        Self(name: level == .unspecified ? "headings" : "headings:h\(level.rawValue)")
    }
    public static var headings: Self { Self(name: "headings") }
    public static var boldText: Self { Self(name: "boldText") }
    public static var italicText: Self { Self(name: "italicText") }
    public static var underlineText: Self { Self(name: "underlineText") }
    public static var misspelledWords: Self { Self(name: "misspelledWords") }
    public static var images: Self { Self(name: "images") }
    public static var textFields: Self { Self(name: "textFields") }
    public static var tables: Self { Self(name: "tables") }
    public static var lists: Self { Self(name: "lists") }
    public static var landmarks: Self { Self(name: "landmarks") }
}

public enum Accessibility {
    public enum AXCustomContent {
        public enum Importance: String, Hashable, Sendable {
            case `default`, high
        }
    }
}

public struct AccessibilityCustomContentKey: Hashable, Sendable {
    public var key: String
    public var id: String
    public init(_ label: Text, id: String) { self.key = _plain(label); self.id = id }
    public init(_ labelKey: LocalizedStringKey, id: String) { self.key = labelKey.key; self.id = id }
    public init(_ labelKey: LocalizedStringKey) { self.key = labelKey.key; self.id = labelKey.key }
    #if !os(WASI)
    @_disfavoredOverload public init(_ label: LocalizedStringResource, id: String) { self.key = _res(label); self.id = id }
    @_disfavoredOverload public init(_ label: LocalizedStringResource) { self.key = _res(label); self.id = key }
    #endif
}

/// Exists so attachments type-check. Modifiers return `some View`, not `ModifiedContent`.
public struct AccessibilityAttachmentModifier: Hashable, Sendable {
    public init() {}
}

open class AXChartDescriptor {
    public var title: String?
    public var summary: String?
    public init(title: String? = nil, summary: String? = nil) {
        self.title = title
        self.summary = summary
    }
}

public protocol AXChartDescriptorRepresentable {
    func makeChartDescriptor() -> AXChartDescriptor
    func updateChartDescriptor(_ descriptor: AXChartDescriptor)
}

extension AXChartDescriptorRepresentable {
    public func updateChartDescriptor(_ descriptor: AXChartDescriptor) {}
}

// MARK: - Rotors

public protocol AccessibilityRotorContent {
    associatedtype Body: AccessibilityRotorContent
    @AccessibilityRotorContentBuilder var body: Body { get }
    func _entries() -> [(label: String, id: String?)]
}

extension AccessibilityRotorContent {
    public func _entries() -> [(label: String, id: String?)] { body._entries() }
}

public struct AccessibilityRotorTuple: AccessibilityRotorContent {
    public typealias Body = AccessibilityRotorTuple
    public var body: AccessibilityRotorTuple { AccessibilityRotorTuple(entries: []) }
    let entries: [(String, String?)]
    public func _entries() -> [(label: String, id: String?)] { entries.map { (label: $0.0, id: $0.1) } }
}

public struct AccessibilityRotorEntry: AccessibilityRotorContent {
    public typealias Body = AccessibilityRotorTuple
    public var body: AccessibilityRotorTuple { AccessibilityRotorTuple(entries: []) }
    let label: String
    let entryID: String?
    public func _entries() -> [(label: String, id: String?)] { [(label: label, id: entryID)] }

    private init(label: String, id: String?, prepare: () -> Void) {
        self.label = label
        self.entryID = id
        _ = prepare
    }

    public init(_ label: Text, id: some Hashable, textRange: Range<String.Index>? = nil, prepare: @escaping () -> Void = {}) {
        self.init(label: _rotorLabel(_plain(label), textRange), id: _idKey(id), prepare: prepare)
    }
    public init(_ label: Text, id: some Hashable, in namespace: Namespace.ID, textRange: Range<String.Index>? = nil, prepare: @escaping () -> Void = {}) {
        self.init(label: _rotorLabel(_plain(label), textRange), id: "\(namespace.raw):\(_idKey(id))", prepare: prepare)
    }
    public init(_ label: Text? = nil, textRange: Range<String.Index>, prepare: @escaping () -> Void = {}) {
        self.init(label: _rotorLabel(label.map(_plain) ?? "", textRange), id: nil, prepare: prepare)
    }
    public init(_ labelKey: LocalizedStringKey, id: some Hashable, textRange: Range<String.Index>? = nil, prepare: @escaping () -> Void = {}) {
        self.init(label: _rotorLabel(labelKey.key, textRange), id: _idKey(id), prepare: prepare)
    }
    public init(_ labelKey: LocalizedStringKey, id: some Hashable, in namespace: Namespace.ID, textRange: Range<String.Index>? = nil, prepare: @escaping () -> Void = {}) {
        self.init(label: _rotorLabel(labelKey.key, textRange), id: "\(namespace.raw):\(_idKey(id))", prepare: prepare)
    }
    @_disfavoredOverload public init<L: StringProtocol>(_ label: L, id: some Hashable, textRange: Range<String.Index>? = nil, prepare: @escaping () -> Void = {}) {
        self.init(label: _rotorLabel(String(label), textRange), id: _idKey(id), prepare: prepare)
    }
    @_disfavoredOverload public init<L: StringProtocol>(_ label: L, id: some Hashable, in namespace: Namespace.ID, textRange: Range<String.Index>? = nil, prepare: @escaping () -> Void = {}) {
        self.init(label: _rotorLabel(String(label), textRange), id: "\(namespace.raw):\(_idKey(id))", prepare: prepare)
    }
    @_disfavoredOverload public init<L: StringProtocol>(_ label: L, _ id: some Hashable, in namespace: Namespace.ID, textRange: Range<String.Index>? = nil, prepare: @escaping () -> Void = {}) {
        self.init(label: _rotorLabel(String(label), textRange), id: "\(namespace.raw):\(_idKey(id))", prepare: prepare)
    }
    public init(_ labelKey: LocalizedStringKey, textRange: Range<String.Index>, prepare: @escaping () -> Void = {}) {
        self.init(label: _rotorLabel(labelKey.key, textRange), id: nil, prepare: prepare)
    }
    @_disfavoredOverload public init<L: StringProtocol>(_ label: L, textRange: Range<String.Index>, prepare: @escaping () -> Void = {}) {
        self.init(label: _rotorLabel(String(label), textRange), id: nil, prepare: prepare)
    }
    #if !os(WASI)
    @_disfavoredOverload public init(_ label: LocalizedStringResource, id: some Hashable, textRange: Range<String.Index>? = nil, prepare: @escaping () -> Void = {}) {
        self.init(Text(_res(label)), id: id, textRange: textRange, prepare: prepare)
    }
    @_disfavoredOverload public init(_ label: LocalizedStringResource, id: some Hashable, in namespace: Namespace.ID, textRange: Range<String.Index>? = nil, prepare: @escaping () -> Void = {}) {
        self.init(Text(_res(label)), id: id, in: namespace, textRange: textRange, prepare: prepare)
    }
    @_disfavoredOverload public init(_ label: LocalizedStringResource, textRange: Range<String.Index>, prepare: @escaping () -> Void = {}) {
        self.init(Text(_res(label)), textRange: textRange, prepare: prepare)
    }
    #endif
}

func _rotorLabel(_ text: String, _ range: Range<String.Index>?) -> String {
    guard let range else { return text }
    let shown = String(describing: range)
    return text.isEmpty ? shown : text
}

@resultBuilder
public struct AccessibilityRotorContentBuilder {
    public static func buildExpression<C: AccessibilityRotorContent>(_ c: C) -> AccessibilityRotorTuple {
        AccessibilityRotorTuple(entries: c._entries().map { ($0.label, $0.id) })
    }
    public static func buildBlock(_ parts: AccessibilityRotorTuple...) -> AccessibilityRotorTuple {
        AccessibilityRotorTuple(entries: parts.flatMap(\.entries))
    }
    public static func buildOptional(_ c: AccessibilityRotorTuple?) -> AccessibilityRotorTuple {
        c ?? AccessibilityRotorTuple(entries: [])
    }
    public static func buildIf(_ c: AccessibilityRotorTuple?) -> AccessibilityRotorTuple { buildOptional(c) }
    public static func buildEither(first: AccessibilityRotorTuple) -> AccessibilityRotorTuple { first }
    public static func buildEither(second: AccessibilityRotorTuple) -> AccessibilityRotorTuple { second }
    public static func buildLimitedAvailability(_ c: AccessibilityRotorTuple) -> AccessibilityRotorTuple { c }
}

// MARK: - Focus

@propertyWrapper
public struct AccessibilityFocusState<Value: Hashable>: _StateProperty {
    @propertyWrapper
    public struct Binding {
        let state: AccessibilityFocusState<Value>
        public var wrappedValue: Value {
            get { state.wrappedValue }
            nonmutating set { state.wrappedValue = newValue }
        }
        public var projectedValue: Self { self }
    }

    private let state: State<Value>
    private let resetValue: Value
    let technologies: AccessibilityTechnologies?

    public init() where Value == Bool { state = State(wrappedValue: false); resetValue = false; technologies = nil }
    public init<T: Hashable>() where Value == T? { state = State(wrappedValue: nil); resetValue = nil; technologies = nil }
    public init(for technologies: AccessibilityTechnologies) where Value == Bool {
        state = State(wrappedValue: false); resetValue = false; self.technologies = technologies
    }
    public init<T: Hashable>(for technologies: AccessibilityTechnologies) where Value == T? {
        state = State(wrappedValue: nil); resetValue = nil; self.technologies = technologies
    }

    func _bind(_ key: String) { state._bind(key) }
    public var wrappedValue: Value {
        get { state.wrappedValue }
        nonmutating set { state.wrappedValue = newValue }
    }
    public var projectedValue: Binding { Binding(state: self) }
    func matches(_ value: Value) -> Bool { wrappedValue == value && value != resetValue }
    func atReset() -> Bool { wrappedValue == resetValue }
}

// MARK: - Emission

struct _Ax<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content
    let make: (String) -> Node?
    func _children(_ path: String) -> [Node] {
        let made = make(path)
        return _modTargets(content, path).map { n in
            guard let made else { return n }
            var n = n
            n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [["accessibility": made]]
            return n
        }
    }
}

func _actionId(_ n: Node) -> UInt64? {
    if let u = n["action"] as? UInt64, u != 0 { return u }
    if let i = n["action"] as? Int, i > 0 { return UInt64(i) }
    return nil
}

func _nodePlain(_ n: Node) -> String {
    if let t = n["text"] as? String, !t.isEmpty { return t }
    var parts: [String] = []
    for key in ["label", "title", "content", "child"] {
        if let c = n[key] as? Node {
            let s = _nodePlain(c)
            if !s.isEmpty { parts.append(s) }
        }
    }
    if let kids = n["children"] as? [Node] {
        for c in kids {
            let s = _nodePlain(c)
            if !s.isEmpty { parts.append(s) }
        }
    }
    return parts.joined(separator: " ")
}

func _axTexts(_ n: Node) -> [String] {
    var out: [String] = []
    func walk(_ n: Node) {
        if let t = n["text"] as? String, !t.isEmpty { out.append(t) }
        for key in ["label", "title", "content", "child"] {
            if let c = n[key] as? Node { walk(c) }
        }
        if let kids = n["children"] as? [Node] { kids.forEach(walk) }
    }
    walk(n)
    return out
}

func _axButtons(_ n: Node, _ category: String?) -> [Node] {
    var out: [Node] = []
    func walk(_ n: Node) {
        if (n["kind"] as? String) == "button", let id = _actionId(n) {
            let name = _nodePlain(n)
            if !name.isEmpty {
                var a: Node = ["name": name, "action": id]
                if let category { a["category"] = category }
                out.append(a)
            }
        }
        for key in ["label", "title", "content", "child"] {
            if let c = n[key] as? Node { walk(c) }
        }
        if let kids = n["children"] as? [Node] { kids.forEach(walk) }
    }
    walk(n)
    return out
}

func _rotorJSON(_ name: String, _ rows: [(String, String?)]) -> Node {
    ["name": name, "entries": rows.map { row -> Node in
        var n: Node = ["label": row.0]
        if let id = row.1 { n["id"] = id }
        return n
    }]
}

extension View {
    func _ax(_ enabled: Bool = true, _ fields: @escaping () -> Node) -> some View {
        _Ax(content: self, make: { _ in enabled ? fields() : nil })
    }
    func _axAt(_ make: @escaping (String) -> Node?) -> some View {
        _Ax(content: self, make: make)
    }

    func _axLabel(_ text: String, _ enabled: Bool) -> some View { _ax(enabled) { ["label": text] } }
    func _axValue(_ text: String, _ enabled: Bool) -> some View { _ax(enabled) { ["value": text] } }
    func _axHint(_ text: String, _ enabled: Bool) -> some View { _ax(enabled) { ["hint": text] } }

    // MARK: Labels, values, hints

    public func accessibilityLabel(_ label: Text, isEnabled: Bool = true) -> some View { _axLabel(_plain(label), isEnabled) }
    public func accessibilityLabel(_ labelKey: LocalizedStringKey, isEnabled: Bool = true) -> some View { _axLabel(labelKey.key, isEnabled) }
    @_disfavoredOverload public func accessibilityLabel<S: StringProtocol>(_ label: S, isEnabled: Bool = true) -> some View { _axLabel(String(label), isEnabled) }
    #if !os(WASI)
    @_disfavoredOverload public func accessibilityLabel(_ label: LocalizedStringResource, isEnabled: Bool = true) -> some View { _axLabel(_res(label), isEnabled) }
    #endif
    public func accessibilityLabel<V: View>(@ViewBuilder content: @escaping (_ label: PlaceholderContentView<Self>) -> V) -> some View {
        _axAt { path in
            // Evaluating `label` when the closure includes the modified view registers its actions again.
            let node = _eval(content(PlaceholderContentView(content: self)), path + "#axlabel")
            return ["label": _nodePlain(node)]
        }
    }

    public func accessibilityValue(_ valueDescription: Text, isEnabled: Bool = true) -> some View { _axValue(_plain(valueDescription), isEnabled) }
    public func accessibilityValue(_ valueKey: LocalizedStringKey, isEnabled: Bool = true) -> some View { _axValue(valueKey.key, isEnabled) }
    @_disfavoredOverload public func accessibilityValue<S: StringProtocol>(_ value: S, isEnabled: Bool = true) -> some View { _axValue(String(value), isEnabled) }
    #if !os(WASI)
    @_disfavoredOverload public func accessibilityValue(_ value: LocalizedStringResource, isEnabled: Bool = true) -> some View { _axValue(_res(value), isEnabled) }
    #endif

    public func accessibilityHint(_ hint: Text, isEnabled: Bool = true) -> some View { _axHint(_plain(hint), isEnabled) }
    public func accessibilityHint(_ hintKey: LocalizedStringKey, isEnabled: Bool = true) -> some View { _axHint(hintKey.key, isEnabled) }
    @_disfavoredOverload public func accessibilityHint<S: StringProtocol>(_ hint: S, isEnabled: Bool = true) -> some View { _axHint(String(hint), isEnabled) }
    #if !os(WASI)
    @_disfavoredOverload public func accessibilityHint(_ hint: LocalizedStringResource, isEnabled: Bool = true) -> some View { _axHint(_res(hint), isEnabled) }
    #endif

    public func accessibilityInputLabels(_ inputLabels: [Text], isEnabled: Bool = true) -> some View {
        _ax(isEnabled) { ["inputLabels": inputLabels.map(_plain)] }
    }
    public func accessibilityInputLabels(_ inputLabelKeys: [LocalizedStringKey], isEnabled: Bool = true) -> some View {
        _ax(isEnabled) { ["inputLabels": inputLabelKeys.map(\.key)] }
    }
    @_disfavoredOverload public func accessibilityInputLabels<S: StringProtocol>(_ inputLabels: [S], isEnabled: Bool = true) -> some View {
        _ax(isEnabled) { ["inputLabels": inputLabels.map { String($0) }] }
    }

    public func accessibilityIdentifier(_ identifier: String, isEnabled: Bool = true) -> some View {
        _ax(isEnabled) { ["identifier": identifier] }
    }
    public func accessibilityHidden(_ hidden: Bool, isEnabled: Bool = true) -> some View {
        _ax(isEnabled) { ["hidden": hidden] }
    }
    public func accessibilitySortPriority(_ sortPriority: Double) -> some View { _ax { ["sortPriority": sortPriority] } }
    public func accessibilityHeading(_ level: AccessibilityHeadingLevel) -> some View { _ax { ["heading": Int(level.rawValue)] } }
    public func accessibilityAddTraits(_ traits: AccessibilityTraits) -> some View { _ax { ["addTraits": traits.names()] } }
    public func accessibilityRemoveTraits(_ traits: AccessibilityTraits) -> some View { _ax { ["removeTraits": traits.names()] } }
    public func accessibilityTextContentType(_ value: AccessibilityTextContentType) -> some View { _ax { ["textContentType": value.wire] } }
    public func accessibilityElement(children: AccessibilityChildBehavior = .ignore) -> some View { _ax { ["children": children.wire] } }

    public func accessibilityScrollStatus(_ status: Text, isEnabled: Bool = true) -> some View { _ax(isEnabled) { ["scrollStatus": _plain(status)] } }
    public func accessibilityScrollStatus(_ statusKey: LocalizedStringKey, isEnabled: Bool = true) -> some View { _ax(isEnabled) { ["scrollStatus": statusKey.key] } }
    @_disfavoredOverload public func accessibilityScrollStatus(_ status: some StringProtocol, isEnabled: Bool = true) -> some View {
        _ax(isEnabled) { ["scrollStatus": String(status)] }
    }
    #if !os(WASI)
    @_disfavoredOverload public func accessibilityScrollStatus(_ status: LocalizedStringResource, isEnabled: Bool = true) -> some View {
        _ax(isEnabled) { ["scrollStatus": _res(status)] }
    }
    #endif

    // MARK: Actions

    public func accessibilityAction(_ actionKind: AccessibilityActionKind = .default, _ handler: @escaping () -> Void) -> some View {
        let name = actionKind.name
        return _axAt { _ in
            let id = _Runtime.shared.registerAction(handler)
            return ["actions": [["name": name, "action": id] as Node]]
        }
    }
    public func accessibilityAction(named name: Text, _ handler: @escaping () -> Void) -> some View {
        accessibilityAction(.init(named: name), handler)
    }
    public func accessibilityAction(named nameKey: LocalizedStringKey, _ handler: @escaping () -> Void) -> some View {
        accessibilityAction(.init(named: Text(nameKey.key)), handler)
    }
    @_disfavoredOverload public func accessibilityAction<S: StringProtocol>(named name: S, _ handler: @escaping () -> Void) -> some View {
        accessibilityAction(.init(named: Text(String(name))), handler)
    }
    #if !os(WASI)
    @_disfavoredOverload public func accessibilityAction(named nameResource: LocalizedStringResource, _ handler: @escaping () -> Void) -> some View {
        accessibilityAction(.init(named: Text(_res(nameResource))), handler)
    }
    #endif
    public func accessibilityAction<Label: View>(action: @escaping () -> Void, @ViewBuilder label: () -> Label) -> some View {
        let name = _nodePlain(_eval(label(), "axaction"))
        return _axAt { _ in
            let id = _Runtime.shared.registerAction(action)
            return ["actions": [["name": name, "action": id] as Node]]
        }
    }
    public func accessibilityActions<Content: View>(@ViewBuilder _ content: @escaping () -> Content) -> some View {
        _axAt { path in ["actions": _axButtons(_eval(content(), path + "#axactions"), nil)] }
    }
    public func accessibilityActions<Content: View>(category: AccessibilityActionCategory, @ViewBuilder _ content: @escaping () -> Content) -> some View {
        let name = category.name
        return _axAt { path in ["actions": _axButtons(_eval(content(), path + "#axactions"), name)] }
    }
    public func accessibilityAdjustableAction(_ handler: @escaping (AccessibilityAdjustmentDirection) -> Void) -> some View {
        _axAt { _ in
            let inc = _Runtime.shared.registerAction { handler(.increment) }
            let dec = _Runtime.shared.registerAction { handler(.decrement) }
            return ["adjustable": ["increment": inc, "decrement": dec]]
        }
    }
    public func accessibilityScrollAction(_ handler: @escaping (Edge) -> Void) -> some View {
        _axAt { _ in
            func edge(_ e: Edge) -> UInt64 { _Runtime.shared.registerAction { handler(e) } }
            return ["scroll": ["top": edge(.top), "bottom": edge(.bottom), "leading": edge(.leading), "trailing": edge(.trailing)]]
        }
    }
    public func accessibilityZoomAction(_ handler: @escaping (AccessibilityZoomGestureAction) -> Void) -> some View {
        _axAt { _ in
            let zoomIn = _Runtime.shared.registerAction {
                handler(AccessibilityZoomGestureAction(direction: .zoomIn, location: .center, point: .zero))
            }
            let zoomOut = _Runtime.shared.registerAction {
                handler(AccessibilityZoomGestureAction(direction: .zoomOut, location: .center, point: .zero))
            }
            return ["zoom": ["in": zoomIn, "out": zoomOut]]
        }
    }

    // MARK: Points, traits of interaction, content

    func _axPoint(_ kind: String, _ x: CGFloat, _ y: CGFloat, _ unit: Bool, _ description: String?, _ enabled: Bool) -> some View {
        _ax(enabled) {
            var n: Node = ["x": Double(x), "y": Double(y)]
            if unit { n["unit"] = true }
            if let description { n["description"] = description }
            return [kind: n]
        }
    }
    public func accessibilityActivationPoint(_ activationPoint: CGPoint, isEnabled: Bool = true) -> some View {
        _axPoint("activation", activationPoint.x, activationPoint.y, false, nil, isEnabled)
    }
    public func accessibilityActivationPoint(_ activationPoint: UnitPoint, isEnabled: Bool = true) -> some View {
        _axPoint("activation", activationPoint.x, activationPoint.y, true, nil, isEnabled)
    }
    public func accessibilityDragPoint(_ point: UnitPoint, description: Text, isEnabled: Bool = true) -> some View {
        _axPoint("drag", point.x, point.y, true, _plain(description), isEnabled)
    }
    public func accessibilityDragPoint(_ point: UnitPoint, description: LocalizedStringKey, isEnabled: Bool = true) -> some View {
        _axPoint("drag", point.x, point.y, true, description.key, isEnabled)
    }
    @_disfavoredOverload public func accessibilityDragPoint<S: StringProtocol>(_ point: UnitPoint, description: S, isEnabled: Bool = true) -> some View {
        _axPoint("drag", point.x, point.y, true, String(description), isEnabled)
    }
    public func accessibilityDragPoint(_ point: CGPoint, description: Text, isEnabled: Bool = true) -> some View {
        _axPoint("drag", point.x, point.y, false, _plain(description), isEnabled)
    }
    public func accessibilityDragPoint(_ point: CGPoint, description: LocalizedStringKey, isEnabled: Bool = true) -> some View {
        _axPoint("drag", point.x, point.y, false, description.key, isEnabled)
    }
    @_disfavoredOverload public func accessibilityDragPoint<S: StringProtocol>(_ point: CGPoint, description: S, isEnabled: Bool = true) -> some View {
        _axPoint("drag", point.x, point.y, false, String(description), isEnabled)
    }
    public func accessibilityDropPoint(_ point: UnitPoint, description: Text, isEnabled: Bool = true) -> some View {
        _axPoint("drop", point.x, point.y, true, _plain(description), isEnabled)
    }
    public func accessibilityDropPoint(_ point: UnitPoint, description: LocalizedStringKey, isEnabled: Bool = true) -> some View {
        _axPoint("drop", point.x, point.y, true, description.key, isEnabled)
    }
    @_disfavoredOverload public func accessibilityDropPoint<S: StringProtocol>(_ point: UnitPoint, description: S, isEnabled: Bool = true) -> some View {
        _axPoint("drop", point.x, point.y, true, String(description), isEnabled)
    }
    public func accessibilityDropPoint(_ point: CGPoint, description: Text, isEnabled: Bool = true) -> some View {
        _axPoint("drop", point.x, point.y, false, _plain(description), isEnabled)
    }
    public func accessibilityDropPoint(_ point: CGPoint, description: LocalizedStringKey, isEnabled: Bool = true) -> some View {
        _axPoint("drop", point.x, point.y, false, description.key, isEnabled)
    }
    @_disfavoredOverload public func accessibilityDropPoint<S: StringProtocol>(_ point: CGPoint, description: S, isEnabled: Bool = true) -> some View {
        _axPoint("drop", point.x, point.y, false, String(description), isEnabled)
    }
    #if !os(WASI)
    @_disfavoredOverload public func accessibilityDragPoint(_ point: UnitPoint, description: LocalizedStringResource, isEnabled: Bool = true) -> some View {
        _axPoint("drag", point.x, point.y, true, _res(description), isEnabled)
    }
    @_disfavoredOverload public func accessibilityDragPoint(_ point: CGPoint, description: LocalizedStringResource, isEnabled: Bool = true) -> some View {
        _axPoint("drag", point.x, point.y, false, _res(description), isEnabled)
    }
    @_disfavoredOverload public func accessibilityDropPoint(_ point: UnitPoint, description: LocalizedStringResource, isEnabled: Bool = true) -> some View {
        _axPoint("drop", point.x, point.y, true, _res(description), isEnabled)
    }
    @_disfavoredOverload public func accessibilityDropPoint(_ point: CGPoint, description: LocalizedStringResource, isEnabled: Bool = true) -> some View {
        _axPoint("drop", point.x, point.y, false, _res(description), isEnabled)
    }
    #endif

    public func accessibilityDirectTouch(_ isDirectTouchArea: Bool = true, options: AccessibilityDirectTouchOptions = []) -> some View {
        _ax {
            var n: Node = ["directTouch": isDirectTouchArea]
            let names = options.names()
            if !names.isEmpty { n["directTouchOptions"] = names }
            return n
        }
    }
    public func accessibilityRespondsToUserInteraction(_ respondsToUserInteraction: Bool = true) -> some View {
        _ax { ["responds": respondsToUserInteraction] }
    }
    public func accessibilityRespondsToUserInteraction(_ respondsToUserInteraction: Bool, isEnabled: Bool) -> some View {
        _ax(isEnabled) { ["responds": respondsToUserInteraction] }
    }
    public func accessibilityIgnoresInvertColors(_ active: Bool = true) -> some View { _ax { ["ignoresInvertColors": active] } }

    func _axCustom(_ key: String, _ id: String?, _ value: String?, _ importance: Accessibility.AXCustomContent.Importance) -> some View {
        _ax {
            guard let value else { return ["customRemove": [id ?? key]] }
            var item: Node = ["key": key, "value": value, "importance": importance.rawValue]
            if let id { item["id"] = id }
            return ["custom": [item]]
        }
    }
    public func accessibilityCustomContent(_ key: AccessibilityCustomContentKey, _ value: Text?, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(key.key, key.id, value.map(_plain), importance)
    }
    public func accessibilityCustomContent(_ key: AccessibilityCustomContentKey, _ valueKey: LocalizedStringKey, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(key.key, key.id, valueKey.key, importance)
    }
    public func accessibilityCustomContent(_ label: Text, _ value: Text, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(_plain(label), nil, _plain(value), importance)
    }
    public func accessibilityCustomContent(_ labelKey: LocalizedStringKey, _ value: Text, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(labelKey.key, nil, _plain(value), importance)
    }
    public func accessibilityCustomContent(_ labelKey: LocalizedStringKey, _ valueKey: LocalizedStringKey, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(labelKey.key, labelKey.key, valueKey.key, importance)
    }
    @_disfavoredOverload public func accessibilityCustomContent<S: StringProtocol>(_ key: AccessibilityCustomContentKey, _ value: S, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(key.key, key.id, String(value), importance)
    }
    @_disfavoredOverload public func accessibilityCustomContent<V: StringProtocol>(_ labelKey: LocalizedStringKey, _ value: V, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(labelKey.key, labelKey.key, String(value), importance)
    }
    @_disfavoredOverload public func accessibilityCustomContent<L: StringProtocol, V: StringProtocol>(_ label: L, _ value: V, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(String(label), nil, String(value), importance)
    }
    #if !os(WASI)
    @_disfavoredOverload public func accessibilityCustomContent(_ key: AccessibilityCustomContentKey, _ value: LocalizedStringResource, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(key.key, key.id, _res(value), importance)
    }
    @_disfavoredOverload public func accessibilityCustomContent(_ label: LocalizedStringResource, _ value: Text, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(_res(label), nil, _plain(value), importance)
    }
    @_disfavoredOverload public func accessibilityCustomContent(_ label: LocalizedStringResource, _ value: LocalizedStringResource, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(_res(label), nil, _res(value), importance)
    }
    @_disfavoredOverload public func accessibilityCustomContent<V: StringProtocol>(_ label: LocalizedStringResource, _ value: V, importance: Accessibility.AXCustomContent.Importance = .default) -> some View {
        _axCustom(_res(label), nil, String(value), importance)
    }
    #endif

    public func accessibilityRepresentation<V: View>(@ViewBuilder representation: @escaping () -> V) -> some View {
        _axAt { path in
            let texts = _axTexts(_eval(representation(), path + "#axrep"))
            return ["elements": texts, "children": "ignore"]
        }
    }
    public func accessibilityChildren<V: View>(@ViewBuilder children: @escaping () -> V) -> some View {
        _axAt { path in ["elements": _axTexts(_eval(children(), path + "#axkids"))] }
    }
    public func accessibilityShowsLargeContentViewer() -> some View { _ax { ["largeContent": true] } }
    public func accessibilityShowsLargeContentViewer<V: View>(@ViewBuilder _ largeContentView: @escaping () -> V) -> some View {
        _axAt { path in
            let label = _nodePlain(_eval(largeContentView(), path + "#axlarge"))
            var n: Node = ["largeContent": true]
            if !label.isEmpty { n["largeLabel"] = label }
            return n
        }
    }
    public func accessibilityChartDescriptor<R: AXChartDescriptorRepresentable>(_ representable: R) -> some View {
        let made = representable.makeChartDescriptor()
        return _ax {
            var n: Node = ["chart": true]
            if let title = made.title ?? made.summary { n["label"] = title }
            return n
        }
    }

    public func accessibilityLinkedGroup<ID: Hashable>(id: ID, in namespace: Namespace.ID) -> some View {
        _ax { ["linked": "\(namespace.raw):\(_idKey(id))"] }
    }
    public func accessibilityLabeledPair<ID: Hashable>(role: AccessibilityLabeledPairRole, id: ID, in namespace: Namespace.ID) -> some View {
        _ax { ["pair": ["role": role.wire, "id": "\(namespace.raw):\(_idKey(id))"]] }
    }
    public func accessibilityRotorEntry<ID: Hashable>(id: ID, in namespace: Namespace.ID) -> some View {
        _ax { ["rotorEntry": "\(namespace.raw):\(_idKey(id))"] }
    }

    // MARK: Rotors

    func _axRotor(_ name: String, _ rows: [(String, String?)]) -> some View { _ax { ["rotors": [_rotorJSON(name, rows)]] } }
    func _rotorRows<C: AccessibilityRotorContent>(_ content: C) -> [(String, String?)] { content._entries().map { ($0.label, $0.id) } }
    func _axRotorBuilt<C: AccessibilityRotorContent>(_ name: String, _ entries: @escaping () -> C) -> some View {
        _ax { ["rotors": [_rotorJSON(name, _rotorRows(entries()))]] }
    }

    public func accessibilityRotor<Content: AccessibilityRotorContent>(_ label: Text, @AccessibilityRotorContentBuilder entries: @escaping () -> Content) -> some View {
        _axRotorBuilt(_plain(label), entries)
    }
    public func accessibilityRotor<Content: AccessibilityRotorContent>(_ systemRotor: AccessibilitySystemRotor, @AccessibilityRotorContentBuilder entries: @escaping () -> Content) -> some View {
        _axRotorBuilt(systemRotor.name, entries)
    }
    public func accessibilityRotor<Content: AccessibilityRotorContent>(_ labelKey: LocalizedStringKey, @AccessibilityRotorContentBuilder entries: @escaping () -> Content) -> some View {
        _axRotorBuilt(labelKey.key, entries)
    }
    @_disfavoredOverload public func accessibilityRotor<L: StringProtocol, Content: AccessibilityRotorContent>(_ label: L, @AccessibilityRotorContentBuilder entries: @escaping () -> Content) -> some View {
        _axRotorBuilt(String(label), entries)
    }

    public func accessibilityRotor<EntryModel: Identifiable>(_ rotorLabel: Text, entries: [EntryModel], entryLabel: KeyPath<EntryModel, String>) -> some View {
        _axRotor(_plain(rotorLabel), entries.map { ($0[keyPath: entryLabel], _idKey($0.id)) })
    }
    public func accessibilityRotor<EntryModel, ID: Hashable>(_ rotorLabel: Text, entries: [EntryModel], entryID: KeyPath<EntryModel, ID>, entryLabel: KeyPath<EntryModel, String>) -> some View {
        _axRotor(_plain(rotorLabel), entries.map { ($0[keyPath: entryLabel], _idKey($0[keyPath: entryID])) })
    }
    public func accessibilityRotor<EntryModel: Identifiable>(_ systemRotor: AccessibilitySystemRotor, entries: [EntryModel], entryLabel: KeyPath<EntryModel, String>) -> some View {
        _axRotor(systemRotor.name, entries.map { ($0[keyPath: entryLabel], _idKey($0.id)) })
    }
    public func accessibilityRotor<EntryModel, ID: Hashable>(_ systemRotor: AccessibilitySystemRotor, entries: [EntryModel], entryID: KeyPath<EntryModel, ID>, entryLabel: KeyPath<EntryModel, String>) -> some View {
        _axRotor(systemRotor.name, entries.map { ($0[keyPath: entryLabel], _idKey($0[keyPath: entryID])) })
    }
    public func accessibilityRotor<EntryModel: Identifiable>(_ rotorLabelKey: LocalizedStringKey, entries: [EntryModel], entryLabel: KeyPath<EntryModel, String>) -> some View {
        _axRotor(rotorLabelKey.key, entries.map { ($0[keyPath: entryLabel], _idKey($0.id)) })
    }
    @_disfavoredOverload public func accessibilityRotor<L: StringProtocol, EntryModel: Identifiable>(_ rotorLabel: L, entries: [EntryModel], entryLabel: KeyPath<EntryModel, String>) -> some View {
        _axRotor(String(rotorLabel), entries.map { ($0[keyPath: entryLabel], _idKey($0.id)) })
    }
    public func accessibilityRotor<EntryModel, ID: Hashable>(_ rotorLabelKey: LocalizedStringKey, entries: [EntryModel], entryID: KeyPath<EntryModel, ID>, entryLabel: KeyPath<EntryModel, String>) -> some View {
        _axRotor(rotorLabelKey.key, entries.map { ($0[keyPath: entryLabel], _idKey($0[keyPath: entryID])) })
    }
    @_disfavoredOverload public func accessibilityRotor<L: StringProtocol, EntryModel, ID: Hashable>(_ rotorLabel: L, entries: [EntryModel], entryID: KeyPath<EntryModel, ID>, entryLabel: KeyPath<EntryModel, String>) -> some View {
        _axRotor(String(rotorLabel), entries.map { ($0[keyPath: entryLabel], _idKey($0[keyPath: entryID])) })
    }
    public func accessibilityRotor(_ label: Text, textRanges: [Range<String.Index>]) -> some View {
        _axRotor(_plain(label), textRanges.map { (String(describing: $0), nil) })
    }
    public func accessibilityRotor(_ systemRotor: AccessibilitySystemRotor, textRanges: [Range<String.Index>]) -> some View {
        _axRotor(systemRotor.name, textRanges.map { (String(describing: $0), nil) })
    }
    public func accessibilityRotor(_ labelKey: LocalizedStringKey, textRanges: [Range<String.Index>]) -> some View {
        _axRotor(labelKey.key, textRanges.map { (String(describing: $0), nil) })
    }
    @_disfavoredOverload public func accessibilityRotor<L: StringProtocol>(_ label: L, textRanges: [Range<String.Index>]) -> some View {
        _axRotor(String(label), textRanges.map { (String(describing: $0), nil) })
    }
    #if !os(WASI)
    @_disfavoredOverload public func accessibilityRotor<Content: AccessibilityRotorContent>(_ label: LocalizedStringResource, @AccessibilityRotorContentBuilder entries: @escaping () -> Content) -> some View {
        _axRotorBuilt(_res(label), entries)
    }
    @_disfavoredOverload public func accessibilityRotor<EntryModel: Identifiable>(_ rotorLabel: LocalizedStringResource, entries: [EntryModel], entryLabel: KeyPath<EntryModel, String>) -> some View {
        _axRotor(_res(rotorLabel), entries.map { ($0[keyPath: entryLabel], _idKey($0.id)) })
    }
    @_disfavoredOverload public func accessibilityRotor<EntryModel, ID: Hashable>(_ rotorLabel: LocalizedStringResource, entries: [EntryModel], entryID: KeyPath<EntryModel, ID>, entryLabel: KeyPath<EntryModel, String>) -> some View {
        _axRotor(_res(rotorLabel), entries.map { ($0[keyPath: entryLabel], _idKey($0[keyPath: entryID])) })
    }
    @_disfavoredOverload public func accessibilityRotor(_ label: LocalizedStringResource, textRanges: [Range<String.Index>]) -> some View {
        _axRotor(_res(label), textRanges.map { (String(describing: $0), nil) })
    }
    #endif

    // MARK: Focus

    func _focusFields<Value: Hashable>(_ state: AccessibilityFocusState<Value>, _ value: Value, _ defaultFocus: Bool) -> Node {
        var n: Node = ["focused": state.matches(value)]
        if defaultFocus { n["defaultFocus"] = true }
        if let tech = state.technologies, !tech.isEmpty { n["technologies"] = tech.names() }
        return n
    }
    public func accessibilityFocused<Value: Hashable>(_ binding: AccessibilityFocusState<Value>.Binding, equals value: Value) -> some View {
        _ax { _focusFields(binding.state, value, false) }
    }
    public func accessibilityFocused(_ condition: AccessibilityFocusState<Bool>.Binding) -> some View {
        accessibilityFocused(condition, equals: true)
    }
    public func accessibilityDefaultFocus<Value: Hashable>(_ binding: AccessibilityFocusState<Value>.Binding, _ value: Value) -> some View {
        _axAt { path in
            let binding = binding
            _Runtime.shared.noteLifecycle(path + "#axDefault", {
                if binding.state.atReset() { binding.wrappedValue = value }
            }, nil)
            return _focusFields(binding.state, value, true)
        }
    }

    // MARK: Deprecated

    @available(*, deprecated, renamed: "accessibilityLabel(_:)")
    public func accessibility(label: Text) -> some View { accessibilityLabel(label) }
    @available(*, deprecated, renamed: "accessibilityValue(_:)")
    public func accessibility(value: Text) -> some View { accessibilityValue(value) }
    @available(*, deprecated, renamed: "accessibilityHint(_:)")
    public func accessibility(hint: Text) -> some View { accessibilityHint(hint) }
    @available(*, deprecated, renamed: "accessibilityInputLabels(_:)")
    public func accessibility(inputLabels: [Text]) -> some View { accessibilityInputLabels(inputLabels) }
    @available(*, deprecated, renamed: "accessibilityIdentifier(_:)")
    public func accessibility(identifier: String) -> some View { accessibilityIdentifier(identifier) }
    @available(*, deprecated, renamed: "accessibilityIdentifier(_:)")
    public func accessibility(selectionIdentifier: AnyHashable) -> some View { accessibilityIdentifier(String(describing: selectionIdentifier)) }
    @available(*, deprecated, renamed: "accessibilityHidden(_:)")
    public func accessibility(hidden: Bool) -> some View { accessibilityHidden(hidden) }
    @available(*, deprecated, renamed: "accessibilitySortPriority(_:)")
    public func accessibility(sortPriority: Double) -> some View { accessibilitySortPriority(sortPriority) }
    @available(*, deprecated, renamed: "accessibilityActivationPoint(_:)")
    public func accessibility(activationPoint: CGPoint) -> some View { accessibilityActivationPoint(activationPoint) }
    @available(*, deprecated, renamed: "accessibilityActivationPoint(_:)")
    public func accessibility(activationPoint: UnitPoint) -> some View { accessibilityActivationPoint(activationPoint) }
    @available(*, deprecated, renamed: "accessibilityAddTraits(_:)")
    public func accessibility(addTraits traits: AccessibilityTraits) -> some View { accessibilityAddTraits(traits) }
    @available(*, deprecated, renamed: "accessibilityRemoveTraits(_:)")
    public func accessibility(removeTraits traits: AccessibilityTraits) -> some View { accessibilityRemoveTraits(traits) }
}
