import SwiftUI

@main
struct TextInputApp: App {
    var body: some Scene { WindowGroup { TextInputView() } }
}

struct TextInputView: View {
    @State private var name = ""
    @State private var password = "sample"
    @State private var notes = "First line\nSecond line"
    @FocusState private var focus: Field?

    private enum Field: Hashable {
        case name
        case password
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            TextField("Name", text: $name, prompt: Text("Full name"))
                .textFieldStyle(.roundedBorder)
                .focused($focus, equals: .name)
                .submitLabel(.next)
                .onSubmit { focus = .password }

            SecureField("Password", text: $password)
                .textFieldStyle(.plain)
                .focused($focus, equals: .password)
                .submitLabel(.done)
                .onSubmit { focus = nil }

            TextEditor(text: $notes)
                .frame(height: 80)
                .overlay { RoundedRectangle(cornerRadius: 6).strokeBorder(.gray) }

            Spacer()
        }
        .padding(24)
    }
}
