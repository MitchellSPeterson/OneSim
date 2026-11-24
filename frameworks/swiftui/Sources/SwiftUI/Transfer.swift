#if !os(WASI)
import Foundation
#endif
#if canImport(UniformTypeIdentifiers)
import UniformTypeIdentifiers
#endif
// CoreTransferable, declared in OneSim's SwiftUI module so `import SwiftUI` apps get it (docs/protocol.md
// section W). OneSim's pasteboard and share sheet carry text: every representation converts an item to text
// (export) and back (import). `String` is itself, `URL` its absolute string, `Data` its UTF-8, a Codable item
// its JSON; `ProxyRepresentation` goes through its proxy.

public protocol Transferable {
    associatedtype Representation: TransferRepresentation
    @TransferRepresentationBuilder<Self> static var transferRepresentation: Representation { get }
}

public protocol TransferRepresentation<Item> {
    associatedtype Item: Transferable
    associatedtype Body: TransferRepresentation
    @TransferRepresentationBuilder<Item> var body: Body { get }
}

/// A representation that converts by itself (not through `body`). Conversions report through `done`: at once for
/// synchronous ones, on the guest thread later for async closures (the event loop never blocks on them).
protocol _TransferLeaf {
    func _export(_ item: Any, _ done: @escaping (String?) -> Void)
    func _import(_ text: String, _ done: @escaping (Any?) -> Void)
}

/// The first non-nil result of `f` over `xs`, tried in order.
func _firstResult<A, R>(_ xs: ArraySlice<A>, _ f: @escaping (A, @escaping (R?) -> Void) -> Void, _ done: @escaping (R?) -> Void) {
    guard let x = xs.first else { return done(nil) }
    f(x) { r in if let r { done(r) } else { _firstResult(xs.dropFirst(), f, done) } }
}

/// `f` over every element in order, keeping the non-nil results.
func _allResults<A, R>(_ xs: [A], _ f: @escaping (A, @escaping (R?) -> Void) -> Void, _ done: @escaping ([R]) -> Void) {
    var out: [R] = []
    func step(_ i: Int) {
        guard i < xs.count else { return done(out) }
        f(xs[i]) { r in if let r { out.append(r) }; step(i + 1) }
    }
    step(0)
}

extension TransferRepresentation {
    /// The converting representations, in declaration order (the first that succeeds wins).
    func _leaves() -> [_TransferLeaf] {
        if let l = self as? _TransferLeaf { return [l] }
        if let g = self as? _AnyTransferGroup { return g.leaves }
        if self is _AnyNoBody { return [] }
        return body._leaves()
    }
}

/// Several representations from a `TransferRepresentationBuilder` block.
public struct _TransferGroup<Item: Transferable>: TransferRepresentation {
    let leaves: [_TransferLeaf]
    public var body: _NoBody<Item> { _NoBody() }
}
protocol _AnyTransferGroup { var leaves: [_TransferLeaf] { get } }
protocol _AnyNoBody {}
extension _NoBody: _AnyNoBody {}
extension _TransferGroup: _AnyTransferGroup {}

@resultBuilder
public struct TransferRepresentationBuilder<Item: Transferable> {
    public static func buildExpression<R: TransferRepresentation>(_ r: R) -> R where R.Item == Item { r }
    public static func buildBlock<R: TransferRepresentation>(_ r: R) -> R where R.Item == Item { r }
    public static func buildBlock(_ rs: any TransferRepresentation...) -> _TransferGroup<Item> {
        _TransferGroup(leaves: rs.flatMap { $0._leaves() })
    }
    public static func buildOptional<R: TransferRepresentation>(_ r: R?) -> _TransferGroup<Item> where R.Item == Item {
        _TransferGroup(leaves: r?._leaves() ?? [])
    }
    public static func buildEither<R: TransferRepresentation>(first r: R) -> _TransferGroup<Item> where R.Item == Item {
        _TransferGroup(leaves: r._leaves())
    }
    public static func buildEither<R: TransferRepresentation>(second r: R) -> _TransferGroup<Item> where R.Item == Item {
        _TransferGroup(leaves: r._leaves())
    }
    public static func buildLimitedAvailability<R: TransferRepresentation>(_ r: R) -> _TransferGroup<Item> where R.Item == Item {
        _TransferGroup(leaves: r._leaves())
    }
}

/// The body of representations that convert by themselves.
public struct _NoBody<Item: Transferable>: TransferRepresentation {
    public var body: _NoBody<Item> { self }
}

extension Never: Transferable {
    public static var transferRepresentation: _NoBody<Never> { _NoBody() }
}

/// Text conversions of the built-in transferable types.
public struct _TextRepresentation<Item: Transferable>: TransferRepresentation, _TransferLeaf {
    let export: (Item) -> String?, importing: (String) -> Item?
    public var body: _NoBody<Item> { _NoBody() }
    func _export(_ item: Any, _ done: @escaping (String?) -> Void) { done((item as? Item).flatMap(export)) }
    func _import(_ text: String, _ done: @escaping (Any?) -> Void) { done(importing(text)) }
}

extension String: Transferable {
    public static var transferRepresentation: _TextRepresentation<String> { _TextRepresentation<String>(export: { $0 }, importing: { $0 }) }
}
extension URL: Transferable {
    /// Only text that is a whole URL with a scheme imports as a URL (plain text has no URL representation).
    public static var transferRepresentation: _TextRepresentation<URL> {
        _TextRepresentation<URL>(export: { $0.absoluteString }, importing: { t in !t.contains(" ") && _hasScheme(t) ? URL(string: t) : nil })
    }
}
extension Image: Transferable {
    /// An image has no text form: it is shared as an image (a `SharePreview`), never pasted.
    public static var transferRepresentation: _TextRepresentation<Image> { _TextRepresentation<Image>(export: { _ in nil }, importing: { _ in nil }) }
}
#if !os(WASI)
extension Data: Transferable {
    public static var transferRepresentation: _TextRepresentation<Data> {
        _TextRepresentation<Data>(export: { String(data: $0, encoding: .utf8) }, importing: { Data($0.utf8) })
    }
}
#endif

/// Converts through another transferable type: `exporting` makes the proxy, `importing` builds the item from one.
public struct ProxyRepresentation<Item: Transferable, ProxyRepresentation: Transferable>: TransferRepresentation, _TransferLeaf {
    let exporting: ((Item) throws -> ProxyRepresentation)?
    let importing: ((ProxyRepresentation) throws -> Item)?
    public var body: _NoBody<Item> { _NoBody() }
    public init(exporting: @escaping (Item) throws -> ProxyRepresentation) { self.exporting = exporting; importing = nil }
    public init(exporting: @escaping (Item) throws -> ProxyRepresentation, importing: @escaping (ProxyRepresentation) throws -> Item) {
        self.exporting = exporting; self.importing = importing
    }
    public init(importing: @escaping (ProxyRepresentation) throws -> Item) { exporting = nil; self.importing = importing }
    func _export(_ item: Any, _ done: @escaping (String?) -> Void) {
        guard let exporting, let i = item as? Item, let proxy = try? exporting(i) else { return done(nil) }
        _transferText(proxy, done)
    }
    func _import(_ text: String, _ done: @escaping (Any?) -> Void) {
        guard let importing else { return done(nil) }
        _transferValue(text, ProxyRepresentation.self) { proxy in done(proxy.flatMap { try? importing($0) }) }
    }
}

#if !os(WASI)
/// A `Codable` item as JSON text.
public struct CodableRepresentation<Item: Transferable & Codable, Encoder, Decoder>: TransferRepresentation, _TransferLeaf {
    let contentType: UTType
    public var body: _NoBody<Item> { _NoBody() }
    public init(contentType: UTType) where Encoder == JSONEncoder, Decoder == JSONDecoder { self.contentType = contentType }
    public init(for itemType: Item.Type = Item.self, contentType: UTType) where Encoder == JSONEncoder, Decoder == JSONDecoder {
        self.contentType = contentType
    }
    func _export(_ item: Any, _ done: @escaping (String?) -> Void) {
        guard let i = item as? Item, let d = try? JSONEncoder().encode(i) else { return done(nil) }
        done(String(data: d, encoding: .utf8))
    }
    func _import(_ text: String, _ done: @escaping (Any?) -> Void) { done(try? JSONDecoder().decode(Item.self, from: Data(text.utf8))) }
}

/// Raw bytes as UTF-8 text. The async closures run off the guest thread; the share or paste completes when they do.
public struct DataRepresentation<Item: Transferable>: TransferRepresentation, _TransferLeaf {
    let exporting: ((Item) async throws -> Data)?
    let importing: ((Data) async throws -> Item)?
    public var body: _NoBody<Item> { _NoBody() }
    public init(contentType: UTType, exporting: @escaping (Item) async throws -> Data, importing: @escaping (Data) async throws -> Item) {
        self.exporting = exporting; self.importing = importing
    }
    public init(exportedContentType: UTType, exporting: @escaping (Item) async throws -> Data) { self.exporting = exporting; importing = nil }
    public init(importedContentType: UTType, importing: @escaping (Data) async throws -> Item) { exporting = nil; self.importing = importing }
    func _export(_ item: Any, _ done: @escaping (String?) -> Void) {
        guard let exporting, let i = item as? Item else { return done(nil) }
        _async({ try await exporting(i) }) { d in done(d.flatMap { String(data: $0, encoding: .utf8) }) }
    }
    func _import(_ text: String, _ done: @escaping (Any?) -> Void) {
        guard let importing else { return done(nil) }
        _async({ try await importing(Data(text.utf8)) }) { done($0) }
    }
}

/// A received or sent file.
public struct SentTransferredFile: Sendable {
    public let file: URL
    public let allowAccessingOriginalFile: Bool
    public init(_ file: URL, allowAccessingOriginalFile: Bool = false) { self.file = file; self.allowAccessingOriginalFile = allowAccessingOriginalFile }
}
public struct ReceivedTransferredFile: Sendable {
    public let file: URL
    public let isOriginalFile: Bool
}

/// A file's contents as UTF-8 text (sharing reads the exported file; pasting writes a temporary one).
public struct FileRepresentation<Item: Transferable>: TransferRepresentation, _TransferLeaf {
    let exporting: ((Item) async throws -> SentTransferredFile)?
    let importing: ((ReceivedTransferredFile) async throws -> Item)?
    public var body: _NoBody<Item> { _NoBody() }
    public init(contentType: UTType, shouldAttemptToOpenInPlace: Bool = false, exporting: @escaping (Item) async throws -> SentTransferredFile,
                importing: @escaping (ReceivedTransferredFile) async throws -> Item) { self.exporting = exporting; self.importing = importing }
    public init(exportedContentType: UTType, shouldAllowToOpenInPlace: Bool = false, exporting: @escaping (Item) async throws -> SentTransferredFile) {
        self.exporting = exporting; importing = nil
    }
    public init(importedContentType: UTType, shouldAttemptToOpenInPlace: Bool = false, importing: @escaping (ReceivedTransferredFile) async throws -> Item) {
        exporting = nil; self.importing = importing
    }
    func _export(_ item: Any, _ done: @escaping (String?) -> Void) {
        guard let exporting, let i = item as? Item else { return done(nil) }
        _async({ try await exporting(i) }) { f in
            done(f.flatMap { try? Data(contentsOf: $0.file) }.flatMap { String(data: $0, encoding: .utf8) })
        }
    }
    func _import(_ text: String, _ done: @escaping (Any?) -> Void) {
        guard let importing else { return done(nil) }
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("onesim-paste-\(UUID().uuidString).txt")
        guard (try? Data(text.utf8).write(to: url)) != nil else { return done(nil) }
        _async({ try await importing(ReceivedTransferredFile(file: url, isOriginalFile: false)) }) { done($0) }
    }
}

/// Runs an async conversion off the guest thread and hands its result back on the guest thread (the event loop
/// keeps running meanwhile, so a closure may hop to the main actor).
func _async<T>(_ f: @escaping @Sendable () async throws -> T, _ done: @escaping (T?) -> Void) {
    let box = _AsyncResult<T>()
    Task.detached {
        box.value = try? await f()
        box.finished.signal()
        _onGuestThread { done(box.value) }
    }
    // The guest thread only drains main-actor jobs when woken, so keep waking it while the closure runs.
    // ponytail: a 5 ms nudge loop per conversion; a main-queue hook in the runtime would make it event-driven.
    Thread.detachNewThread {
        while box.finished.wait(timeout: .now() + .milliseconds(5)) == .timedOut { _wakeGuest() }
    }
}
final class _AsyncResult<T>: @unchecked Sendable {
    var value: T?
    let finished = DispatchSemaphore(value: 0)
}
#endif

extension TransferRepresentation {
    /// Only offers this representation when `condition` holds for the item.
    public func exportingCondition(_ condition: @escaping (Item) -> Bool) -> some TransferRepresentation<Item> {
        _ConditionalRepresentation<Item>(leaves: _leaves(), condition: condition)
    }
    /// File names do not exist in OneSim's text transfers: no effect.
    public func suggestedFileName(_ fileName: String) -> Self { self }
    public func suggestedFileName(_ fileName: @escaping (Item) -> String?) -> Self { self }
    /// Visibility to other apps or processes does not apply to a single app's text pasteboard: no effect.
    public func visibility(_ visibility: TransferRepresentationVisibility) -> Self { self }
}

public struct _ConditionalRepresentation<Item: Transferable>: TransferRepresentation, _TransferLeaf {
    let leaves: [_TransferLeaf], condition: (Item) -> Bool
    public var body: _NoBody<Item> { _NoBody() }
    func _export(_ item: Any, _ done: @escaping (String?) -> Void) {
        guard let i = item as? Item, condition(i) else { return done(nil) }
        _firstResult(leaves[...], { l, d in l._export(i, d) }, done)
    }
    func _import(_ text: String, _ done: @escaping (Any?) -> Void) { _firstResult(leaves[...], { l, d in l._import(text, d) }, done) }
}

public struct TransferRepresentationVisibility: Sendable {
    public static let all = TransferRepresentationVisibility()
    public static let team = TransferRepresentationVisibility()
    public static let group = TransferRepresentationVisibility()
    public static let ownProcess = TransferRepresentationVisibility()
}

/// An item's text, from the first of its representations that exports it (nil: it has no text form).
func _transferText(_ v: Any, _ done: @escaping (String?) -> Void) {
    guard let t = v as? any Transferable else { return done(nil) }
    func open<T: Transferable>(_ x: T) { _firstResult(T.transferRepresentation._leaves()[...], { l, d in l._export(x, d) }, done) }
    open(t)
}

/// Builds a `T` from pasted text with the first of its representations that imports it; nil when none does.
func _transferValue<T>(_ text: String, _ type: T.Type, _ done: @escaping (T?) -> Void) {
    guard let t = T.self as? any Transferable.Type else { return done(nil) }
    func open<U: Transferable>(_ u: U.Type) {
        _firstResult(U.transferRepresentation._leaves()[...], { l, d in l._import(text, d) }) { done($0 as? T) }
    }
    open(t)
}

#if !canImport(UniformTypeIdentifiers)
/// UniformTypeIdentifiers' type, for guests whose platform lacks the framework (Linux, WASI).
public struct UTType: Hashable, Sendable, CustomStringConvertible {
    public let identifier: String
    let supertypes: [String]
    init(_ identifier: String, _ supertypes: [String]) { self.identifier = identifier; self.supertypes = supertypes }
    public init?(_ identifier: String) { self.init(identifier, ["public.data", "public.item"]) }
    public init(exportedAs identifier: String, conformingTo parent: UTType? = nil) {
        self.init(identifier, parent.map { [$0.identifier] + $0.supertypes } ?? ["public.data", "public.item"])
    }
    public init(importedAs identifier: String, conformingTo parent: UTType? = nil) { self.init(exportedAs: identifier, conformingTo: parent) }
    public var description: String { identifier }
    public func conforms(to type: UTType) -> Bool { type.identifier == identifier || supertypes.contains(type.identifier) }
    public static let item = UTType("public.item", [])
    public static let content = UTType("public.content", ["public.item"])
    public static let data = UTType("public.data", ["public.item"])
    public static let text = UTType("public.text", ["public.data", "public.content", "public.item"])
    public static let plainText = UTType("public.plain-text", ["public.text", "public.data", "public.content", "public.item"])
    public static let utf8PlainText = UTType("public.utf8-plain-text", ["public.plain-text", "public.text", "public.data", "public.content", "public.item"])
    public static let json = UTType("public.json", ["public.text", "public.data", "public.content", "public.item"])
    public static let url = UTType("public.url", ["public.data", "public.item"])
    public static let fileURL = UTType("public.file-url", ["public.url", "public.data", "public.item"])
    public static let image = UTType("public.image", ["public.data", "public.content", "public.item"])
    public static let png = UTType("public.png", ["public.image", "public.data", "public.content", "public.item"])
    public static let jpeg = UTType("public.jpeg", ["public.image", "public.data", "public.content", "public.item"])
}

/// Foundation's item provider, for guests whose Foundation lacks it: holds one pasted text.
public final class NSItemProvider {
    let text: String
    init(text: String) { self.text = text }
    public var registeredTypeIdentifiers: [String] { [UTType.utf8PlainText.identifier] }
    public func hasItemConformingToTypeIdentifier(_ typeIdentifier: String) -> Bool {
        UTType.utf8PlainText.conforms(to: UTType(typeIdentifier, []))
    }
    public func canLoadObject(ofClass aClass: String.Type) -> Bool { true }
    public func loadObject(ofClass aClass: String.Type, completionHandler: @escaping (String?, (any Error)?) -> Void) {
        completionHandler(text, nil)
    }
    public func loadItem(forTypeIdentifier typeIdentifier: String, options: [AnyHashable: Any]? = nil,
                         completionHandler: ((Any?, (any Error)?) -> Void)? = nil) {
        completionHandler?(text, nil)
    }
}
func _itemProvider(_ text: String) -> NSItemProvider { NSItemProvider(text: text) }
#else
func _itemProvider(_ text: String) -> NSItemProvider { NSItemProvider(object: text as NSString) }
#endif
