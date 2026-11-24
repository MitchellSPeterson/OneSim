import SwiftUI

// Date and color pickers: DatePicker (compact, graphical, wheel; date, time and both), MultiDatePicker and
// ColorPicker. Builds unchanged against Apple's SwiftUI (scripts/build-ios-sim.sh); the reference states are in
// tests/fixtures/ios27/swiftui-pickers-ios/ (iOS 27.2) and tests/fixtures/swiftui-pickers-ios/ (iOS 26.5).
// Every date is in GMT (`\.timeZone`), so the screens look the same in any host time zone.

@main
struct PickersApp: App {
    var body: some Scene {
        WindowGroup { PickersView() }
    }
}

let gmt = TimeZone(secondsFromGMT: 0)!
/// 2025-03-14 14:30 GMT (a Friday).
let friday = Date(timeIntervalSince1970: 1_741_962_600)

struct PickersView: View {
    @State private var start = friday
    @State private var due = friday
    @State private var tint = Color.blue
    @State private var paper = Color(red: 1, green: 0.8, blue: 0)

    var body: some View {
        NavigationStack {
            Form {
                Section("Compact") {
                    DatePicker("Start", selection: $start)
                    DatePicker("Due", selection: $due, displayedComponents: .date)
                    DatePicker("Alarm", selection: $start, displayedComponents: .hourAndMinute)
                }
                Section("Colors") {
                    ColorPicker("Tint", selection: $tint)
                    ColorPicker("Paper", selection: $paper, supportsOpacity: false)
                }
                Section {
                    NavigationLink("Graphical") { GraphicalScreen() }
                    NavigationLink("Wheel") { WheelScreen() }
                    NavigationLink("Multiple") { MultiScreen() }
                }
            }
            .navigationTitle("Pickers")
        }
        .environment(\.timeZone, gmt)
    }
}

struct GraphicalScreen: View {
    @State private var day = friday

    var body: some View {
        VStack {
            DatePicker("Day", selection: $day, in: ...Date(timeIntervalSince1970: 1_742_774_400),
                       displayedComponents: [.date, .hourAndMinute])
                .datePickerStyle(.graphical)
            Spacer()
        }
        .padding(.horizontal, 16)
        .navigationTitle("Graphical")
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct WheelScreen: View {
    @State private var when = friday
    @State private var birthday = friday

    var body: some View {
        VStack(spacing: 24) {
            DatePicker("When", selection: $when)
                .datePickerStyle(.wheel)
                .labelsHidden()
            DatePicker("Birthday", selection: $birthday, displayedComponents: .date)
                .datePickerStyle(.wheel)
                .labelsHidden()
            Spacer()
        }
        .navigationTitle("Wheel")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// March 1-31 2025 (GMT): the multi-date picker shows this month whatever today is.
let march = Date(timeIntervalSince1970: 1_740_787_200)..<Date(timeIntervalSince1970: 1_743_422_400)

struct MultiScreen: View {
    @State private var days: Set<DateComponents> = [
        DateComponents(year: 2025, month: 3, day: 3),
        DateComponents(year: 2025, month: 3, day: 14),
        DateComponents(year: 2025, month: 3, day: 15),
    ]

    var body: some View {
        Form {
            Section("Days off") {
                MultiDatePicker("Days", selection: $days, in: march)
                Text("\(days.count) days")
            }
        }
        .navigationTitle("Multiple")
        .navigationBarTitleDisplayMode(.inline)
    }
}
