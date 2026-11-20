#if !os(WASI)
import Foundation
#endif

// Liquid Glass (iOS 26). Wire format: docs/protocol.md section F.

public struct Glass {
    var variant: String
    var tintColor: Color?
    var isInteractive = false
    public static let regular = Glass(variant: "regular")
    public static let clear = Glass(variant: "clear")
    public static let identity = Glass(variant: "identity")
    public func tint(_ color: Color?) -> Glass { var g = self; g.tintColor = color; return g }
    public func interactive(_ isEnabled: Bool = true) -> Glass { var g = self; g.isInteractive = isEnabled; return g }
}

public struct Ellipse: Shape, _Primitive {
    public var _shapeName: String { "ellipse" }
    public init() {}
    func _emit(_ path: String) -> Node { _shapeNode(self, path, fill: nil, stroke: nil) }
}

extension Shape where Self == Circle { public static var circle: Circle { Circle() } }
extension Shape where Self == Capsule { public static var capsule: Capsule { Capsule() } }
extension Shape where Self == Ellipse { public static var ellipse: Ellipse { Ellipse() } }
extension Shape where Self == Rectangle { public static var rect: Rectangle { Rectangle() } }
extension Shape where Self == RoundedRectangle {
    public static func rect(cornerRadius: CGFloat) -> RoundedRectangle { RoundedRectangle(cornerRadius: cornerRadius) }
}

func _glassNode<S: Shape>(_ glass: Glass, _ shape: S) -> Node {
    var sh: Node = ["shape": shape._shapeName == "roundedRectangle" ? "rect" : shape._shapeName]
    if let r = (shape as? RoundedRectangle)?.cornerRadius { sh["cornerRadius"] = Double(r) }
    var g: Node = ["variant": glass.variant, "shape": sh]
    if let t = glass.tintColor { g["tint"] = t.json }
    if glass.isInteractive { g["interactive"] = true }
    return ["glass": g]
}

extension View {
    public func glassEffect(_ glass: Glass = .regular) -> some View { glassEffect(glass, in: Capsule()) }
    public func glassEffect<S: Shape>(_ glass: Glass = .regular, in shape: S) -> some View {
        _modified { _glassNode(glass, shape) }
    }
}

/// Pass-through: merging/morphing is out of scope (protocol F).
public struct GlassEffectContainer<Content: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content
    public init(spacing: CGFloat? = nil, @ViewBuilder content: () -> Content) { self.content = content() }
    func _children(_ path: String) -> [Node] { SwiftUI._children(content, path) }
}

public struct GlassButtonStyle: PrimitiveButtonStyle {
    public init() {}
    public var _styleName: String? { "glass" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
public struct GlassProminentButtonStyle: PrimitiveButtonStyle {
    public init() {}
    public var _styleName: String? { "glassProminent" }
    public func makeBody(configuration: Configuration) -> Never { fatalError() }
}
extension PrimitiveButtonStyle where Self == GlassButtonStyle { public static var glass: GlassButtonStyle { .init() } }
extension PrimitiveButtonStyle where Self == GlassProminentButtonStyle { public static var glassProminent: GlassProminentButtonStyle { .init() } }
