#if !os(WASI)
import Foundation
#endif
// ShareLink (the system share sheet) and PasteButton, over the session pasteboard (docs/protocol.md section U).

/// `scheme:` per RFC 3986 (a letter, then letters, digits, `+`, `-`, `.`).
func _hasScheme(_ s: String) -> Bool {
    guard let colon = s.firstIndex(of: ":"), let f = s.first, f.isLetter else { return false }
    return s[..<colon].allSatisfy { $0.isLetter || $0.isNumber || "+-.".contains($0) }
}

/// What the share sheet's header shows for an item.
public struct SharePreview<Image: Transferable, Icon: Transferable> {
    let title: String
    let image: Image?, icon: Icon?
    public init(_ title: String, image: Image, icon: Icon) { self.title = title; self.image = image; self.icon = icon }
    public init(_ title: Text, image: Image, icon: Icon) { self.init(title._plainText, image: image, icon: icon) }
}
extension SharePreview where Image == Never, Icon == Never {
    public init(_ title: String) { self.title = title; image = nil; icon = nil }
    public init(_ title: Text) { self.init(title._plainText) }
}
extension SharePreview where Icon == Never {
    public init(_ title: String, image: Image) { self.title = title; self.image = image; icon = nil }
    public init(_ title: Text, image: Image) { self.init(title._plainText, image: image) }
}
extension SharePreview where Image == Never {
    public init(_ title: String, icon: Icon) { self.title = title; image = nil; self.icon = icon }
    public init(_ title: Text, icon: Icon) { self.init(title._plainText, icon: icon) }
}

/// The default `ShareLink` label: the title (default "Share…") with the share symbol.
public struct DefaultShareLinkLabel: View {
    let title: Text
    init(_ title: Text = Text("Share\u{2026}")) { self.title = title }
    public var body: some View { Label { title } icon: { Image(systemName: "square.and.arrow.up") } }
}

final class _ShareState { var open = false }

/// A button presenting the share sheet for `items`. The sheet's Copy puts the items' text on the pasteboard;
/// other activities and the Close button dismiss it.
public struct ShareLink<Data: RandomAccessCollection, PreviewImage: Transferable, PreviewIcon: Transferable, Label: View>: View, _Flattenable
where Data.Element: Transferable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let items: Data, subject: Text?, message: Text?, preview: ((Data.Element) -> SharePreview<PreviewImage, PreviewIcon>)?, label: Label

    public init(items: Data, subject: Text? = nil, message: Text? = nil,
                preview: @escaping (Data.Element) -> SharePreview<PreviewImage, PreviewIcon>, @ViewBuilder label: () -> Label) {
        self.items = items; self.subject = subject; self.message = message; self.preview = preview; self.label = label()
    }

    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let key = "\(path)#ShareLink"
        rt.touched.insert(key)
        let st = (rt.slots[key] as? _ShareState) ?? _ShareState()
        rt.slots[key] = st
        var out = SwiftUI._children(Button(action: { st.open = true; rt.dirty = true }) { label }, path)
        guard st.open, rt.pageIsActive, !rt.disabled, !items.isEmpty else { st.open = false; return out }
        let texts = items.compactMap { _transferText($0) }   // items without a text form (an Image) share nothing
        let close = { st.open = false; rt.dirty = true }
        // Other activities hand the items to the system, as `openURL` does (section U): reported, then closed.
        var activities: Node = [:]
        for name in ["Reminders", "Safari", "More", "Save to Files", "Open in Safari", "Add to Reading List"] {
            activities[name] = rt.registerAction {
                _send(["type": "event", "name": "module.called",
                       "detail": ["module": "ShareSheet", "method": name, "args": texts] as Node])
                close()
            }
        }
        var l: Node = ["kind": "share", "items": texts, "dismiss": rt.registerAction(close), "activities": activities,
                       "copy": rt.registerAction { _setPasteboard(texts); close() }]
        if let id = out.first?["id"] as? String { l["anchor"] = id }
        if let subject { l["subject"] = subject._plainText }
        if let message { l["message"] = message._plainText }
        if items.allSatisfy({ $0 is URL }) { l["url"] = true }
        if let preview, let first = items.first {
            let p = preview(first)
            l["title"] = p.title
            if let v = p.image as? Image { l["image"] = _eval(v, path + "/previewImage") }
            if let v = p.icon as? Image { l["icon"] = _eval(v, path + "/previewIcon") }
        }
        rt.layers.append(l)
        if !out.isEmpty { out[0]["value"] = "sharing" }
        return out
    }
}

extension ShareLink {
    public init<I: Transferable>(item: I, subject: Text? = nil, message: Text? = nil, preview: SharePreview<PreviewImage, PreviewIcon>,
                                 @ViewBuilder label: () -> Label) where Data == CollectionOfOne<I> {
        self.init(items: CollectionOfOne(item), subject: subject, message: message, preview: { _ in preview }, label: label)
    }
}
extension ShareLink where PreviewImage == Never, PreviewIcon == Never {
    public init(items: Data, subject: Text? = nil, message: Text? = nil, @ViewBuilder label: () -> Label) {
        self.items = items; self.subject = subject; self.message = message; preview = nil; self.label = label()
    }
    public init<I: Transferable>(item: I, subject: Text? = nil, message: Text? = nil, @ViewBuilder label: () -> Label) where Data == CollectionOfOne<I> {
        self.init(items: CollectionOfOne(item), subject: subject, message: message, label: label)
    }
}
extension ShareLink where Label == DefaultShareLinkLabel {
    public init(items: Data, subject: Text? = nil, message: Text? = nil, preview: @escaping (Data.Element) -> SharePreview<PreviewImage, PreviewIcon>) {
        self.init(items: items, subject: subject, message: message, preview: preview) { DefaultShareLinkLabel() }
    }
    public init<S: StringProtocol>(_ title: S, items: Data, subject: Text? = nil, message: Text? = nil,
                                   preview: @escaping (Data.Element) -> SharePreview<PreviewImage, PreviewIcon>) {
        self.init(items: items, subject: subject, message: message, preview: preview) { DefaultShareLinkLabel(Text(String(title))) }
    }
    public init(_ title: Text, items: Data, subject: Text? = nil, message: Text? = nil,
                preview: @escaping (Data.Element) -> SharePreview<PreviewImage, PreviewIcon>) {
        self.init(items: items, subject: subject, message: message, preview: preview) { DefaultShareLinkLabel(title) }
    }
    public init<I: Transferable>(item: I, subject: Text? = nil, message: Text? = nil, preview: SharePreview<PreviewImage, PreviewIcon>)
    where Data == CollectionOfOne<I> {
        self.init(item: item, subject: subject, message: message, preview: preview) { DefaultShareLinkLabel() }
    }
    public init<S: StringProtocol, I: Transferable>(_ title: S, item: I, subject: Text? = nil, message: Text? = nil,
                                                    preview: SharePreview<PreviewImage, PreviewIcon>) where Data == CollectionOfOne<I> {
        self.init(item: item, subject: subject, message: message, preview: preview) { DefaultShareLinkLabel(Text(String(title))) }
    }
    public init<I: Transferable>(_ title: Text, item: I, subject: Text? = nil, message: Text? = nil,
                                 preview: SharePreview<PreviewImage, PreviewIcon>) where Data == CollectionOfOne<I> {
        self.init(item: item, subject: subject, message: message, preview: preview) { DefaultShareLinkLabel(title) }
    }
}
extension ShareLink where PreviewImage == Never, PreviewIcon == Never, Label == DefaultShareLinkLabel {
    public init(items: Data, subject: Text? = nil, message: Text? = nil) {
        self.init(items: items, subject: subject, message: message) { DefaultShareLinkLabel() }
    }
    public init<S: StringProtocol>(_ title: S, items: Data, subject: Text? = nil, message: Text? = nil) {
        self.init(items: items, subject: subject, message: message) { DefaultShareLinkLabel(Text(String(title))) }
    }
    public init(_ title: Text, items: Data, subject: Text? = nil, message: Text? = nil) {
        self.init(items: items, subject: subject, message: message) { DefaultShareLinkLabel(title) }
    }
    public init(item: URL, subject: Text? = nil, message: Text? = nil) where Data == CollectionOfOne<URL> {
        self.init(items: CollectionOfOne(item), subject: subject, message: message)
    }
    public init(item: String, subject: Text? = nil, message: Text? = nil) where Data == CollectionOfOne<String> {
        self.init(items: CollectionOfOne(item), subject: subject, message: message)
    }
    public init<S: StringProtocol>(_ title: S, item: URL, subject: Text? = nil, message: Text? = nil) where Data == CollectionOfOne<URL> {
        self.init(title, items: CollectionOfOne(item), subject: subject, message: message)
    }
    public init<S: StringProtocol>(_ title: S, item: String, subject: Text? = nil, message: Text? = nil) where Data == CollectionOfOne<String> {
        self.init(title, items: CollectionOfOne(item), subject: subject, message: message)
    }
    public init(_ title: Text, item: URL, subject: Text? = nil, message: Text? = nil) where Data == CollectionOfOne<URL> {
        self.init(title, items: CollectionOfOne(item), subject: subject, message: message)
    }
    public init(_ title: Text, item: String, subject: Text? = nil, message: Text? = nil) where Data == CollectionOfOne<String> {
        self.init(title, items: CollectionOfOne(item), subject: subject, message: message)
    }
}

/// Replaces the session pasteboard and tells the host (`{"type":"pasteboard","items"}`).
func _setPasteboard(_ items: [String]) {
    _Runtime.shared.pasteboard = items
    _send(["type": "pasteboard", "items": items])
}

/// The system Paste button: pastes the pasteboard items that convert to `T` without a permission prompt.
/// Always drawn enabled (as on iOS); a tap with nothing acceptable on the pasteboard does nothing.
public struct PasteButton: View, _Primitive {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let accepts: (String) -> Bool, paste: ([String]) -> Void
    public init<T: Transferable>(payloadType: T.Type, onPaste: @escaping ([T]) -> Void) {
        accepts = { _transferValue($0, T.self) != nil }
        paste = { texts in onPaste(texts.compactMap { _transferValue($0, T.self) }) }
    }
    func _emit(_ path: String) -> Node {
        let rt = _Runtime.shared, paste = paste
        // iOS draws it enabled whatever the pasteboard holds; a tap with nothing it accepts does nothing.
        let items = rt.pasteboard.filter(accepts)
        let label = _eval(Label("Paste", systemImage: "doc.on.clipboard"), path + "/0")
        return ["kind": "button", "id": path, "style": "pasteButton", "disabled": rt.disabled,
                "action": rt.disabled ? 0 : rt.registerAction { if !items.isEmpty { paste(items) } }, "label": label]
    }
}

#if canImport(UniformTypeIdentifiers)
import UniformTypeIdentifiers
#endif

extension PasteButton {
    /// Pasteboard text matches a type that plain text (or, for text with a scheme, a URL) conforms to.
    static func _accepts(_ types: [UTType]) -> (String) -> Bool {
        { text in types.contains { UTType.plainText.conforms(to: $0) || (_hasScheme(text) && UTType.url.conforms(to: $0)) } }
    }
    public init(supportedContentTypes: [UTType], payloadAction: @escaping ([NSItemProvider]) -> Void) {
        accepts = PasteButton._accepts(supportedContentTypes)
        paste = { texts in payloadAction(texts.map(_itemProvider)) }
    }
    public init<Payload>(supportedContentTypes: [UTType], validator: @escaping ([NSItemProvider]) -> Payload?,
                         payloadAction: @escaping (Payload) -> Void) {
        accepts = PasteButton._accepts(supportedContentTypes)
        paste = { texts in if let p = validator(texts.map(_itemProvider)) { payloadAction(p) } }
    }
}
