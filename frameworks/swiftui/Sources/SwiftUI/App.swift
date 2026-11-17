public protocol Scene {
    associatedtype Body: Scene
    @SceneBuilder var body: Body { get }
}
extension Never: Scene {}

@resultBuilder
public struct SceneBuilder {
    public static func buildBlock<C: Scene>(_ c: C) -> C { c }
}

protocol _Launchable { func _launch() -> Never }

public struct WindowGroup<Content: View>: Scene, _Launchable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    public init(_ title: String, @ViewBuilder content: () -> Content) { self.content = content() }
    func _launch() -> Never { _simcoreRun(content) }
}

public protocol App {
    associatedtype Body: Scene
    init()
    @SceneBuilder var body: Body { get }
}

func _launchScene<S: Scene>(_ s: S) -> Never {
    if let l = s as? _Launchable { l._launch() }
    _launchScene(s.body)
}

extension App {
    public static func main() { _launchScene(Self().body) }
}
