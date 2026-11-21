import SwiftUI

struct PickerLab: View {
    private static let start = Date(timeIntervalSince1970: 1_741_962_600)
    private static let april = Date(timeIntervalSince1970: 1_743_465_600)
    private let gmt = TimeZone(secondsFromGMT: 0)!

    @State private var time = start
    @State private var days: Set<DateComponents> = []
    @State private var spring = Date(timeIntervalSince1970: 1_741_429_800)
    @State private var ranged = start
    @State private var futureBounds = true

    private var range: ClosedRange<Date> {
        if futureBounds {
            return Date(timeIntervalSince1970: 1_898_899_200)...Date(timeIntervalSince1970: 1_902_873_600)
        }
        return Date(timeIntervalSince1970: 1_578_182_400)...Date(timeIntervalSince1970: 1_579_478_400)
    }

    var body: some View {
        VStack {
            DatePicker("Bounded time", selection: $time, in: Self.start...Self.start.addingTimeInterval(40 * 60),
                       displayedComponents: .hourAndMinute)
                .datePickerStyle(DelegatingWheel())
            MultiDatePicker("Before April", selection: $days, in: ..<Self.april)
            DatePicker("Spring forward", selection: $spring)
                .environment(\.timeZone, TimeZone(identifier: "America/Los_Angeles")!)
            DatePicker("Ranged calendar", selection: $ranged, in: range, displayedComponents: .date)
                .datePickerStyle(.graphical)
            DatePicker("Ranged compact", selection: $ranged, in: range, displayedComponents: .date)
            Button("Change range") { futureBounds = false }
        }
        .environment(\.timeZone, gmt)
    }
}

struct DelegatingWheel: DatePickerStyle {
    func makeBody(configuration: Configuration) -> some View {
        WheelDatePickerStyle().makeBody(configuration: configuration)
    }
}
