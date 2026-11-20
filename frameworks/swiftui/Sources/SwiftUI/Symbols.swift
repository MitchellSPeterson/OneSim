#if !os(WASI)
import Foundation
#endif

// SF Symbols: the guest only names the symbol; the host rasterizes it (docs/protocol.md section E).

public struct Image: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let systemName: String
    var isResizable = false
    var _bitmap: Node?
    var _renderingMode: String?
    var _symbolRenderingMode: String?
    var _interpolation: String?
    var _antialiased: Bool?
    var _symbolColorRenderingMode: String?
    var _symbolVariableValueMode: String?
    var _dynamicRange: String?
    public init(systemName: String) { self.systemName = systemName }
    public func resizable() -> Image { var i = self; i.isResizable = true; return i }
    func _emit(_ path: String) -> Node {
        var n: Node = ["kind": "image", "id": path, "resizable": isResizable]
        if !systemName.isEmpty { n["systemName"] = systemName }
        if let b = _bitmap { n["bitmap"] = b }
        if let v = _renderingMode { n["renderingMode"] = v }
        if let v = _symbolRenderingMode { n["symbolRenderingMode"] = v }
        if let v = _interpolation { n["interpolation"] = v }
        if let v = _antialiased { n["antialiased"] = v }
        if let v = _symbolColorRenderingMode { n["symbolColorRenderingMode"] = v }
        if let v = _symbolVariableValueMode { n["symbolVariableValueMode"] = v }
        if let v = _dynamicRange { n["dynamicRange"] = v }
        return n
    }
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

extension Image {
    public enum Interpolation {
        case none, low, medium, high
        var _wire: String {
            switch self {
            case .none: return "none"
            case .low: return "low"
            case .medium: return "medium"
            case .high: return "high"
            }
        }
    }
    public enum TemplateRenderingMode {
        case template, original
        var _wire: String {
            switch self {
            case .template: return "template"
            case .original: return "original"
            }
        }
    }
    public struct DynamicRange: Equatable {
        let _wire: String
        public static let standard = DynamicRange(_wire: "standard")
        public static let constrainedHigh = DynamicRange(_wire: "constrainedHigh")
        public static let high = DynamicRange(_wire: "high")
        public static func == (a: DynamicRange, b: DynamicRange) -> Bool { a._wire == b._wire }
    }
}

public struct SymbolRenderingMode: Equatable {
    let _wire: String
    public static let monochrome = SymbolRenderingMode(_wire: "monochrome")
    public static let hierarchical = SymbolRenderingMode(_wire: "hierarchical")
    public static let palette = SymbolRenderingMode(_wire: "palette")
    public static let multicolor = SymbolRenderingMode(_wire: "multicolor")
    public static func == (a: SymbolRenderingMode, b: SymbolRenderingMode) -> Bool { a._wire == b._wire }
}

public struct SymbolVariableValueMode: Equatable {
    let _wire: String
    public static let color = SymbolVariableValueMode(_wire: "color")
    public static let draw = SymbolVariableValueMode(_wire: "draw")
    public static func == (a: SymbolVariableValueMode, b: SymbolVariableValueMode) -> Bool { a._wire == b._wire }
}

public struct SymbolColorRenderingMode: Equatable {
    let _wire: String
    public static let flat = SymbolColorRenderingMode(_wire: "flat")
    public static let gradient = SymbolColorRenderingMode(_wire: "gradient")
    public static func == (a: SymbolColorRenderingMode, b: SymbolColorRenderingMode) -> Bool { a._wire == b._wire }
}

extension Image {
    /// Nil clears the stored mode (the key is omitted).
    public func renderingMode(_ renderingMode: TemplateRenderingMode?) -> Image {
        var i = self; i._renderingMode = renderingMode?._wire; return i
    }
    public func symbolRenderingMode(_ mode: SymbolRenderingMode?) -> Image {
        var i = self; i._symbolRenderingMode = mode?._wire; return i
    }
    public func interpolation(_ interpolation: Interpolation) -> Image {
        var i = self; i._interpolation = interpolation._wire; return i
    }
    public func antialiased(_ isAntialiased: Bool) -> Image {
        var i = self; i._antialiased = isAntialiased; return i
    }
    public func symbolColorRenderingMode(_ mode: SymbolColorRenderingMode?) -> Image {
        var i = self; i._symbolColorRenderingMode = mode?._wire; return i
    }
    public func symbolVariableValueMode(_ mode: SymbolVariableValueMode?) -> Image {
        var i = self; i._symbolVariableValueMode = mode?._wire; return i
    }
    public func allowedDynamicRange(_ range: DynamicRange?) -> Image {
        var i = self; i._dynamicRange = range?._wire; return i
    }
    static func _bitmap(path: String, pixels: (Int, Int), scale: CGFloat) -> Image {
        var i = Image(systemName: "")
        let s = scale == 0 ? 1.0 : Double(scale)
        i._bitmap = ["path": path, "width": pixels.0 > 0 ? Double(pixels.0) / s : 0.0,
                     "height": pixels.1 > 0 ? Double(pixels.1) / s : 0.0, "scale": s]
        return i
    }
}

extension View {
    public func symbolRenderingMode(_ mode: SymbolRenderingMode?) -> some View {
        _modified { ["symbolRenderingMode": _orNull(mode?._wire)] }
    }
    public func symbolColorRenderingMode(_ mode: SymbolColorRenderingMode?) -> some View {
        _modified { ["symbolColorRenderingMode": _orNull(mode?._wire)] }
    }
    public func symbolVariableValueMode(_ mode: SymbolVariableValueMode?) -> some View {
        _modified { ["symbolVariableValueMode": _orNull(mode?._wire)] }
    }
    public func allowedDynamicRange(_ range: Image.DynamicRange?) -> some View {
        _modified { ["allowedDynamicRange": _orNull(range?._wire)] }
    }
}

#if !os(WASI)
struct _AsyncLoadError: Error {}

enum _ImgState { case loading, failed, ready(String, Int, Int) }

enum _ImgCache { static var items: [String: _ImgState] = [:] }

/// PNG or JPEG pixel size. Nil if the bytes are neither.
func _pixelSize(_ data: Data) -> (Int, Int)? {
    let b = [UInt8](data)
    if b.count >= 24, b.starts(with: [0x89, 0x50, 0x4E, 0x47]) {
        let w = (Int(b[16]) << 24) | (Int(b[17]) << 16) | (Int(b[18]) << 8) | Int(b[19])
        let h = (Int(b[20]) << 24) | (Int(b[21]) << 16) | (Int(b[22]) << 8) | Int(b[23])
        if w > 0, h > 0 { return (w, h) }
    }
    if b.count > 4, b[0] == 0xFF, b[1] == 0xD8 {
        var i = 2
        while i + 1 < b.count {
            if b[i] != 0xFF { return nil }
            i += 1
            while i < b.count, b[i] == 0xFF { i += 1 }
            guard i < b.count else { return nil }
            let marker = b[i]
            i += 1
            if marker == 0xD9 || marker == 0xDA { return nil }
            if marker == 0x01 || (marker >= 0xD0 && marker <= 0xD8) { continue }
            guard i + 1 < b.count else { return nil }
            let len = (Int(b[i]) << 8) | Int(b[i + 1])
            if len < 2 || i + len > b.count { return nil }
            if marker == 0xC0 || marker == 0xC1 || marker == 0xC2 || marker == 0xC3 {
                guard len >= 7 else { return nil }
                let h = (Int(b[i + 3]) << 8) | Int(b[i + 4])
                let w = (Int(b[i + 5]) << 8) | Int(b[i + 6])
                return w > 0 && h > 0 ? (w, h) : nil
            }
            i += len
        }
    }
    return nil
}

func _persistImage(_ key: String, _ data: Data) -> String? {
    var h: UInt64 = 1469598103934665603
    for byte in key.utf8 { h ^= UInt64(byte); h &*= 1099511628211 }
    let url = FileManager.default.temporaryDirectory.appendingPathComponent("onesim-async-\(h).img")
    do { try data.write(to: url, options: .atomic); return url.path } catch { return nil }
}

func _phase(_ state: _ImgState, scale: CGFloat) -> AsyncImagePhase {
    switch state {
    case .loading: return .empty
    case .failed: return .failure(_AsyncLoadError())
    case .ready(let path, let w, let h): return .success(Image._bitmap(path: path, pixels: (w, h), scale: scale))
    }
}

/// File URLs load during this evaluation so the first tree contains the bitmap. HTTP completes on the main queue.
func _asyncPhase(url: URL?, scale: CGFloat) -> AsyncImagePhase {
    guard let url else { return .empty }
    let key = url.absoluteString
    if let hit = _ImgCache.items[key] { return _phase(hit, scale: scale) }
    if url.isFileURL {
        let state: _ImgState
        if let data = try? Data(contentsOf: url), !data.isEmpty {
            let px = _pixelSize(data) ?? (0, 0)
            state = .ready(url.path, px.0, px.1)
        } else { state = .failed }
        _ImgCache.items[key] = state
        return _phase(state, scale: scale)
    }
    _ImgCache.items[key] = .loading
    let session = _Runtime.shared.imageSession ?? .shared
    session.dataTask(with: url) { data, response, error in
        let statusOK: Bool
        if let http = response as? HTTPURLResponse { statusOK = (200..<300).contains(http.statusCode) }
        else { statusOK = error == nil }
        let state: _ImgState
        if statusOK, error == nil, let data = data, !data.isEmpty, let path = _persistImage(key, data) {
            let px = _pixelSize(data) ?? (0, 0)
            state = .ready(path, px.0, px.1)
        } else { state = .failed }
        DispatchQueue.main.async {
            _ImgCache.items[key] = state
            _Runtime.shared.submitTree()
        }
    }.resume()
    return .empty
}

public enum AsyncImagePhase {
    case empty
    case success(Image)
    case failure(Error)
    public var image: Image? { if case .success(let image) = self { return image }; return nil }
    public var error: Error? { if case .failure(let error) = self { return error }; return nil }
}

public struct AsyncImage<Content: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let url: URL?
    let scale: CGFloat
    let content: ((AsyncImagePhase) -> Content)?
    init(url: URL?, scale: CGFloat, content: ((AsyncImagePhase) -> Content)?) {
        self.url = url; self.scale = scale; self.content = content
    }
    public init(url: URL?, scale: CGFloat = 1, transaction: Transaction = Transaction(), @ViewBuilder content: @escaping (AsyncImagePhase) -> Content) {
        _ = transaction
        self.init(url: url, scale: scale, content: content)
    }
    func _children(_ path: String) -> [Node] {
        let phase = _asyncPhase(url: url, scale: scale)
        if let content = content { return SwiftUI._children(content(phase), path) }
        if case .success(let image) = phase { return SwiftUI._children(image, path) }
        return [["kind": "empty", "id": path] as Node]
    }
}

extension AsyncImage where Content == Image {
    public init(url: URL?, scale: CGFloat = 1) { self.init(url: url, scale: scale, content: nil) }
}

extension AsyncImage {
    public init<I: View, P: View>(url: URL?, scale: CGFloat = 1, @ViewBuilder content: @escaping (Image) -> I, @ViewBuilder placeholder: @escaping () -> P) where Content == _ConditionalContent<I, P> {
        self.init(url: url, scale: scale) { phase in
            if let image = phase.image { return _ConditionalContent(.first(content(image))) }
            return _ConditionalContent(.second(placeholder()))
        }
    }
}

extension View {
    public func asyncImageURLSession(_ session: URLSession) -> some View {
        _CtxView(content: self) { $0.imageSession = session }
    }
}
#endif
