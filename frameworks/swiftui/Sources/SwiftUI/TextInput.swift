#if !os(WASI)
import Foundation
#endif

public struct TextField<Label: View>: View, _Primitive {
    public var body: Never { fatalError() }
    let label: Label
    var input: _TextInput

    public init(text: Binding<String>, prompt: Text? = nil, axis: Axis = .horizontal, @ViewBuilder label: () -> Label) {
        self.init(label: label(), input: _TextInput(text: text, prompt: prompt, axis: axis))
    }

    init(label: Label, input: _TextInput) {
        self.label = label
        self.input = input
    }

    func _emit(_ path: String) -> Node {
        if input.style == nil, let style = _Runtime.shared.textInput.fieldStyle {
            let configuration = TextField<_TextFieldStyleLabel>(label: _TextFieldStyleLabel(label), input: input)
            return style(configuration, path)
        }
        return input.emit(_eval(label, path + "/label"), path)
    }
}

extension TextField where Label == Text {
    public init<S: StringProtocol>(_ title: S, text: Binding<String>, prompt: Text? = nil, axis: Axis = .horizontal) {
        self.init(text: text, prompt: prompt, axis: axis) { Text(String(title)) }
    }

    public init<S: StringProtocol>(_ title: S, text: Binding<String>, onCommit: @escaping () -> Void) {
        self.init(title, text: text)
        input.commit = onCommit
    }
}

public struct SecureField<Label: View>: View {
    let field: TextField<Label>

    public init(text: Binding<String>, prompt: Text? = nil, @ViewBuilder label: () -> Label) {
        field = TextField(label: label(), input: _TextInput(text: text, prompt: prompt, secure: true))
    }

    public var body: some View { field }
}

extension SecureField where Label == Text {
    public init<S: StringProtocol>(_ title: S, text: Binding<String>, prompt: Text? = nil) {
        self.init(text: text, prompt: prompt) { Text(String(title)) }
    }

    public init<S: StringProtocol>(_ title: S, text: Binding<String>, onCommit: @escaping () -> Void) {
        field = TextField(label: Text(String(title)), input: _TextInput(text: text, secure: true, commit: onCommit))
    }
}

public struct TextEditor: View, _Primitive {
    public var body: Never { fatalError() }
    let input: _TextInput

    public init(text: Binding<String>) {
        input = _TextInput(text: text, axis: .vertical, editor: true)
    }

    init(input: _TextInput) { self.input = input }

    func _emit(_ path: String) -> Node {
        if input.style == nil, let style = _Runtime.shared.textInput.editorStyle {
            return style(self, path)
        }
        return input.emit(nil, path)
    }
}

enum _TextInputStyle: String {
    case automatic, plain, bordered
    case roundedBorder = "rounded_border"
}

public struct TextInputBorderShape: InsettableShape, Sendable, Equatable {
    enum Kind: String, Sendable {
        case automatic, capsule
        case roundedRectangle = "rounded_rectangle"
    }

    let kind: Kind
    private var insetAmount: CGFloat = 0

    public static let automatic = Self(kind: .automatic)
    public static let capsule = Self(kind: .capsule)
    public static let roundedRectangle = Self(kind: .roundedRectangle)

    public func path(in rect: CGRect) -> Path {
        switch kind {
        case .automatic, .capsule:
            return Capsule().inset(by: insetAmount).path(in: rect)
        case .roundedRectangle:
            return RoundedRectangle(cornerRadius: 10).inset(by: insetAmount).path(in: rect)
        }
    }

    public func inset(by amount: CGFloat) -> Self {
        var copy = self
        copy.insetAmount += amount
        return copy
    }
}

struct _TextInput {
    let text: Binding<String>
    var prompt: Text?
    var axis: Axis = .horizontal
    var secure = false
    var editor = false
    var style: _TextInputStyle?
    var commit: (() -> Void)?

    func emit(_ label: Node?, _ path: String) -> Node {
        let rt = _Runtime.shared
        let context = rt.textInput
        let placeholder = prompt?.runs.map(\.text).joined() ?? label?["text"] as? String ?? ""
        let isEnabled = !rt.disabled && rt.pageIsActive
        var node: Node = ["kind": "text_field", "id": path, "text": text.wrappedValue, "placeholder": placeholder,
                          "field": isEnabled ? rt.registerField { text.wrappedValue = $0 } : 0]
        if !isEnabled { node["disabled"] = true }
        if secure { node["secure"] = true }
        if editor { node["editor"] = true }
        if axis == .vertical { node["axis"] = "vertical" }
        node["input_style"] = style?.rawValue
        node["input_border_shape"] = context.borderShape?.kind.rawValue
        if let prompt { node["prompt"] = _eval(prompt, path + "/prompt") }
        node["keyboard_type"] = context.keyboardType
        node["autocapitalization"] = context.autocapitalization?.name
        node["submit_label"] = context.submitLabel?.name
        node["autocorrection_disabled"] = EnvironmentValues().disableAutocorrection
        node["content_type"] = context.contentType?.rawValue
        guard isEnabled else { return node }
        let targets = context.focus
        if !targets.isEmpty {
            rt.textFocus[path] = targets
            node["focused"] = targets.contains { $0.isFocused() }
            node["focus_action"] = rt.registerAction {
                for (id, others) in rt.textFocus where id != path {
                    for other in others { other.blur() }
                }
                var identities: Set<ObjectIdentifier> = []
                for target in targets where identities.insert(target.identity).inserted { target.focus() }
            }
            node["blur_action"] = rt.registerAction { for target in targets { target.blur() } }
        }
        var submit = context.submit.filter { $0.0.contains(.text) }.map(\.1)
        if let commit { submit.append(commit) }
        if !submit.isEmpty {
            node["submit_action"] = rt.registerAction { submit.forEach { $0() } }
        }
        return node
    }
}

struct _TextInputContext {
    var fieldStyle: ((TextField<_TextFieldStyleLabel>, String) -> Node)?
    var editorStyle: ((TextEditor, String) -> Node)?
    var focus: [_TextFocus] = []
    var borderShape: TextInputBorderShape?
    var keyboardType: String?
    var autocapitalization: TextInputAutocapitalization?
    var submitLabel: SubmitLabel?
    var contentType: UITextContentType?
    var submit: [(SubmitTriggers, () -> Void)] = []
}

private struct _AutocorrectionKey: EnvironmentKey { static let defaultValue: Bool? = nil }

extension EnvironmentValues {
    public var autocorrectionDisabled: Bool {
        get { disableAutocorrection ?? false }
        set { disableAutocorrection = newValue }
    }

    public var disableAutocorrection: Bool? {
        get { self[_AutocorrectionKey.self] }
        set { self[_AutocorrectionKey.self] = newValue }
    }
}

public enum UIKeyboardType: Int, Sendable {
    case `default`, asciiCapable, numbersAndPunctuation, URL, numberPad, phonePad, namePhonePad
    case emailAddress, decimalPad, twitter, webSearch, asciiCapableNumberPad
    public static let alphabet = asciiCapable
}

public enum UITextAutocapitalizationType: Int, Sendable {
    case none, words, sentences, allCharacters
}

public struct TextInputAutocapitalization: Sendable {
    let name: String
    public static let never = Self(name: "never")
    public static let words = Self(name: "words")
    public static let sentences = Self(name: "sentences")
    public static let characters = Self(name: "characters")

    public init?(_ type: UITextAutocapitalizationType) {
        switch type {
        case .none: self = .never
        case .words: self = .words
        case .sentences: self = .sentences
        case .allCharacters: self = .characters
        }
    }

    private init(name: String) { self.name = name }
}

public struct UITextContentType: RawRepresentable, Hashable, Sendable {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
}

extension UITextContentType {
    public static let name = Self(rawValue: "name")
    public static let namePrefix = Self(rawValue: "namePrefix")
    public static let givenName = Self(rawValue: "givenName")
    public static let middleName = Self(rawValue: "middleName")
    public static let familyName = Self(rawValue: "familyName")
    public static let nameSuffix = Self(rawValue: "nameSuffix")
    public static let nickname = Self(rawValue: "nickname")
    public static let jobTitle = Self(rawValue: "jobTitle")
    public static let organizationName = Self(rawValue: "organizationName")
    public static let location = Self(rawValue: "location")
    public static let fullStreetAddress = Self(rawValue: "fullStreetAddress")
    public static let streetAddressLine1 = Self(rawValue: "streetAddressLine1")
    public static let streetAddressLine2 = Self(rawValue: "streetAddressLine2")
    public static let addressCity = Self(rawValue: "addressCity")
    public static let addressState = Self(rawValue: "addressState")
    public static let addressCityAndState = Self(rawValue: "addressCityAndState")
    public static let sublocality = Self(rawValue: "sublocality")
    public static let countryName = Self(rawValue: "countryName")
    public static let postalCode = Self(rawValue: "postalCode")
    public static let telephoneNumber = Self(rawValue: "telephoneNumber")
    public static let emailAddress = Self(rawValue: "emailAddress")
    public static let URL = Self(rawValue: "URL")
    public static let creditCardNumber = Self(rawValue: "creditCardNumber")
    public static let username = Self(rawValue: "username")
    public static let password = Self(rawValue: "password")
    public static let newPassword = Self(rawValue: "newPassword")
    public static let oneTimeCode = Self(rawValue: "oneTimeCode")
    public static let shipmentTrackingNumber = Self(rawValue: "shipmentTrackingNumber")
    public static let flightNumber = Self(rawValue: "flightNumber")
    public static let dateTime = Self(rawValue: "dateTime")
    public static let birthdate = Self(rawValue: "birthdate")
    public static let birthdateDay = Self(rawValue: "birthdateDay")
    public static let birthdateMonth = Self(rawValue: "birthdateMonth")
    public static let birthdateYear = Self(rawValue: "birthdateYear")
    public static let creditCardSecurityCode = Self(rawValue: "creditCardSecurityCode")
    public static let creditCardName = Self(rawValue: "creditCardName")
    public static let creditCardGivenName = Self(rawValue: "creditCardGivenName")
    public static let creditCardMiddleName = Self(rawValue: "creditCardMiddleName")
    public static let creditCardFamilyName = Self(rawValue: "creditCardFamilyName")
    public static let creditCardExpiration = Self(rawValue: "creditCardExpiration")
    public static let creditCardExpirationMonth = Self(rawValue: "creditCardExpirationMonth")
    public static let creditCardExpirationYear = Self(rawValue: "creditCardExpirationYear")
    public static let creditCardType = Self(rawValue: "creditCardType")
    public static let cellularEID = Self(rawValue: "cellularEID")
    public static let cellularIMEI = Self(rawValue: "cellularIMEI")
    public static let cellularIMEI1 = Self(rawValue: "cellularIMEI1")
    public static let cellularIMEI2 = Self(rawValue: "cellularIMEI2")
    public static let cellularNAL = Self(rawValue: "cellularNAL")
}

public struct SubmitLabel: Sendable {
    let name: String
    public static let done = Self(name: "done")
    public static let go = Self(name: "go")
    public static let send = Self(name: "send")
    public static let join = Self(name: "join")
    public static let route = Self(name: "route")
    public static let search = Self(name: "search")
    public static let `return` = Self(name: "return")
    public static let next = Self(name: "next")
    public static let `continue` = Self(name: "continue")
}

public struct SubmitTriggers: OptionSet, Sendable {
    public let rawValue: Int
    public init(rawValue: Int) { self.rawValue = rawValue }
    public static let text = Self(rawValue: 1)
    public static let search = Self(rawValue: 2)
}

extension View {
    func _textInput(_ apply: @escaping (inout _TextInputContext) -> Void) -> some View {
        _CtxView(content: self) { apply(&$0.textInput) }
    }

    public func keyboardType(_ type: UIKeyboardType) -> some View { _textInput { $0.keyboardType = String(describing: type) } }
    public func textInputAutocapitalization(_ style: TextInputAutocapitalization?) -> some View { _textInput { $0.autocapitalization = style } }
    public func autocapitalization(_ style: UITextAutocapitalizationType) -> some View { textInputAutocapitalization(TextInputAutocapitalization(style)) }
    public func textInputBorderShape(_ shape: TextInputBorderShape) -> some View { _textInput { $0.borderShape = shape } }
    public func submitLabel(_ label: SubmitLabel) -> some View { _textInput { $0.submitLabel = label } }
    public func textContentType(_ type: UITextContentType?) -> some View { _textInput { $0.contentType = type } }
    public func autocorrectionDisabled(_ disable: Bool = true) -> some View { environment(\.autocorrectionDisabled, disable) }
    public func disableAutocorrection(_ disable: Bool?) -> some View { environment(\.disableAutocorrection, disable) }

    public func onSubmit(of triggers: SubmitTriggers = .text, _ action: @escaping () -> Void) -> some View {
        _textInput { $0.submit.append((triggers, action)) }
    }

    public func submitScope(_ isBlocking: Bool = true) -> some View {
        _textInput { if isBlocking { $0.submit.removeAll() } }
    }
}

@propertyWrapper
public struct FocusState<Value: Hashable>: _StateProperty {
    @propertyWrapper
    public struct Binding {
        let state: FocusState<Value>
        public var wrappedValue: Value {
            get { state.wrappedValue }
            nonmutating set { state.wrappedValue = newValue }
        }
        public var projectedValue: Self { self }
    }

    private let state: State<Value>
    private let resetValue: Value

    public init() where Value == Bool { state = State(wrappedValue: false); resetValue = false }
    public init<T>() where Value == T?, T: Hashable { state = State(wrappedValue: nil); resetValue = nil }

    func _bind(_ key: String) { state._bind(key) }
    public var wrappedValue: Value {
        get { state.wrappedValue }
        nonmutating set {
            guard newValue != state.wrappedValue else { return }
            if newValue != resetValue {
                let targets = _Runtime.shared.textFocus.values
                let id = identity, value = AnyHashable(newValue)
                let matchingViews = targets.filter { view in
                    view.contains { $0.identity == id && $0.value == value }
                }
                if !matchingViews.isEmpty {
                    let retained = Set(matchingViews.joined().map(\.identity))
                    for other in targets.joined() where !retained.contains(other.identity) { other.blur() }
                }
            }
            state.wrappedValue = newValue
        }
    }
    public var projectedValue: Binding { Binding(state: self) }

    private var identity: ObjectIdentifier {
        state.storage.slot.map(ObjectIdentifier.init) ?? ObjectIdentifier(state.storage)
    }

    func target(_ value: Value) -> _TextFocus {
        _TextFocus(identity: identity, value: AnyHashable(value),
                   isFocused: { value != resetValue && wrappedValue == value },
                   focus: { wrappedValue = value },
                   blur: { if wrappedValue == value { wrappedValue = resetValue } })
    }
}

struct _TextFocus {
    let identity: ObjectIdentifier
    let value: AnyHashable
    let isFocused: () -> Bool
    let focus: () -> Void
    let blur: () -> Void

    func matches(_ other: Self) -> Bool { identity == other.identity && value == other.value }
}

extension View {
    public func focused<Value: Hashable>(_ binding: FocusState<Value>.Binding, equals value: Value) -> some View {
        _textInput { $0.focus.append(binding.state.target(value)) }
    }

    public func focused(_ binding: FocusState<Bool>.Binding) -> some View { focused(binding, equals: true) }
}

public struct _TextFieldStyleLabel: View, _Primitive {
    let emit: (String) -> Node
    init<V: View>(_ content: V) { emit = { _eval(content, $0) } }
    public var body: Never { fatalError() }
    func _emit(_ path: String) -> Node { emit(path) }
}

public protocol TextFieldStyle {
    associatedtype _Body: View
    typealias _Label = _TextFieldStyleLabel
    @ViewBuilder func _body(configuration: TextField<_Label>) -> _Body
}

public protocol TextEditorStyle {
    associatedtype Body: View
    typealias Configuration = TextEditorStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
}

public struct TextEditorStyleConfiguration {
    let editor: TextEditor
}

extension View {
    public func textFieldStyle<S: TextFieldStyle>(_ style: S) -> some View {
        _textInput { context in
            context.fieldStyle = { field, path in
                let rt = _Runtime.shared
                let previous = rt.textInput.fieldStyle
                rt.textInput.fieldStyle = nil
                defer { rt.textInput.fieldStyle = previous }
                return _eval(style._body(configuration: field), path)
            }
        }
    }

    public func textEditorStyle<S: TextEditorStyle>(_ style: S) -> some View {
        _textInput { context in
            context.editorStyle = { editor, path in
                let rt = _Runtime.shared
                let previous = rt.textInput.editorStyle
                rt.textInput.editorStyle = nil
                defer { rt.textInput.editorStyle = previous }
                return _eval(style.makeBody(configuration: .init(editor: editor)), path)
            }
        }
    }
}

extension TextField {
    fileprivate func styled(_ style: _TextInputStyle) -> Self {
        var field = self
        field.input.style = style
        return field
    }
}

public struct DefaultTextFieldStyle: TextFieldStyle {
    public init() {}
    public func _body(configuration: TextField<_Label>) -> some View { configuration.styled(.automatic) }
}

public struct PlainTextFieldStyle: TextFieldStyle {
    public init() {}
    public func _body(configuration: TextField<_Label>) -> some View { configuration.styled(.plain) }
}

public struct BorderedTextFieldStyle: TextFieldStyle {
    public init() {}
    public func _body(configuration: TextField<_Label>) -> some View { configuration.styled(.bordered) }
}

public struct RoundedBorderTextFieldStyle: TextFieldStyle {
    public init() {}
    public func _body(configuration: TextField<_Label>) -> some View { configuration.styled(.roundedBorder) }
}

extension TextFieldStyle where Self == DefaultTextFieldStyle { public static var automatic: Self { .init() } }
extension TextFieldStyle where Self == PlainTextFieldStyle { public static var plain: Self { .init() } }
extension TextFieldStyle where Self == BorderedTextFieldStyle { public static var bordered: Self { .init() } }
extension TextFieldStyle where Self == RoundedBorderTextFieldStyle { public static var roundedBorder: Self { .init() } }

public struct AutomaticTextEditorStyle: TextEditorStyle {
    public init() {}
    public func makeBody(configuration: Configuration) -> some View {
        var input = configuration.editor.input
        input.style = .automatic
        return TextEditor(input: input)
    }
}

public struct PlainTextEditorStyle: TextEditorStyle {
    public init() {}
    public func makeBody(configuration: Configuration) -> some View {
        var input = configuration.editor.input
        input.style = .plain
        return TextEditor(input: input)
    }
}

extension TextEditorStyle where Self == AutomaticTextEditorStyle { public static var automatic: Self { .init() } }
extension TextEditorStyle where Self == PlainTextEditorStyle { public static var plain: Self { .init() } }
