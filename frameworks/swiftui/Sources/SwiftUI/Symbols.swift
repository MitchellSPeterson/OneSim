import Foundation

// SF Symbols: the guest only names the symbol; the host rasterizes it (docs/protocol.md section E).

public struct Image: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let systemName: String
    var isResizable = false
    public init(systemName: String) { self.systemName = systemName }
    public func resizable() -> Image { var i = self; i.isResizable = true; return i }
    func _emit(_ path: String) -> Node { ["kind": "image", "id": path, "systemName": systemName, "resizable": isResizable] }
}

public enum ContentMode { case fit, fill }

public struct Label<Title: View, Icon: View>: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let title: Title, icon: Icon
    public init(@ViewBuilder title: () -> Title, @ViewBuilder icon: () -> Icon) { self.title = title(); self.icon = icon() }
    func _emit(_ path: String) -> Node {
        ["kind": "label", "id": path, "title": _eval(title, path + "/t"), "icon": _eval(icon, path + "/i")]
    }
}
extension Label where Title == Text, Icon == Image {
    public init(_ title: String, systemImage: String) { self.init(title: { Text(title) }, icon: { Image(systemName: systemImage) }) }
}

public enum ImageScale { case small, medium, large }

/// Sets `contentMode` on the image node it wraps (other views ignore aspect ratio).
struct _AspectView<Content: View>: View, _Flattenable {
    typealias Body = Never
    var body: Never { fatalError() }
    let content: Content
    let mode: String
    func _children(_ path: String) -> [Node] {
        var c = SwiftUI._children(content, path)
        if !c.isEmpty, c[0]["kind"] as? String == "image" { c[0]["contentMode"] = mode }
        return c
    }
}

extension View {
    public func imageScale(_ scale: ImageScale) -> some View {
        _modified { ["imageScale": scale == .small ? "small" : scale == .large ? "large" : "medium"] }
    }
    public func scaledToFit() -> some View { _AspectView(content: self, mode: "fit") }
    public func scaledToFill() -> some View { _AspectView(content: self, mode: "fill") }
    public func aspectRatio(contentMode: ContentMode) -> some View { _AspectView(content: self, mode: contentMode == .fit ? "fit" : "fill") }
}
