#if !os(WASI)
import Foundation
#endif

/// A tick at a slider value, optionally labelled.
public struct SliderTick<V: BinaryFloatingPoint>: SliderTickContent, Identifiable, Comparable {
    public typealias Value = V

    public struct ID: Hashable {
        fileprivate let value: V
    }

    let value: V
    let label: _StyleLabel?
    public var id: ID { ID(value: value) }
    public var body: [Self] { [self] }

    public init(_ value: V) {
        self.value = value
        label = nil
    }

    public init(_ value: V, @ViewBuilder label: () -> some View) {
        self.value = value
        self.label = _styleLabel(label())
    }

    public init(_ titleKey: LocalizedStringKey, _ value: V) { self.init(value) { Text(titleKey.key) } }
    @_disfavoredOverload public init<S: StringProtocol>(_ title: S, _ value: V) { self.init(value) { Text(String(title)) } }
    #if !os(WASI)
    @_disfavoredOverload public init(_ title: LocalizedStringResource, _ value: V) { self.init(value) { Text(_res(title)) } }
    #endif

    public static func < (lhs: Self, rhs: Self) -> Bool { lhs.value < rhs.value }
    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.value == rhs.value }
}

public protocol SliderTickContent<Value> {
    associatedtype Value: BinaryFloatingPoint
    associatedtype Body: RandomAccessCollection where Body.Element == SliderTick<Value>
    var body: Body { get }
}

private struct _SliderTicks<V: BinaryFloatingPoint>: SliderTickContent {
    typealias Value = V
    let body: [SliderTick<V>]
}

extension Optional: SliderTickContent where Wrapped: SliderTickContent {
    public typealias Value = Wrapped.Value
    public var body: [SliderTick<Value>] { map { Array($0.body) } ?? [] }
}

public struct TupleSliderTickContent<V: BinaryFloatingPoint, T>: SliderTickContent {
    public typealias Value = V
    public var value: T
    let collect: (T) -> [SliderTick<V>]
    public var body: [SliderTick<V>] { collect(value) }
}

@resultBuilder public struct SliderTickBuilder<V: BinaryFloatingPoint> {
    public static func buildBlock() -> some SliderTickContent<V> { _SliderTicks<V>(body: []) }
    public static func buildBlock<C: SliderTickContent<V>>(_ content: C) -> C { content }
    public static func buildExpression<C: SliderTickContent<V>>(_ content: C) -> C { content }
    public static func buildOptional<C: SliderTickContent<V>>(_ content: C?) -> C? { content }
    public static func buildEither<T: SliderTickContent<V>, F: SliderTickContent<V>>(first: T) -> _ConditionalContent<T, F> { .init(.first(first)) }
    public static func buildEither<T: SliderTickContent<V>, F: SliderTickContent<V>>(second: F) -> _ConditionalContent<T, F> { .init(.second(second)) }
    public static func buildArray<C: SliderTickContent<V>>(_ content: [C]) -> some SliderTickContent<V> { _SliderTicks<V>(body: content.flatMap(\.body)) }
    public static func buildPartialBlock<C: SliderTickContent<V>>(first: C) -> C { first }

    public static func buildPartialBlock<A: SliderTickContent<V>, N: SliderTickContent<V>>(
        accumulated: A, next: N
    ) -> TupleSliderTickContent<V, (A, N)> {
        .init(value: (accumulated, next)) { Array($0.0.body) + $0.1.body }
    }
}

extension _ConditionalContent: SliderTickContent where T: SliderTickContent, F: SliderTickContent, T.Value == F.Value {
    public typealias Value = T.Value
    public var body: [SliderTick<Value>] {
        switch storage {
        case .first(let content): return Array(content.body)
        case .second(let content): return Array(content.body)
        }
    }
}

public struct SliderTickContentForEach<Data: RandomAccessCollection, ID: Hashable, Content: SliderTickContent>: SliderTickContent {
    public typealias Value = Content.Value
    let data: Data
    let content: (Data.Element) -> Content
    public var body: [SliderTick<Content.Value>] { data.flatMap { content($0).body } }

    public init<V>(_ data: Data, id: KeyPath<Data.Element, ID>, @SliderTickBuilder<V> content: @escaping (Data.Element) -> Content)
    where V == Data.Element, V == Content.Value {
        self.data = data
        self.content = content
    }

    public init<V>(_ data: Data, @SliderTickBuilder<V> content: @escaping (Data.Element) -> Content)
    where V: Identifiable, V == Data.Element, V == Content.Value, V.ID == ID {
        self.init(data, id: \.id, content: content)
    }

    public init<V>(_ data: Range<Int>, @SliderTickBuilder<V> content: @escaping (Int) -> Content)
    where Data == Range<Int>, V == Content.Value {
        self.data = data
        self.content = content
    }
}

public struct Slider<Label: View, ValueLabel: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }

    let get: () -> Double, set: (Double) -> Void
    let lo: Double, hi: Double, step: Double?
    let label: Label, editing: (Bool) -> Void
    var minLabel: ValueLabel?, maxLabel: ValueLabel?, currentLabel: _StyleLabel?
    var enabledBounds: ClosedRange<Double>?, neutralValue: Double?
    var ticks: [(value: Double, label: _StyleLabel?)] = []
    var tickAt: ((Double) -> _StyleLabel?)?
    var modern = false

    init<V: BinaryFloatingPoint>(_ value: Binding<V>, _ bounds: ClosedRange<V>, _ step: Double?, _ label: Label,
                                _ editing: @escaping (Bool) -> Void) {
        get = { Double(value.wrappedValue) }
        set = { value.wrappedValue = V($0) }
        lo = min(Double(bounds.lowerBound), .greatestFiniteMagnitude)
        hi = max(Double(bounds.upperBound), -.greatestFiniteMagnitude)
        self.step = step.flatMap { $0.isFinite && $0 > 0 ? $0 : nil }
        self.label = label
        self.editing = editing
    }

    private func selectedValue(_ value: Double) -> Double {
        let bounds = enabledBounds ?? lo...hi
        if value <= bounds.lowerBound { return bounds.lowerBound }
        if value >= bounds.upperBound { return bounds.upperBound }
        guard let step else { return value }
        let units = (value - lo) / step
        guard units.isFinite else { return value }
        return min(bounds.upperBound, max(bounds.lowerBound, lo + units.rounded() * step))
    }

    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared
        let value = get()
        let displayed = value.isNaN ? lo : min(hi, max(lo, value))
        var n: Node = ["kind": "slider", "id": path, "value": displayed,
                       "min": lo, "max": hi, "disabled": rt.disabled, "editing": true]
        if modern { n["modern"] = true }
        if let step { n["step"] = step }
        if let neutralValue { n["neutralValue"] = neutralValue }
        if let enabledBounds { n["enabledBounds"] = [enabledBounds.lowerBound, enabledBounds.upperBound] }
        if rt.layoutDirection == .rightToLeft { n["rtl"] = true }
        if let node = _optionalNode(label, path + "/label") { n["label"] = _plainText(node) }
        let selectedTick = tickAt?(displayed) ?? ticks.first(where: { $0.value == displayed })?.label
        if let node = _optionalNode(selectedTick, path + "/currentTick") ?? _optionalNode(currentLabel, path + "/current") {
            n["currentValueLabel"] = node
        }
        if !ticks.isEmpty {
            n["ticks"] = ticks.enumerated().map { i, tick -> Node in
                var mark: Node = ["value": tick.value]
                if let label = _optionalNode(tick.label, path + "/tick/\(i)") { mark["label"] = label }
                return mark
            }
        }
        if !rt.disabled {
            rt.noteLifecycle(path + "#slider", nil) { rt.sliderEdits.removeValue(forKey: path)?(false) }
            n["field"] = rt.registerField { input in
                if input == "cancel" {
                    rt.sliderEdits.removeValue(forKey: path)?(false)
                    return
                }
                let parts = input.split(separator: ":", maxSplits: 1)
                guard let text = parts.last, let value = Double(text), value.isFinite else { return }
                let previous = get(), next = selectedValue(value)
                if parts.count == 1 {
                    editing(true)
                    if next != previous { set(next) }
                    editing(false)
                    return
                }
                switch parts[0] {
                case "start":
                    if rt.sliderEdits[path] == nil {
                        rt.sliderEdits[path] = editing
                        editing(true)
                    }
                case "move", "end":
                    guard rt.sliderEdits[path] != nil else { return }
                default: return
                }
                if next != previous { set(next) }
                if parts[0] == "end" { rt.sliderEdits.removeValue(forKey: path)?(false) }
            }
        }
        let minNode = _optionalNode(minLabel, path + "/min"), maxNode = _optionalNode(maxLabel, path + "/max")
        if minLabel != nil || maxLabel != nil { n["id"] = path + "/s" }
        if minNode == nil && maxNode == nil { return n }
        let labels = rt.layoutDirection == .rightToLeft ? [maxNode, n, minNode] : [minNode, n, maxNode]
        return ["kind": "hstack", "id": path, "spacing": 11.0, "alignment": "center", "children": labels.compactMap { $0 }]
    }
}

extension Slider where ValueLabel == EmptyView {
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V> = 0...1, @ViewBuilder label: () -> Label,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, nil, label(), onEditingChanged)
    }
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride, @ViewBuilder label: () -> Label,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, Double(step), label(), onEditingChanged)
    }
}
extension Slider {
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V> = 0...1, @ViewBuilder label: () -> Label,
                                        @ViewBuilder minimumValueLabel: () -> ValueLabel, @ViewBuilder maximumValueLabel: () -> ValueLabel,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, nil, label(), onEditingChanged)
        minLabel = minimumValueLabel(); maxLabel = maximumValueLabel()
    }
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride, @ViewBuilder label: () -> Label,
                                        @ViewBuilder minimumValueLabel: () -> ValueLabel, @ViewBuilder maximumValueLabel: () -> ValueLabel,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, Double(step), label(), onEditingChanged)
        minLabel = minimumValueLabel(); maxLabel = maximumValueLabel()
    }
}
extension Slider where Label == EmptyView, ValueLabel == EmptyView {
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V> = 0...1,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, nil, EmptyView(), onEditingChanged)
    }
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in }) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, Double(step), EmptyView(), onEditingChanged)
    }
}

extension Slider {
    public init<V: BinaryFloatingPoint>(
        value: Binding<V>, in bounds: ClosedRange<V> = 0...1, neutralValue: V? = nil,
        enabledBounds: ClosedRange<V>? = nil, @ViewBuilder label: () -> Label,
        @ViewBuilder currentValueLabel: () -> some View = { EmptyView() },
        @ViewBuilder minimumValueLabel: () -> ValueLabel = { EmptyView() },
        @ViewBuilder maximumValueLabel: () -> ValueLabel = { EmptyView() },
        onEditingChanged: @escaping (Bool) -> Void = { _ in }
    ) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, nil, label(), onEditingChanged)
        configure(neutralValue, enabledBounds, currentValueLabel(), minimumValueLabel(), maximumValueLabel())
    }

    public init<V: BinaryFloatingPoint>(
        value: Binding<V>, in bounds: ClosedRange<V> = 0...1, neutralValue: V? = nil,
        enabledBounds: ClosedRange<V>? = nil, @ViewBuilder label: () -> Label,
        @ViewBuilder currentValueLabel: () -> some View = { EmptyView() },
        @ViewBuilder minimumValueLabel: () -> ValueLabel = { EmptyView() },
        @ViewBuilder maximumValueLabel: () -> ValueLabel = { EmptyView() },
        @SliderTickBuilder<V> ticks: () -> some SliderTickContent<V>,
        onEditingChanged: @escaping (Bool) -> Void = { _ in }
    ) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, nil, label(), onEditingChanged)
        configure(neutralValue, enabledBounds, currentValueLabel(), minimumValueLabel(), maximumValueLabel())
        self.ticks = ticks().body.compactMap { mark($0) }
    }

    public init<V: BinaryFloatingPoint>(
        value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride = 1, neutralValue: V? = nil,
        enabledBounds: ClosedRange<V>? = nil, @ViewBuilder label: () -> Label,
        @ViewBuilder currentValueLabel: () -> some View = { EmptyView() },
        @ViewBuilder minimumValueLabel: () -> ValueLabel = { EmptyView() },
        @ViewBuilder maximumValueLabel: () -> ValueLabel = { EmptyView() },
        tick: @escaping (V) -> SliderTick<V>? = { _ in nil },
        onEditingChanged: @escaping (Bool) -> Void = { _ in }
    ) where V.Stride: BinaryFloatingPoint {
        self.init(value, bounds, Double(step), label(), onEditingChanged)
        configure(neutralValue, enabledBounds, currentValueLabel(), minimumValueLabel(), maximumValueLabel())
        // Only the current mark can supply a label. Keep dense ranges cheap to evaluate.
        tickAt = { tick(V($0))?.label }
    }

    private mutating func configure<V: BinaryFloatingPoint>(_ neutral: V?, _ enabled: ClosedRange<V>?,
                                                            _ current: some View, _ minLabel: ValueLabel, _ maxLabel: ValueLabel) {
        modern = true
        neutralValue = neutral.flatMap { $0.isNaN ? nil : min(hi, max(lo, Double($0))) }
        enabledBounds = enabled.map {
            min(hi, max(lo, Double($0.lowerBound)))...min(hi, max(lo, Double($0.upperBound)))
        }
        currentLabel = _styleLabel(current)
        self.minLabel = minLabel
        self.maxLabel = maxLabel
    }

    private func mark<V: BinaryFloatingPoint>(_ tick: SliderTick<V>) -> (Double, _StyleLabel?)? {
        let value = Double(tick.value)
        return value.isFinite && lo...hi ~= value ? (value, tick.label) : nil
    }
}

extension Slider {
    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V> = 0...1,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in },
                                        minimumValueLabel: ValueLabel, maximumValueLabel: ValueLabel,
                                        @ViewBuilder label: () -> Label) where V.Stride: BinaryFloatingPoint {
        self.init(value: value, in: bounds, label: label, minimumValueLabel: { minimumValueLabel },
                  maximumValueLabel: { maximumValueLabel }, onEditingChanged: onEditingChanged)
    }

    public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride = 1,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in },
                                        minimumValueLabel: ValueLabel, maximumValueLabel: ValueLabel,
                                        @ViewBuilder label: () -> Label) where V.Stride: BinaryFloatingPoint {
        self.init(value: value, in: bounds, step: step, label: label, minimumValueLabel: { minimumValueLabel },
                  maximumValueLabel: { maximumValueLabel }, onEditingChanged: onEditingChanged)
    }
}

extension Slider where ValueLabel == EmptyView {
    @_disfavoredOverload public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V> = 0...1,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in },
                                        @ViewBuilder label: () -> Label) where V.Stride: BinaryFloatingPoint {
        self.init(value: value, in: bounds, label: label, onEditingChanged: onEditingChanged)
    }

    @_disfavoredOverload public init<V: BinaryFloatingPoint>(value: Binding<V>, in bounds: ClosedRange<V>, step: V.Stride = 1,
                                        onEditingChanged: @escaping (Bool) -> Void = { _ in },
                                        @ViewBuilder label: () -> Label) where V.Stride: BinaryFloatingPoint {
        self.init(value: value, in: bounds, step: step, label: label, onEditingChanged: onEditingChanged)
    }
}
