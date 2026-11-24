import SwiftUI

@main
struct KeypadApp: App {
    var body: some Scene { WindowGroup { KeypadView() } }
}

private struct KeypadView: View {
    @State private var number = ""
    @State private var decimal = ""
    @State private var phone = ""

    var body: some View {
        VStack(spacing: 16) {
            TextField("Number", text: $number).keyboardType(.numberPad)
            TextField("Decimal", text: $decimal).keyboardType(.decimalPad)
            TextField("Phone", text: $phone).keyboardType(.phonePad)
            Spacer()
        }
        .textFieldStyle(.roundedBorder)
        .padding(24)
    }
}
