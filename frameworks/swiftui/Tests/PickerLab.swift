import SwiftUI

struct PickerLab: View {
    private static let start = Date(timeIntervalSince1970: 1_741_962_600)
    private static let april = Date(timeIntervalSince1970: 1_743_465_600)
    private let gmt = TimeZone(secondsFromGMT: 0)!

    @State private var time = start
    @State private var days: Set<DateComponents> = []
    @State private var spring = Date(timeIntervalSince1970: 1_741_429_800)

    var body: some View {
        VStack {
            DatePicker("Bounded time", selection: $time, in: Self.start...Self.start.addingTimeInterval(40 * 60),
                       displayedComponents: .hourAndMinute)
                .datePickerStyle(DelegatingWheel())
            MultiDatePicker("Before April", selection: $days, in: ..<Self.april)
            DatePicker("Spring forward", selection: $spring)
                .environment(\.timeZone, TimeZone(identifier: "America/Los_Angeles")!)
        }
        .environment(\.timeZone, gmt)
    }
}

struct DelegatingWheel: DatePickerStyle {
    func makeBody(configuration: Configuration) -> some View {
        WheelDatePickerStyle().makeBody(configuration: configuration)
    }
}
