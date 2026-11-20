#if !os(WASI)
import Foundation
#endif

public struct BackgroundStyle: ShapeStyle, Sendable {
    public init() {}
    public var _json: [String: Any] { ["style": "background"] }
}

extension ShapeStyle where Self == BackgroundStyle {
    public static var background: BackgroundStyle { .init() }
}

public struct TintShapeStyle: ShapeStyle, Sendable {
    public init() {}
    public var _json: [String: Any] { ["style": "tint"] }
}

extension ShapeStyle where Self == TintShapeStyle {
    public static var tint: TintShapeStyle { .init() }
}

public struct FillShapeStyle: ShapeStyle, Sendable {
    public init() {}
    public var _json: [String: Any] { ["style": "fill"] }
}

extension ShapeStyle where Self == FillShapeStyle {
    public static var fill: FillShapeStyle { .init() }
}

public struct LinkShapeStyle: ShapeStyle, Sendable {
    public init() {}
    public var _json: [String: Any] { ["style": "link"] }
}

extension ShapeStyle where Self == LinkShapeStyle {
    public static var link: LinkShapeStyle { .init() }
}

public struct PlaceholderTextShapeStyle: ShapeStyle, Sendable {
    public init() {}
    public var _json: [String: Any] { ["style": "placeholder"] }
}

extension ShapeStyle where Self == PlaceholderTextShapeStyle {
    public static var placeholder: PlaceholderTextShapeStyle { .init() }
}

public struct SelectionShapeStyle: ShapeStyle, Sendable {
    public init() {}
    public var _json: [String: Any] { ["style": "selection"] }
}

extension ShapeStyle where Self == SelectionShapeStyle {
    public static var selection: SelectionShapeStyle { .init() }
}

public struct SeparatorShapeStyle: ShapeStyle, Sendable {
    public init() {}
    public var _json: [String: Any] { ["style": "separator"] }
}

extension ShapeStyle where Self == SeparatorShapeStyle {
    public static var separator: SeparatorShapeStyle { .init() }
}

public struct WindowBackgroundShapeStyle: ShapeStyle, Sendable {
    public init() {}
    public var _json: [String: Any] { ["style": "windowBackground"] }
}

extension ShapeStyle where Self == WindowBackgroundShapeStyle {
    public static var windowBackground: WindowBackgroundShapeStyle { .init() }
}

extension ShapeStyle where Self == HierarchicalShapeStyle {
    public static var quaternary: HierarchicalShapeStyle { .init(name: "quaternary") }
    public static var quinary: HierarchicalShapeStyle { .init(name: "quinary") }
}

extension ShapeStyle {
    public var secondary: some ShapeStyle { _LevelShapeStyle(base: self, level: "secondary") }
    public var tertiary: some ShapeStyle { _LevelShapeStyle(base: self, level: "tertiary") }
    public var quaternary: some ShapeStyle { _LevelShapeStyle(base: self, level: "quaternary") }
    public var quinary: some ShapeStyle { _LevelShapeStyle(base: self, level: "quinary") }
}

private struct _LevelShapeStyle<Base: ShapeStyle>: ShapeStyle {
    let base: Base
    let level: String
    var _json: [String: Any] {
        var paint = base._json
        if paint["style"] as? String == "fill" {
            return ["style": level + "Fill"]
        }
        if let alpha = paint["a"] as? Double {
            let levels = ["secondary": 0.6, "tertiary": 0.3, "quaternary": 0.18, "quinary": 0.12]
            paint["a"] = alpha * (levels[level] ?? 1)
            return paint
        }
        return ["style": level]
    }
}

func _foregroundPaint(_ paint: Node) -> (String, Node) {
    if paint["r"] != nil || paint["semantic"] != nil { return ("foregroundColor", paint) }
    return ("foregroundPaint", paint.isEmpty ? ["style": "foreground"] : paint)
}

private enum _TintKey: EnvironmentKey { static var defaultValue: Color? { nil } }

extension EnvironmentValues {
    public var tint: Color? {
        get { self[_TintKey.self] }
        set { self[_TintKey.self] = newValue }
    }
}

extension View {
    private func _foregroundPalette(_ primary: Node, _ secondary: Node, _ tertiary: Node?) -> some View {
        _modified {
            func inherited(_ paint: Node) -> Node { paint.isEmpty ? ["style": "foreground"] : paint }
            var palette: Node = ["primary": inherited(primary), "secondary": inherited(secondary)]
            if let tertiary { palette["tertiary"] = inherited(tertiary) }
            return ["foregroundPalette": palette]
        }
    }

    public func foregroundStyle<S1: ShapeStyle, S2: ShapeStyle>(_ primary: S1, _ secondary: S2) -> some View {
        _foregroundPalette(primary._json, secondary._json, nil)
    }

    public func foregroundStyle<S1: ShapeStyle, S2: ShapeStyle, S3: ShapeStyle>(
        _ primary: S1, _ secondary: S2, _ tertiary: S3
    ) -> some View {
        _foregroundPalette(primary._json, secondary._json, tertiary._json)
    }

    public func backgroundStyle<S: ShapeStyle>(_ style: S) -> some View {
        _modified {
            let paint: Node = style._json.isEmpty ? ["style": "foreground"] : style._json
            return ["backgroundPaint": paint]
        }
    }

    public func tint(_ tint: Color?) -> some View {
        _modified { ["tint": tint?.json as Any? ?? NSNull()] }.environment(\.tint, tint)
    }

    public func tint<S: ShapeStyle>(_ tint: S) -> some View {
        _modified {
            let paint: Node = tint._json.isEmpty ? ["style": "foreground"] : tint._json
            return ["tint": paint]
        }
    }

    @_disfavoredOverload
    public func background<S: ShapeStyle>(_ style: S, ignoresSafeAreaEdges edges: Edge.Set = .all) -> some View {
        background(Rectangle().fill(style), alignment: .center)
    }

    public func background(ignoresSafeAreaEdges edges: Edge.Set = .all) -> some View {
        background(BackgroundStyle(), ignoresSafeAreaEdges: edges)
    }

    public func background<S: ShapeStyle, T: Shape>(_ style: S, in shape: T, fillStyle: FillStyle = FillStyle()) -> some View {
        background(shape.fill(style, style: fillStyle), alignment: .center)
    }

    public func background<S: Shape>(in shape: S, fillStyle: FillStyle = FillStyle()) -> some View {
        background(BackgroundStyle(), in: shape, fillStyle: fillStyle)
    }

    public func background<V: View>(alignment: Alignment = .center, @ViewBuilder content: () -> V) -> some View {
        background(content(), alignment: alignment)
    }

    @_disfavoredOverload
    public func overlay<S: ShapeStyle>(_ style: S, ignoresSafeAreaEdges edges: Edge.Set = .all) -> some View {
        overlay(Rectangle().fill(style), alignment: .center)
    }

    public func overlay<S: ShapeStyle, T: Shape>(_ style: S, in shape: T, fillStyle: FillStyle = FillStyle()) -> some View {
        overlay(shape.fill(style, style: fillStyle), alignment: .center)
    }

    public func overlay<S: Shape>(in shape: S, fillStyle: FillStyle = FillStyle()) -> some View {
        overlay(ForegroundStyle(), in: shape, fillStyle: fillStyle)
    }
}
