import SwiftUI

private struct PlainTestFieldStyle: TextFieldStyle {
    func _body(configuration: TextField<_Label>) -> some View {
        configuration.textFieldStyle(.plain).padding(2)
    }
}

struct TextInputLab: View {
    enum Field: Hashable { case name, fullName, password, notes }
    @FocusState private var focus: Field?
    @FocusState private var other: Bool
    @State private var name = ""
    @State private var password = ""
    @State private var notes = ""
    @State private var submission = ""
    @State private var showPassword = true

    var body: some View {
        VStack {
            TextField("Name", text: $name, prompt: Text("Full name").bold())
                .textFieldStyle(.roundedBorder)
                .keyboardType(.emailAddress)
                .textInputAutocapitalization(TextInputAutocapitalization.never)
                .autocorrectionDisabled()
                .submitLabel(.next)
                .focused($focus, equals: .name)
                .focused($focus, equals: .fullName)
                .onSubmit { submission += "inner "; focus = .password }
                .onSubmit(of: .search) { submission += "search " }
            if showPassword {
                SecureField("Password", text: $password, prompt: Text("Enter password"))
                    .focused($focus, equals: .password)
                    .textContentType(.password)
                    .submitScope(false)
            }
            TextEditor(text: $notes)
                .textEditorStyle(.plain)
                .focused($focus, equals: .notes)
                .submitScope()
            TextField("Other", text: .constant(""))
                .textFieldStyle(PlainTestFieldStyle())
                .focused($other)
                .submitScope()
            TextField("Disabled", text: .constant(""))
                .textFieldStyle(.bordered)
                .textInputBorderShape(.roundedRectangle)
                .disabled(true)
            HStack {
                Button("Focus name") { focus = .name }
                Button("Focus full name") { focus = .fullName }
                Button("Hide password") { showPassword = false }
            }
            Text(submission)
            Text(focus == nil ? "No enum focus" : "Enum focus")
        }
        .onSubmit { submission += "outer " }
    }
}
