public protocol Scene {
    associatedtype Body: Scene
    @SceneBuilder var body: Body { get }
}
extension Never: Scene {}

@resultBuilder
public struct SceneBuilder {
    public static func buildBlock<C: Scene>(_ c: C) -> C { c }
}

#if os(WASI)
typealias _Done = Void  // reactor: launching registers the app and returns
#else
typealias _Done = Never
#endif

protocol _Launchable { func _launch() -> _Done }

public struct WindowGroup<Content: View>: Scene, _Launchable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let content: Content
    public init(@ViewBuilder content: () -> Content) { self.content = content() }
    public init(_ title: String, @ViewBuilder content: () -> Content) { self.content = content() }
    func _launch() -> _Done { _simcoreRun(content) }
}

public protocol App {
    associatedtype Body: Scene
    init()
    @SceneBuilder var body: Body { get }
}

func _launchScene<S: Scene>(_ s: S) -> _Done {
    if let l = s as? _Launchable { return l._launch() }
    return _launchScene(s.body)
}

extension App {
    public static func main() { _launchScene(Self().body) }
}
