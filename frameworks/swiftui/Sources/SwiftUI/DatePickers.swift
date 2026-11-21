#if os(WASI)
import WASILibc
#else
import Foundation
#endif

// DatePicker (compact, graphical, wheel), MultiDatePicker, ColorPicker, and the `\.calendar`, `\.timeZone` and
// `\.locale` environment values they read (docs/protocol.md section S). The pickers are built from ordinary nodes
// here; the host adds a `popover` layer and a multi-column `wheel` kind. Dates are Gregorian; days, weekdays and
// wall-clock times come from `_Civil` in the environment's time zone.

// MARK: - Environment

struct _TimeZoneKey: EnvironmentKey { static var defaultValue: TimeZone { TimeZone.current } }
struct _CalendarKey: EnvironmentKey { static var defaultValue: Calendar { Calendar.current } }
struct _LocaleKey: EnvironmentKey { static var defaultValue: Locale { Locale.current } }
extension EnvironmentValues {
    public var timeZone: TimeZone { get { self[_TimeZoneKey.self] } set { self[_TimeZoneKey.self] = newValue } }
    public var calendar: Calendar { get { self[_CalendarKey.self] } set { self[_CalendarKey.self] = newValue } }
    public var locale: Locale { get { self[_LocaleKey.self] } set { self[_LocaleKey.self] = newValue } }
}

// MARK: - Civil dates

/// Proleptic Gregorian day arithmetic (days since 1970-01-01), independent of Foundation so WASI behaves the same.
enum _Civil {
    static func days(_ y: Int, _ m: Int, _ d: Int) -> Int {
        let y2 = m <= 2 ? y - 1 : y
        let era = (y2 >= 0 ? y2 : y2 - 399) / 400
        let yoe = y2 - era * 400
        let mp = (m + 9) % 12
        let doy = (153 * mp + 2) / 5 + d - 1
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy
        return era * 146097 + doe - 719468
    }
    static func ymd(_ z0: Int) -> (y: Int, m: Int, d: Int) {
        let z = z0 + 719468
        let era = (z >= 0 ? z : z - 146096) / 146097
        let doe = z - era * 146097
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
        let mp = (5 * doy + 2) / 153
        let d = doy - (153 * mp + 2) / 5 + 1
        let m = mp < 10 ? mp + 3 : mp - 9
        return (yoe + era * 400 + (m <= 2 ? 1 : 0), m, d)
    }
    static func monthDays(_ y: Int, _ m: Int) -> Int { days(m == 12 ? y + 1 : y, m == 12 ? 1 : m + 1, 1) - days(y, m, 1) }
    /// 0 = Sunday ... 6 = Saturday (1970-01-01 was a Thursday).
    static func weekday(_ day: Int) -> Int { ((day % 7) + 7 + 4) % 7 }
    /// (year, month) moved by `n` months.
    static func addMonths(_ y: Int, _ m: Int, _ n: Int) -> (y: Int, m: Int) {
        let t = y * 12 + (m - 1) + n
        let y2 = t >= 0 ? t / 12 : (t - 11) / 12
        return (y2, t - y2 * 12 + 1)
    }
}

/// A date's wall clock in a time zone: the day number and the seconds into it.
struct _Wall: Equatable {
    var day: Int
    var seconds: Double   // 0 ..< 86400 (a little more across a DST fall-back is folded into the day)
    var ymd: (y: Int, m: Int, d: Int) { _Civil.ymd(day) }
    var hour: Int { Int(seconds) / 3600 }
    var minute: Int { Int(seconds) / 60 % 60 }

    init(day: Int, seconds: Double) { self.day = day; self.seconds = seconds }
    init(_ date: Date, _ tz: TimeZone) {
        let timestamp = date.timeIntervalSince1970
        guard timestamp.isFinite else {
            self.init(day: 0, seconds: 0)
            return
        }
        let local = timestamp + Double(tz.secondsFromGMT(for: date))
        // Leave space for civil-date arithmetic on either side of the displayed day.
        let extreme = Int.max / 2
        let d = (local / 86400).rounded(.down)
        let day = Int(min(Double(extreme), max(-Double(extreme), d)))
        let remainder = local.truncatingRemainder(dividingBy: 86400)
        self.init(day: day, seconds: remainder < 0 ? remainder + 86400 : remainder)
    }
    /// The instant showing this wall clock (the earlier one when DST repeats it; a skipped time moves forward).
    func date(_ tz: TimeZone) -> Date {
        let local = Double(day) * 86400 + seconds
        let naive = Date(timeIntervalSince1970: local)
        #if os(WASI)
        return naive.addingTimeInterval(-Double(tz.secondsFromGMT(for: naive)))
        #else
        // Sample both sides of any transition near the wall time. A repeated time has
        // two exact candidates. A skipped time has one candidate on either side of the gap.
        let offsets = Set([-86400.0, 0, 86400.0].map { tz.secondsFromGMT(for: naive.addingTimeInterval($0)) })
        let candidates = offsets.map { naive.addingTimeInterval(-Double($0)) }
        let exact = candidates.filter { _Wall($0, tz) == self }
        return exact.min() ?? candidates.max() ?? naive
        #endif
    }
}

/// Bounds of a date picker. Multi-date pickers can exclude the upper endpoint.
struct _DateBounds {
    var lo: Date?, hi: Date?
    var excludesUpper = false
    func clamp(_ d: Date) -> Date {
        var d = d
        if let lo, d < lo { d = lo }
        if let hi, d > hi { d = hi }
        return d
    }
    func allows(instant: Date) -> Bool {
        if let lo, instant < lo { return false }
        if let hi, excludesUpper ? instant >= hi : instant > hi { return false }
        return true
    }
    /// Whether any instant of the wall-clock day is inside the bounds.
    func allows(day: Int, _ tz: TimeZone) -> Bool {
        if let lo, day < _Wall(lo, tz).day { return false }
        if excludesUpper, let lo, let hi, lo >= hi { return false }
        if let hi {
            let upper = _Wall(hi, tz)
            if day > upper.day { return false }
            if excludesUpper && day == upper.day && upper.seconds == 0 { return false }
        }
        return true
    }
    func calendarMonth(containing day: Int, _ tz: TimeZone) -> (y: Int, m: Int) {
        var day = day
        if let lo { day = max(day, _Wall(lo, tz).day) }
        if let hi {
            let upper = _Wall(hi, tz)
            let lastDay = excludesUpper && upper.seconds == 0 ? upper.day - 1 : upper.day
            day = min(day, lastDay)
        }
        let date = _Civil.ymd(day)
        return (date.y, date.m)
    }
    func allows(month y: Int, _ m: Int, _ tz: TimeZone) -> Bool {
        let first = _Civil.days(y, m, 1), last = first + _Civil.monthDays(y, m) - 1
        if let lo, last < _Wall(lo, tz).day { return false }
        return allows(day: max(first, lo.map { _Wall($0, tz).day } ?? first), tz)
    }
}

// MARK: - Formatting

/// Strings the pickers show. US English is built in (and is all WASI has); other locales use `DateFormatter`
/// with the environment's locale, calendar and time zone.
struct _DateText {
    static let months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
                         "November", "December"]
    static let weekdays = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
    let locale: Locale
    var builtIn: Bool { ["", "en", "en_US", "en_US_POSIX", "en-US"].contains(locale.identifier) }

    init(_ env: EnvironmentValues) { locale = env.locale }

    #if !os(WASI)
    func format(_ template: String, _ y: Int, _ m: Int, _ d: Int, _ h: Int = 0, _ mi: Int = 0) -> String {
        let f = DateFormatter()
        f.locale = locale
        f.calendar = Calendar(identifier: .gregorian)
        f.timeZone = TimeZone(secondsFromGMT: 0)
        f.setLocalizedDateFormatFromTemplate(template)
        let t = Double(_Civil.days(y, m, d)) * 86400 + Double(h * 3600 + mi * 60)
        return f.string(from: Date(timeIntervalSince1970: t))
    }
    #endif

    var hour12: Bool {
        #if !os(WASI)
        if !builtIn { return (DateFormatter.dateFormat(fromTemplate: "j", options: 0, locale: locale) ?? "h a").contains("a") }
        #endif
        return true
    }
    func monthName(_ m: Int) -> String {
        #if !os(WASI)
        if !builtIn { return format("LLLL", 2001, m, 1) }
        #endif
        return Self.months[m - 1]
    }
    /// "Mar 14, 2025"
    func date(_ y: Int, _ m: Int, _ d: Int) -> String {
        #if !os(WASI)
        if !builtIn { return format("yMMMd", y, m, d) }
        #endif
        return "\(String(Self.months[m - 1].prefix(3))) \(d), \(y)"
    }
    /// "2:30 PM" (a narrow no-break space before the period, as iOS) or "14:30".
    func time(_ h: Int, _ mi: Int) -> String {
        #if !os(WASI)
        if !builtIn { return format("jmm", 2001, 1, 1, h, mi) }
        #endif
        return "\(h % 12 == 0 ? 12 : h % 12):\(_pad2(mi))\u{202F}\(h < 12 ? "AM" : "PM")"
    }
    /// "March 2025"
    func monthYear(_ y: Int, _ m: Int) -> String {
        #if !os(WASI)
        if !builtIn { return format("yMMMM", y, m, 1) }
        #endif
        return "\(Self.months[m - 1]) \(y)"
    }
    /// Weekday column titles from Sunday, upper-cased ("SUN").
    func weekdayHeaders() -> [String] {
        #if !os(WASI)
        if !builtIn { return (0..<7).map { format("EEE", 2001, 1, 7 + $0).uppercased() } }   // 2001-01-07 was a Sunday
        #endif
        return Self.weekdays.map { $0.uppercased() }
    }
    /// The day column of a date-and-time wheel: "Fri Mar 14", or "Today".
    func wheelDay(_ day: Int, today: Int) -> String {
        if day == today { return "Today" }
        let (y, m, d) = _Civil.ymd(day)
        #if !os(WASI)
        if !builtIn { return format("EEEMMMd", y, m, d) }
        #endif
        return "\(Self.weekdays[_Civil.weekday(day)]) \(String(Self.months[m - 1].prefix(3))) \(d)"
    }
}

/// Two digits ("07").
func _pad2(_ n: Int) -> String { n < 10 ? "0\(n)" : "\(n)" }

/// The weekday the environment's calendar starts its weeks on (0 = Sunday).
func _firstWeekday(_ env: EnvironmentValues) -> Int { (env.calendar.firstWeekday - 1 + 7) % 7 }

// MARK: - Components and styles

public struct DatePickerComponents: OptionSet, Hashable, Sendable {
    public let rawValue: UInt
    public init(rawValue: UInt) { self.rawValue = rawValue }
    public static let hourAndMinute = DatePickerComponents(rawValue: 1 << 2)
    public static let date = DatePickerComponents(rawValue: 1 << 4)
}

public struct DatePickerStyleConfiguration {
    public struct Label: View {
        let make: _StyleLabel
        public var body: some View { make }
    }
    public let label: Label
    @Binding public var selection: Date
    public var minimumDate: Date?
    public var maximumDate: Date?
    public var displayedComponents: DatePickerComponents
}

public protocol DatePickerStyle {
    associatedtype Body: View
    typealias Configuration = DatePickerStyleConfiguration
    @ViewBuilder func makeBody(configuration: Configuration) -> Body
    var _styleName: String? { get }
}
extension DatePickerStyle { public var _styleName: String? { nil } }

public struct DefaultDatePickerStyle: DatePickerStyle {
    public init() {}
    public var _styleName: String? { "compact" }
    public func makeBody(configuration: Configuration) -> some View { DatePicker(configuration).datePickerStyle(.automatic) }
}
public struct CompactDatePickerStyle: DatePickerStyle {
    public init() {}
    public var _styleName: String? { "compact" }
    public func makeBody(configuration: Configuration) -> some View { DatePicker(configuration).datePickerStyle(.compact) }
}
public struct GraphicalDatePickerStyle: DatePickerStyle {
    public init() {}
    public var _styleName: String? { "graphical" }
    public func makeBody(configuration: Configuration) -> some View { DatePicker(configuration).datePickerStyle(.graphical) }
}
public struct WheelDatePickerStyle: DatePickerStyle {
    public init() {}
    public var _styleName: String? { "wheel" }
    public func makeBody(configuration: Configuration) -> some View { DatePicker(configuration).datePickerStyle(.wheel) }
}
extension DatePickerStyle where Self == DefaultDatePickerStyle { public static var automatic: DefaultDatePickerStyle { .init() } }
extension DatePickerStyle where Self == CompactDatePickerStyle { public static var compact: CompactDatePickerStyle { .init() } }
extension DatePickerStyle where Self == GraphicalDatePickerStyle { public static var graphical: GraphicalDatePickerStyle { .init() } }
extension DatePickerStyle where Self == WheelDatePickerStyle { public static var wheel: WheelDatePickerStyle { .init() } }

extension View {
    public func datePickerStyle<S: DatePickerStyle>(_ style: S) -> some View {
        _CtxView(content: self) {
            $0.styles.datePicker = _named(style._styleName) { cfg, p in _eval(style.makeBody(configuration: cfg), p) }
        }
    }
}

// MARK: - Look (iOS 27.2 iPhone 18 Pro, tests/fixtures/ios27/swiftui-pickers-ios)

enum _DP {
    /// Compact field: a capsule of tertiary fill around the 17 pt value.
    static let pillH = 36.0, pillDateOnlyH = 34.33, pillPadH = 12.0, pillGap = 4.0, pillTracking = -0.2
    static let pillFill = Color(json: ["r": 239.0 / 255, "g": 239.0 / 255, "b": 240.0 / 255, "a": 1.0, "dark": [44.0 / 255, 44.0 / 255, 46.0 / 255]])
    /// Popover under a compact field: its top sits this far below the field.
    static let popoverGap = 8.67
    /// Calendar: header row (top inset, height), weekday row height, the day circle never larger than this.
    static let headerTop = 16.0, headerH = 37.67, weekdayH = 32.67, weekdayTop = 15.0, weekdayTextH = 17.67, maxCircle = 44.0
    /// Days outside the bounds (quaternary label).
    static let outOfRange = Color(json: ["r": 60.0 / 255, "g": 60.0 / 255, "b": 67.0 / 255, "a": 0.18, "dark": [235.0 / 255, 235.0 / 255, 245.0 / 255]])
    static let navW = 43.33
    static let dayPt = 20.0, headerPt = 17.0, weekdayPt = 13.0
    /// Graphical: the calendar starts this far in on the leading side; rows are this tall. Time row below.
    static let graphicalLead = 8.0, graphicalRowH = 43.0, graphicalRows = 6
    /// A calendar in a List / Form row (MultiDatePicker): week height and padding under the last week.
    static let listRowH = 39.0, listBottom = 19.67
    static let timeRowGap = 5.67, timeRowH = 38.0
    /// Compact date popover: 320 pt card, grid inset, row height, bottom padding.
    static let popoverW = 320.0, popoverInset = 10.33, popoverRowH = 38.0, popoverBottom = 17.67, popoverHeaderX = 20.0
    /// Time popover: the wheel's size inside its card, and the card's padding.
    static let timeWheel = (w: 218.0, h: 172.0), timePad = (h: 7.0, v: 16.0)
    /// Wheel style: 320 x 216, columns 5 pt apart.
    static let wheelW = 320.0, wheelH = 216.0, wheelGap = 5.0
    static let dimmed = Color(json: ["r": 0.0, "g": 0.0, "b": 0.0, "a": 0.25, "dark": [1.0, 1.0, 1.0]])
}

// MARK: - DatePicker

final class _DatePickerState {
    enum Open { case none, date, time }
    var open = Open.none
    var month: (y: Int, m: Int)?   // the month the calendar shows (nil: the selection's)
    var yearPicker = false         // the header's month / year wheel instead of the days
}

/// Per-picker state that survives re-evaluation (popover, shown month).
func _slot<T: AnyObject>(_ key: String, _ make: () -> T) -> T {
    let rt = _Runtime.shared
    rt.touched.insert(key)
    if let s = rt.slots[key] as? T { return s }
    let s = make()
    rt.slots[key] = s
    return s
}

/// Styles: `.compact` (also `.automatic`): the label, then capsule fields that open the calendar or a time wheel in a
/// popover; `.graphical`: the month calendar (and a time field); `.wheel`: one drum per component.
public struct DatePicker<Label: View>: View, _Flattenable {
    public typealias Body = Never
    public typealias Components = DatePickerComponents
    public var body: Never { fatalError() }
    let selection: Binding<Date>, bounds: _DateBounds, components: DatePickerComponents, label: Label

    init(_ selection: Binding<Date>, _ bounds: _DateBounds, _ components: DatePickerComponents, _ label: Label) {
        self.selection = selection; self.bounds = bounds
        self.components = components.isEmpty ? [.date, .hourAndMinute] : components
        self.label = label
    }
    public init(selection: Binding<Date>, displayedComponents: DatePickerComponents = [.hourAndMinute, .date], @ViewBuilder label: () -> Label) {
        self.init(selection, _DateBounds(), displayedComponents, label())
    }
    public init(selection: Binding<Date>, in range: ClosedRange<Date>, displayedComponents: DatePickerComponents = [.hourAndMinute, .date],
                @ViewBuilder label: () -> Label) {
        self.init(selection, _DateBounds(lo: range.lowerBound, hi: range.upperBound), displayedComponents, label())
    }
    public init(selection: Binding<Date>, in range: PartialRangeFrom<Date>, displayedComponents: DatePickerComponents = [.hourAndMinute, .date],
                @ViewBuilder label: () -> Label) {
        self.init(selection, _DateBounds(lo: range.lowerBound), displayedComponents, label())
    }
    public init(selection: Binding<Date>, in range: PartialRangeThrough<Date>, displayedComponents: DatePickerComponents = [.hourAndMinute, .date],
                @ViewBuilder label: () -> Label) {
        self.init(selection, _DateBounds(hi: range.upperBound), displayedComponents, label())
    }

    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let style = rt.styles.datePicker
        if case .custom(let f)? = style {
            let cfg = DatePickerStyleConfiguration(label: .init(make: _styleLabel(label)), selection: selection,
                                                   minimumDate: bounds.lo, maximumDate: bounds.hi, displayedComponents: components)
            return [_without({ $0.datePicker = nil }) { f(cfg, path) }]
        }
        var name = "compact"
        if case .name(let n)? = style { name = n }
        let env = EnvironmentValues()
        let st = _slot("\(path)#DatePicker") { _DatePickerState() }
        if !rt.pageIsActive { st.open = .none }
        let ctx = _DPContext(selection: selection, bounds: bounds, env: env, state: st, disabled: rt.disabled)
        let labelNode: Node? = rt.styles.labelsHidden ? nil : _optionalNode(label, path + "/0")
        switch name {
        case "graphical":
            return [_graphical(ctx, components, path)]
        case "wheel":
            let wheel = _dateWheel(ctx, components, path + "/w", width: _DP.wheelW, height: _DP.wheelH)
            guard let labelNode else { return [wheel] }
            // A wheel with a visible label: the label leading, the drums trailing (as `LabeledContent`).
            return [["kind": "hstack", "id": path, "spacing": NSNull(), "alignment": "center",
                     "children": [labelNode, ["kind": "spacer", "id": path + "/s", "minLength": NSNull()] as Node, wheel]] as Node]
        default:
            return [_compact(ctx, components, labelNode, path)]
        }
    }
}

extension DatePicker where Label == Text {
    public init<S: StringProtocol>(_ title: S, selection: Binding<Date>, displayedComponents: DatePickerComponents = [.hourAndMinute, .date]) {
        self.init(selection: selection, displayedComponents: displayedComponents) { Text(String(title)) }
    }
    public init<S: StringProtocol>(_ title: S, selection: Binding<Date>, in range: ClosedRange<Date>,
                displayedComponents: DatePickerComponents = [.hourAndMinute, .date]) {
        self.init(selection: selection, in: range, displayedComponents: displayedComponents) { Text(String(title)) }
    }
    public init<S: StringProtocol>(_ title: S, selection: Binding<Date>, in range: PartialRangeFrom<Date>,
                displayedComponents: DatePickerComponents = [.hourAndMinute, .date]) {
        self.init(selection: selection, in: range, displayedComponents: displayedComponents) { Text(String(title)) }
    }
    public init<S: StringProtocol>(_ title: S, selection: Binding<Date>, in range: PartialRangeThrough<Date>,
                displayedComponents: DatePickerComponents = [.hourAndMinute, .date]) {
        self.init(selection: selection, in: range, displayedComponents: displayedComponents) { Text(String(title)) }
    }
}

extension DatePicker where Label == DatePickerStyleConfiguration.Label {
    /// The built-in look, for use inside a custom style's `makeBody`.
    public init(_ configuration: DatePickerStyleConfiguration) {
        self.init(configuration.$selection, _DateBounds(lo: configuration.minimumDate, hi: configuration.maximumDate),
                  configuration.displayedComponents, configuration.label)
    }
}

/// What the picker parts share: the binding, bounds and environment of one evaluation.
struct _DPContext {
    let selection: Binding<Date>, bounds: _DateBounds, env: EnvironmentValues, state: _DatePickerState, disabled: Bool
    var tz: TimeZone { env.timeZone }
    var text: _DateText { _DateText(env) }
    var wall: _Wall { _Wall(selection.wrappedValue, tz) }
    /// Today in the picker's time zone (the session clock, `SIMCORE_TIME` when set).
    var today: Int { _Wall(Date(timeIntervalSince1970: _Runtime.shared.currentDate.timeIntervalSince1970), tz).day }
    /// Set the selection from a wall clock, kept inside the bounds.
    func set(_ w: _Wall) {
        selection.wrappedValue = bounds.clamp(w.date(tz))
        _Runtime.shared.dirty = true
    }
    func action(_ f: @escaping () -> Void) -> UInt64 { disabled ? 0 : _Runtime.shared.registerAction(f) }
}

func _text(_ id: String, _ s: String, _ pt: Double, _ weight: String? = nil, _ color: Color? = nil) -> Node {
    var mods: [Node] = [["font": ["size": pt, "weight": weight ?? "regular"] as Node]]
    if let color { mods.append(["foregroundColor": color.json]) }
    return ["kind": "text", "id": id, "text": s, "modifiers": mods]
}

func _frame(_ n: Node, w: Double?, h: Double?, align: String = "center") -> Node {
    var n = n
    var mods = (n["modifiers"] as? [Node]) ?? []
    var f: Node = ["alignment": align]
    if let w { f["width"] = w }
    if let h { f["height"] = h }
    mods.append(["frame": f])
    n["modifiers"] = mods
    return n
}

func _plainButton(_ id: String, _ action: UInt64, _ label: Node, disabled: Bool = false, value: String? = nil) -> Node {
    var n: Node = ["kind": "button", "id": id, "style": "plain", "action": action, "disabled": disabled || action == 0, "label": label]
    if let value { n["value"] = value }
    return n
}

/// A capsule field showing `text` (accent while its popover is open).
func _pill(_ id: String, _ text: String, open: Bool, _ action: UInt64, disabled: Bool, h: Double) -> Node {
    var label = _text(id + "/t", text, 17, nil, open ? Color.accentColor : (disabled ? Color(json: ["semantic": "tertiary"]) : nil))
    label["modifiers"] = (label["modifiers"] as! [Node]) + [
        ["tracking": _DP.pillTracking],
        ["padding": ["leading": _DP.pillPadH, "trailing": _DP.pillPadH, "top": 0.0, "bottom": 0.0] as Node],
        ["frame": ["height": h, "alignment": "center"] as Node],
        ["background": ["kind": "shape", "id": id + "/bg", "shape": "capsule", "fill": _DP.pillFill.json] as Node],
    ]
    return _plainButton(id, action, label, disabled: disabled)
}

// MARK: Compact

func _compact(_ ctx: _DPContext, _ components: DatePickerComponents, _ label: Node?, _ path: String) -> Node {
    let rt = _Runtime.shared, st = ctx.state, w = ctx.wall, (y, m, d) = w.ymd
    var pills: [Node] = []
    if components.contains(.date) {
        let id = path + "/date"
        pills.append(_pill(id, ctx.text.date(y, m, d), open: st.open == .date,
                           ctx.action { st.open = st.open == .date ? .none : .date; st.yearPicker = false; st.month = nil; rt.dirty = true },
                           disabled: ctx.disabled, h: components.contains(.hourAndMinute) ? _DP.pillH : _DP.pillDateOnlyH))
        if st.open == .date {
            let width = min(_DP.popoverW, max(0, rt.viewport.width - 32))
            let card = _calendar(ctx, path + "/pop", width: width, inset: _DP.popoverInset, headerX: _DP.popoverHeaderX, navTrailing: 2,
                                 rowH: _DP.popoverRowH, rows: 6, navTint: Color.primary, multi: nil)
            rt.layers.append(["kind": "popover", "anchor": id, "content": _pad(card, bottom: _DP.popoverBottom),
                              "dismiss": rt.registerAction { st.open = .none; rt.dirty = true }])
        }
    }
    if components.contains(.hourAndMinute) {
        let id = path + "/time"
        pills.append(_pill(id, ctx.text.time(w.hour, w.minute), open: st.open == .time,
                           ctx.action { st.open = st.open == .time ? .none : .time; rt.dirty = true }, disabled: ctx.disabled, h: _DP.pillH))
        if st.open == .time {
            let wheel = _dateWheel(ctx, .hourAndMinute, path + "/pop", width: _DP.timeWheel.w, height: _DP.timeWheel.h)
            rt.layers.append(["kind": "popover", "anchor": id,
                              "content": _pad(wheel, h: _DP.timePad.h, v: _DP.timePad.v),
                              "dismiss": rt.registerAction { st.open = .none; rt.dirty = true }])
        }
    }
    let fields: Node = ["kind": "hstack", "id": path + "/f", "spacing": _DP.pillGap, "alignment": "center", "children": pills]
    guard let label else { return fields }
    return ["kind": "hstack", "id": path, "spacing": NSNull(), "alignment": "center",
            "children": [label, ["kind": "spacer", "id": path + "/s", "minLength": NSNull()] as Node, fields]]
}

func _pad(_ n: Node, h: Double = 0, v: Double = 0, bottom: Double? = nil) -> Node {
    var n = n
    n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [["padding": ["leading": h, "trailing": h, "top": v, "bottom": bottom ?? v] as Node]]
    return n
}

// MARK: Calendar (graphical, the compact popover, MultiDatePicker)

/// Day selection for `MultiDatePicker`: whether a day is chosen, and its toggle.
struct _MultiSelection {
    let contains: (Int) -> Bool
    let toggle: (Int) -> Void
}

/// The month calendar: header (month and year, a chevron that opens the month / year wheel, previous and next
/// month), weekday titles and the day grid. `rowH` nil fills the proposed height.
func _calendar(_ ctx: _DPContext, _ path: String, width: Double, inset: Double, headerX: Double, navTrailing: Double, rowH: Double?,
               rows: Int, navTint: Color, multi: _MultiSelection?) -> Node {
    let rt = _Runtime.shared, st = ctx.state, tz = ctx.tz, sel = ctx.wall
    let day = st.month.map { _Civil.days($0.y, $0.m, 1) } ?? sel.day
    let shown = ctx.bounds.calendarMonth(containing: day, tz)
    if st.month != nil { st.month = shown }
    let (y, m) = shown
    let text = ctx.text
    let accent = Color.accentColor
    let dim = Color(json: ["semantic": "tertiary"])
    let prev = _Civil.addMonths(y, m, -1), next = _Civil.addMonths(y, m, 1)
    let canPrev = ctx.bounds.allows(month: prev.y, prev.m, tz), canNext = ctx.bounds.allows(month: next.y, next.m, tz)
    let go = { (to: (y: Int, m: Int)) -> UInt64 in ctx.action { st.month = to; rt.dirty = true } }

    // Header: "March 2025 >" leading; previous / next month trailing.
    let titleColor = st.yearPicker ? accent : Color.primary
    let title: Node = ["kind": "hstack", "id": path + "/h/t", "spacing": 4.0, "alignment": "center", "children": [
        _text(path + "/h/t/0", text.monthYear(y, m), _DP.headerPt, "semibold", titleColor),
        _symbol(path + "/h/t/1", st.yearPicker ? "chevron.down" : "chevron.right", 14, "semibold", accent),
    ] as [Node]]
    let titleButton = _plainButton(path + "/h/title", ctx.action { st.yearPicker.toggle(); rt.dirty = true }, title, value: text.monthYear(y, m))
    func nav(_ id: String, _ symbol: String, _ ok: Bool, _ to: (y: Int, m: Int)) -> Node {
        let tint = ok ? navTint : dim
        let b = _plainButton(path + "/h/" + id, ok ? go(to) : 0, _frame(_symbol(path + "/h/\(id)/i", symbol, 20, "medium", tint),
                                                                        w: _DP.navW, h: _DP.headerH))
        return b
    }
    var header: [Node] = [_pad(titleButton, h: 0, v: 0), ["kind": "spacer", "id": path + "/h/s", "minLength": NSNull()] as Node]
    if !st.yearPicker {
        header += [nav("prev", "chevron.left", canPrev, prev), nav("next", "chevron.right", canNext, next)]
    }
    var headerRow: Node = ["kind": "hstack", "id": path + "/h", "spacing": 0.0, "alignment": "center", "children": header]
    headerRow = _frame(_padEdges(headerRow, leading: headerX, trailing: navTrailing), w: width, h: _DP.headerH)

    let cellW = ((width - 2 * inset) / 7 * 3).rounded(.down) / 3
    var body: [Node] = [_padEdges(headerRow, top: _DP.headerTop)]
    if st.yearPicker {
        body.append(_monthYearWheel(ctx, path + "/ym", shown: shown, width: width - 2 * inset))
    } else {
        let first = _firstWeekday(ctx.env)
        let heads = text.weekdayHeaders()
        let weekdayRow: Node = ["kind": "hstack", "id": path + "/wd", "spacing": 0.0, "alignment": "center",
                                "children": (0..<7).map { i -> Node in
                                    _frame(_text("\(path)/wd/\(i)", heads[(first + i) % 7], _DP.weekdayPt, "semibold",
                                                 Color(json: ["semantic": "secondary"])), w: cellW, h: _DP.weekdayTextH)
                                }]
        body.append(_padEdges(weekdayRow, leading: inset, top: _DP.weekdayTop, bottom: _DP.weekdayH - _DP.weekdayTop - _DP.weekdayTextH))
        let start = _Civil.days(y, m, 1), count = _Civil.monthDays(y, m)
        let lead = (_Civil.weekday(start) - first + 7) % 7
        let weeks = (lead + count + 6) / 7
        var rowsN: [Node] = []
        for wk in 0..<max(weeks, rows) {
            var cells: [Node] = []
            for col in 0..<7 {
                let i = wk * 7 + col - lead
                let id = "\(path)/d/\(wk)/\(col)"
                guard i >= 0, i < count, wk < weeks else {
                    cells.append(_frame(["kind": "empty", "id": id], w: cellW, h: rowH))
                    continue
                }
                let day = start + i
                cells.append(_dayCell(ctx, id, day: day, cellW: cellW, rowH: rowH, multi: multi))
            }
            var row: Node = ["kind": "hstack", "id": "\(path)/d/\(wk)", "spacing": 0.0, "alignment": "center", "children": cells]
            if rowH == nil { row = _frame(row, w: nil, h: nil); row["modifiers"] = [["frame": ["maxHeight": "inf", "alignment": "center"] as Node]] }
            rowsN.append(row)
        }
        var grid: Node = ["kind": "vstack", "id": path + "/d", "spacing": 0.0, "alignment": "leading", "children": rowsN]
        grid = _padEdges(grid, leading: inset)
        body.append(grid)
    }
    var root: Node = ["kind": "vstack", "id": path, "spacing": 0.0, "alignment": "leading", "children": body]
    root = _frame(root, w: width, h: nil, align: "topLeading")
    return root
}

func _padEdges(_ n: Node, leading: Double = 0, trailing: Double = 0, top: Double = 0, bottom: Double = 0) -> Node {
    var n = n
    n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [["padding": ["leading": leading, "trailing": trailing, "top": top, "bottom": bottom] as Node]]
    return n
}

func _symbol(_ id: String, _ name: String, _ pt: Double, _ weight: String, _ color: Color) -> Node {
    ["kind": "image", "id": id, "systemName": name, "resizable": false,
     "modifiers": [["font": ["size": pt, "weight": weight] as Node], ["foregroundColor": color.json]] as [Node]]
}

/// One day: its number, in a filled circle when selected; today in the accent color; out-of-range days dimmed.
func _dayCell(_ ctx: _DPContext, _ id: String, day: Int, cellW: Double, rowH: Double?, multi: _MultiSelection?) -> Node {
    let rt = _Runtime.shared, tz = ctx.tz
    let allowed = ctx.bounds.allows(day: day, tz)
    let selected = multi.map { $0.contains(day) } ?? (day == ctx.wall.day)
    let isToday = day == ctx.today
    let (_, _, d) = _Civil.ymd(day)
    let fg: Color
    if selected { fg = Color(json: ["semantic": "background"]) }
    else if !allowed { fg = _DP.outOfRange }
    else if isToday { fg = Color.accentColor }
    else { fg = Color.primary }
    let num = _text(id + "/n", "\(d)", _DP.dayPt, selected ? "semibold" : "regular", fg)
    var kids: [Node] = []
    if selected {
        let dia = min(cellW, rowH ?? _DP.maxCircle, _DP.maxCircle)
        kids.append(_frame(["kind": "shape", "id": id + "/c", "shape": "circle", "fill": (isToday ? Color.accentColor : Color.primary).json], w: dia, h: dia))
    }
    kids.append(num)
    let label = _frame(["kind": "zstack", "id": id + "/z", "alignment": "center", "children": kids], w: cellW, h: rowH)
    var label2 = label
    if rowH == nil { label2["modifiers"] = [["frame": ["width": cellW, "maxHeight": "inf", "alignment": "center"] as Node]] }
    let action: UInt64 = allowed ? ctx.action {
        if let multi { multi.toggle(day) } else {
            var w = ctx.wall
            w.day = day
            ctx.set(w)
        }
        rt.dirty = true
    } : 0
    return _plainButton(id, action, label2, disabled: !allowed, value: selected ? "selected" : nil)
}

/// The header's month / year chooser: two drums (month name, year).
func _monthYearWheel(_ ctx: _DPContext, _ path: String, shown: (y: Int, m: Int), width: Double) -> Node {
    let rt = _Runtime.shared, st = ctx.state, text = ctx.text, tz = ctx.tz
    func pick(_ y: Int, _ m: Int) -> UInt64 { ctx.action { st.month = (y, m); rt.dirty = true } }
    let months = (-4...4).map { o -> Node in
        let mm = ((shown.m - 1 + o) % 12 + 12) % 12 + 1
        let ok = ctx.bounds.allows(month: shown.y, mm, tz)
        return ["text": text.monthName(mm), "action": o == 0 || !ok ? 0 : pick(shown.y, mm), "disabled": !ok] as Node
    }
    let years = (-4...4).map { o -> Node in
        let yy = shown.y + o
        var mm = shown.m
        var ok = ctx.bounds.allows(month: yy, mm, tz)
        if !ok { for c in 1...12 where ctx.bounds.allows(month: yy, c, tz) { mm = c; ok = true; break } }
        return ["text": "\(yy)", "action": o == 0 || !ok ? 0 : pick(yy, mm), "disabled": !ok] as Node
    }
    return ["kind": "wheel", "id": path, "width": width, "height": _DP.wheelH, "disabled": ctx.disabled, "columns": [
        ["options": months, "selected": 4, "width": 150.0, "align": "center"] as Node,
        ["options": years, "selected": 4, "width": 100.0, "align": "center"] as Node,
    ] as [Node]]
}

func _graphical(_ ctx: _DPContext, _ components: DatePickerComponents, _ path: String) -> Node {
    // The calendar sizes its cells from the column iOS gives it: the screen less the 16 pt margins, less the
    // picker's 8 pt leading inset.
    let rt = _Runtime.shared, st = ctx.state
    let width = (rt.viewport.width - 32) - _DP.graphicalLead
    var kids: [Node] = []
    if components.contains(.date) {
        kids.append(_calendar(ctx, path + "/cal", width: width, inset: 0, headerX: 12.33, navTrailing: -6, rowH: _DP.graphicalRowH,
                              rows: _DP.graphicalRows, navTint: Color.accentColor, multi: nil))
    }
    if components.contains(.hourAndMinute) {
        // "Time" and a field that opens the time wheel in a popover.
        let w = ctx.wall, id = path + "/time"
        let pill = _pill(id, ctx.text.time(w.hour, w.minute), open: st.open == .time,
                         ctx.action { st.open = st.open == .time ? .none : .time; rt.dirty = true }, disabled: ctx.disabled, h: _DP.pillH)
        if st.open == .time {
            let wheel = _dateWheel(ctx, .hourAndMinute, path + "/pop", width: _DP.timeWheel.w, height: _DP.timeWheel.h)
            rt.layers.append(["kind": "popover", "anchor": id, "content": _pad(wheel, h: _DP.timePad.h, v: _DP.timePad.v),
                              "dismiss": rt.registerAction { st.open = .none; rt.dirty = true }])
        }
        var row: Node = ["kind": "hstack", "id": path + "/t", "spacing": 0.0, "alignment": "center", "children": [
            _text(path + "/t/l", "Time", 17, "semibold"), ["kind": "spacer", "id": path + "/t/s", "minLength": NSNull()] as Node, pill,
        ] as [Node]]
        row = _frame(_padEdges(row, leading: 8, trailing: 8), w: width, h: _DP.timeRowH)
        kids.append(_padEdges(row, top: components.contains(.date) ? _DP.timeRowGap : 0))
    }
    return ["kind": "vstack", "id": path, "spacing": 0.0, "alignment": "leading",
            "modifiers": [["padding": ["leading": _DP.graphicalLead, "trailing": 0.0, "top": 0.0, "bottom": 0.0] as Node]] as [Node],
            "children": kids]
}

// MARK: Wheel

/// Drums for the components: day (with weekday) or month / day / year, then hour, minute and AM/PM.
func _dateWheel(_ ctx: _DPContext, _ components: DatePickerComponents, _ path: String, width: Double, height: Double) -> Node {
    let w = ctx.wall, text = ctx.text, tz = ctx.tz
    let span = 4
    var cols: [Node] = []
    var select: [(width: Double, step: (Int) -> _Wall)] = []
    func opt(_ s: String, _ to: _Wall?, exact: Bool = false) -> Node {
        guard let to else { return ["text": s, "action": 0] }
        let ok = exact ? ctx.bounds.allows(instant: to.date(tz)) : ctx.bounds.allows(day: to.day, tz)
        return ["text": s, "action": ok ? ctx.action { ctx.set(to) } : 0, "disabled": !ok] as Node
    }
    func add(_ width: Double, align: String = "center", inset: Double = 0, selectedInset: Double? = nil, exact: Bool = false,
             step: @escaping (Int) -> _Wall, label: (_Wall) -> String) {
        let options = (-span...span).map { offset in
            let value = step(offset)
            return opt(label(value), offset == 0 ? nil : value, exact: exact)
        }
        var col: Node = ["options": options, "selected": span, "width": width, "align": align, "inset": inset]
        if let selectedInset { col["selectedInset"] = selectedInset }
        cols.append(col)
        select.append((width, step))
    }
    func wrap(_ value: Int, _ count: Int) -> Int { ((value % count) + count) % count }
    func shift(_ value: Int, _ delta: Int, limit: Int) -> Int {
        let (sum, overflow) = value.addingReportingOverflow(delta)
        if overflow { return delta < 0 ? -limit : limit }
        return min(limit, max(-limit, sum))
    }
    let popover = width < _DP.wheelW
    if components.contains(.date) && components.contains(.hourAndMinute) {
        let today = ctx.today
        add(135, align: "trailing", inset: 11, selectedInset: 11.33, step: { offset in
            _Wall(day: shift(w.day, offset, limit: Int.max / 2), seconds: w.seconds)
        }, label: { text.wheelDay($0.day, today: today) })
    } else if components.contains(.date) {
        let (y, m, d) = w.ymd
        func with(y y2: Int, m m2: Int, d d2: Int) -> _Wall {
            _Wall(day: _Civil.days(y2, m2, min(d2, _Civil.monthDays(y2, m2))), seconds: w.seconds)
        }
        add(133, align: "leading", inset: 8.33, selectedInset: 1, step: { offset in
            with(y: y, m: wrap(m - 1 + offset % 12, 12) + 1, d: d)
        }, label: { text.monthName($0.ymd.m) })
        let days = _Civil.monthDays(y, m)
        add(49, step: { offset in with(y: y, m: m, d: wrap(d - 1 + offset % days, days) + 1) }, label: { "\($0.ymd.d)" })
        add(75, step: { offset in with(y: shift(y, offset, limit: Int.max / 366), m: m, d: d) }, label: { "\($0.ymd.y)" })
    }
    if components.contains(.hourAndMinute) {
        let h24 = !text.hour12
        let h = w.hour, mi = w.minute
        func at(_ hh: Int, _ mm: Int) -> _Wall {
            _Wall(day: w.day, seconds: Double(hh * 3600 + mm * 60) + w.seconds.truncatingRemainder(dividingBy: 60))
        }
        add(popover ? 55 : 45, align: "trailing", inset: popover ? 28 : 19.67, selectedInset: popover ? 28 : 16.33, exact: true, step: { offset in
            let hour = h24 ? wrap(h + offset % 24, 24) : h / 12 * 12 + wrap(h + offset % 12, 12)
            return at(hour, mi)
        }, label: { value in
            if h24 { return _pad2(value.hour) }
            return "\(value.hour % 12 == 0 ? 12 : value.hour % 12)"
        })
        add(50, exact: true, step: { offset in at(h, wrap(mi + offset % 60, 60)) }, label: { _pad2($0.minute) })
        if !h24 {
            let pm = h >= 12
            let periodWidth: Double = popover ? 65 : 51
            let ampm = [opt("AM", pm ? at(h - 12, mi) : nil, exact: true), opt("PM", pm ? nil : at(h + 12, mi), exact: true)]
            cols.append(["options": ampm, "selected": pm ? 1 : 0, "width": periodWidth, "align": "center"])
            select.append((periodWidth, { offset in
                let period = offset == 0 ? h / 12 : (offset > 0 ? 1 : 0)
                return at(h % 12 + period * 12, mi)
            }))
        }
    }
    let node: Node = ["kind": "wheel", "id": path, "width": width, "height": height, "disabled": ctx.disabled,
                      "columns": cols, "popover": popover]
    let rt = _Runtime.shared
    guard !ctx.disabled, !rt.gesturesOff, !select.isEmpty else { return node }
    let total = select.reduce(0) { $0 + $1.width } + Double(select.count - 1) * _DP.wheelGap
    let drag = DragGesture().onEnded { value in
        guard let steps = Int(exactly: (-Double(value.translation.height) / 32).rounded()), steps != 0 else { return }
        var x = Double(value.startLocation.x) - (width - total) / 2
        for column in select {
            let hitWidth = column.width + _DP.wheelGap
            if x < hitWidth {
                if x >= 0 { ctx.set(column.step(steps)) }
                break
            }
            x -= hitWidth
        }
    }
    return _attachGesture([node], path, .normal) { drag._recognizer($0) }[0]
}

// MARK: - MultiDatePicker

/// A month calendar choosing any number of days. The selection is `DateComponents` with era, year, month and day
/// in the environment's calendar; a tap adds or removes a day.
public struct MultiDatePicker<Label: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let selection: Binding<Set<DateComponents>>, bounds: _DateBounds, label: Label

    init(_ selection: Binding<Set<DateComponents>>, _ bounds: _DateBounds, _ label: Label) {
        self.selection = selection; self.bounds = bounds; self.label = label
    }
    public init(selection: Binding<Set<DateComponents>>, @ViewBuilder label: () -> Label) {
        self.init(selection, _DateBounds(), label())
    }
    public init(selection: Binding<Set<DateComponents>>, in bounds: Range<Date>, @ViewBuilder label: () -> Label) {
        self.init(selection, _DateBounds(lo: bounds.lowerBound, hi: bounds.upperBound, excludesUpper: true), label())
    }
    public init(selection: Binding<Set<DateComponents>>, in bounds: PartialRangeFrom<Date>, @ViewBuilder label: () -> Label) {
        self.init(selection, _DateBounds(lo: bounds.lowerBound), label())
    }
    public init(selection: Binding<Set<DateComponents>>, in bounds: PartialRangeUpTo<Date>, @ViewBuilder label: () -> Label) {
        self.init(selection, _DateBounds(hi: bounds.upperBound, excludesUpper: true), label())
    }

    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared, env = EnvironmentValues()
        let st = _slot("\(path)#DatePicker") { _DatePickerState() }
        let tz = env.timeZone, sel = selection
        func dayNumber(_ c: DateComponents) -> Int? {
            let extreme = Int.max / 366
            guard let year = c.year, (-extreme...extreme).contains(year),
                  let month = c.month, (1...12).contains(month), let day = c.day else { return nil }
            let y = c.era == 0 ? 1 - year : year
            guard (1..._Civil.monthDays(y, month)).contains(day) else { return nil }
            return _Civil.days(y, month, day)
        }
        let days = Set(sel.wrappedValue.compactMap(dayNumber))
        // With nothing selected the calendar opens on today's month, clamped into the bounds.
        let anchor = days.min().map { _Wall(day: $0, seconds: 0).date(tz) } ?? _Runtime.shared.currentDate
        var anchorDate = bounds.clamp(anchor)
        if bounds.excludesUpper, let upper = bounds.hi, anchorDate >= upper {
            let wall = _Wall(upper, tz)
            if wall.seconds == 0 {
                anchorDate = _Wall(day: wall.day - 1, seconds: 0).date(tz)
            }
        }
        let shownDate = anchorDate
        let ctx = _DPContext(selection: Binding(get: { shownDate }, set: { _ in }), bounds: bounds, env: env, state: st, disabled: rt.disabled)
        if st.month == nil {
            let a = _Wall(anchorDate, tz).ymd
            if days.isEmpty { st.month = (a.y, a.m) }
        }
        let calendar = env.calendar
        let multi = _MultiSelection(contains: { days.contains($0) }, toggle: { day in
            let (y, m, d) = _Civil.ymd(day)
            st.month = (y, m)
            var set = sel.wrappedValue
            if days.contains(day) {
                set = set.filter { dayNumber($0) != day }
            } else {
                let c = DateComponents(calendar: calendar, timeZone: tz, era: y > 0 ? 1 : 0,
                                       year: y > 0 ? y : 1 - y, month: m, day: d)
                set.insert(c)
            }
            sel.wrappedValue = set
        })
        // In a List / Form row the calendar has the row's width and 39 pt weeks; elsewhere it fills the column and
        // its weeks share the proposed height.
        if rt.styles.inList {
            let cal = _calendar(ctx, path + "/cal", width: rt.viewport.width - 72, inset: 0, headerX: 12, navTrailing: -6,
                                rowH: _DP.listRowH, rows: 6, navTint: Color.accentColor, multi: multi)
            return [_padEdges(cal, top: -1, bottom: _DP.listBottom)]
        }
        return [_calendar(ctx, path + "/cal", width: rt.viewport.width - 32, inset: 0, headerX: 12.33, navTrailing: -6, rowH: nil, rows: 0,
                          navTint: Color.accentColor, multi: multi)]
    }
}
extension MultiDatePicker where Label == Text {
    public init<S: StringProtocol>(_ title: S, selection: Binding<Set<DateComponents>>) { self.init(selection: selection) { Text(String(title)) } }
    public init<S: StringProtocol>(_ title: S, selection: Binding<Set<DateComponents>>, in bounds: Range<Date>) {
        self.init(selection: selection, in: bounds) { Text(String(title)) }
    }
    public init<S: StringProtocol>(_ title: S, selection: Binding<Set<DateComponents>>, in bounds: PartialRangeFrom<Date>) {
        self.init(selection: selection, in: bounds) { Text(String(title)) }
    }
    public init<S: StringProtocol>(_ title: S, selection: Binding<Set<DateComponents>>, in bounds: PartialRangeUpTo<Date>) {
        self.init(selection: selection, in: bounds) { Text(String(title)) }
    }
}

// MARK: - ColorPicker

/// Grid tab of the system color sheet: 12 columns x 10 rows, sampled from iOS 27.2 (UIColorPickerViewController).
let _colorGrid: [UInt32] = [
    0xFFFFFF, 0xEBEBEB, 0xD6D6D6, 0xC2C2C2, 0xADADAD, 0x999999, 0x858585, 0x707070, 0x5C5C5C, 0x474747, 0x333333, 0x000000,
    0x00374A, 0x011D57, 0x11053B, 0x2E063D, 0x3C071B, 0x5C0701, 0x5A1C00, 0x583300, 0x563D00, 0x666100, 0x4F5504, 0x263E0F,
    0x004D65, 0x012F7B, 0x1A0A52, 0x450D59, 0x551029, 0x831100, 0x7B2900, 0x7A4A00, 0x785800, 0x8D8602, 0x6F760A, 0x38571A,
    0x016E8F, 0x0042A9, 0x2C0977, 0x61187C, 0x791A3D, 0xB51A00, 0xAD3E00, 0xA96800, 0xA67B01, 0xC4BC00, 0x9BA50E, 0x4E7A27,
    0x008CB4, 0x0056D6, 0x371A94, 0x7A219E, 0x99244F, 0xE22400, 0xDA5100, 0xD38301, 0xD19D01, 0xF5EC00, 0xC3D117, 0x669D34,
    0x00A1D8, 0x0061FE, 0x4D22B2, 0x982ABC, 0xB92D5D, 0xFF4015, 0xFF6A00, 0xFFAB01, 0xFDC700, 0xFEFB41, 0xD9EC37, 0x76BB40,
    0x01C7FC, 0x3A87FE, 0x5E30EB, 0xBE38F3, 0xE63B7A, 0xFF6250, 0xFF8648, 0xFEB43F, 0xFECB3E, 0xFFF76B, 0xE4EF65, 0x96D35F,
    0x52D6FC, 0x74A7FF, 0x864FFE, 0xD357FE, 0xEE719E, 0xFF8C82, 0xFFA57D, 0xFFC777, 0xFFD977, 0xFFF994, 0xEAF28F, 0xB1DD8B,
    0x93E3FD, 0xA7C6FF, 0xB18CFE, 0xE292FE, 0xF4A4C0, 0xFFB5AF, 0xFFC5AB, 0xFFD9A8, 0xFEE4A8, 0xFFFBB9, 0xF2F7B7, 0xCDE8B5,
    0xCBF0FF, 0xD3E2FF, 0xD9C9FE, 0xEFCAFF, 0xF9D3E0, 0xFFDBD8, 0xFFE2D6, 0xFFECD4, 0xFFF2D5, 0xFEFCDD, 0xF7FADB, 0xDFEED4,
]
/// The sheet's favorites row on a fresh device.
let _colorFavorites: [UInt32] = [0x000000, 0x0161FD, 0x32C759, 0xFFCC02, 0xFF3A30]
/// The color well's hue ring, every 10 degrees counter-clockwise from the right (iOS 27.2).
let _hueRing: [UInt32] = [
    0xE84746, 0xE85E47, 0xE87448, 0xE88A4A, 0xE89B44, 0xE8AC48, 0xE8BA47, 0xE8CA47, 0xE8D64A, 0xE3E349, 0xCEE249, 0xBCE548,
    0xAAE449, 0x98E54B, 0x85E84B, 0x7AE95F, 0x6AE76C, 0x59E87A, 0x49E88C, 0x4AE19E, 0x4BD8B4, 0x4CD2C8, 0x47C8DC, 0x4FB8E8,
    0x569FE7, 0x5D83E7, 0x686BE8, 0x714DE8, 0x834AE2, 0x9147DE, 0xA347D9, 0xB447D4, 0xBF44C1, 0xCB49A2, 0xD54780, 0xDF475F,
]

func _hex(_ v: UInt32, _ a: Double = 1) -> Color {
    Color(red: Double(v >> 16 & 0xFF) / 255, green: Double(v >> 8 & 0xFF) / 255, blue: Double(v & 0xFF) / 255, opacity: a)
}

/// A color's light-appearance components (semantic colors resolve to their iOS values).
func _rgba(_ c: Color) -> (r: Double, g: Double, b: Double, a: Double) {
    let j = c.json
    if let r = j["r"] as? Double, let g = j["g"] as? Double, let b = j["b"] as? Double {
        return (r, g, b, j["a"] as? Double ?? 1)
    }
    switch j["semantic"] as? String {
    case "accent": return (0, 136.0 / 255, 1, 1)
    case "secondary": return (60.0 / 255, 60.0 / 255, 67.0 / 255, 0.6)
    case "tertiary": return (60.0 / 255, 60.0 / 255, 67.0 / 255, 0.3)
    case "background": return (1, 1, 1, 1)
    default: return (0, 0, 0, 1)
    }
}

/// Look of the color well and sheet (iOS 27.2; sheet values are the 402 pt layout the floating sheet scales by 0.96).
enum _CP {
    static let well = 28.0, ring = 3.0, swatch = 18.0, ringSegments = 72, rowH = 58.0
    /// Resting height of the floating sheet (points on screen, from its top to the screen bottom).
    static let sheetH = 637.33, sheetHNoOpacity = 547.0
    static let margin = 16.0, headerY = 37.5, headerButton = 44.0
    static let segmentTop = 71.0, segmentH = 30.0
    static let gridTop = 116.7, gridRows = 10, gridCols = 12
    static let opacityTop = 445.0, trackH = 32.7, thumb = 31.0, fieldW = 75.0, fieldH = 35.4
    static let dividerY = 521.5, swatchTop = 543.8, bigSwatch = 77.7, favorite = 30.5, favoriteStep = 47.9
    static let sheetFill = Color(json: ["r": 237.0 / 255, "g": 236.0 / 255, "b": 238.0 / 255, "a": 1.0, "dark": [28.0 / 255, 28.0 / 255, 30.0 / 255]])
    static let segmentFill = Color(json: ["r": 118.0 / 255, "g": 118.0 / 255, "b": 128.0 / 255, "a": 0.2, "dark": [118.0 / 255, 118.0 / 255, 128.0 / 255]])
    static let control = Color(json: ["r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0, "dark": [99.0 / 255, 99.0 / 255, 102.0 / 255]])
    static let plusFill = Color(json: ["r": 217.0 / 255, "g": 217.0 / 255, "b": 217.0 / 255, "a": 1.0, "dark": [58.0 / 255, 58.0 / 255, 60.0 / 255]])
}

final class _ColorPickerState {
    var open = false
    enum Tab: String, CaseIterable { case grid = "Grid", spectrum = "Spectrum", sliders = "Sliders" }
    var tab = Tab.grid
    var hexDraft: (text: String, rgb: String)?
    var favorites = _colorFavorites.map { _hex($0) }
}

/// A row with the label and a color well; tapping it opens the system color sheet (grid, opacity, favorites).
public struct ColorPicker<Label: View>: View, _Flattenable {
    public typealias Body = Never
    public var body: Never { fatalError() }
    let get: () -> Color, set: (Color) -> Void, supportsOpacity: Bool, label: Label

    public init(selection: Binding<Color>, supportsOpacity: Bool = true, @ViewBuilder label: () -> Label) {
        get = { selection.wrappedValue }; set = { selection.wrappedValue = $0 }
        self.supportsOpacity = supportsOpacity; self.label = label()
    }

    func _children(_ path: String) -> [Node] {
        let rt = _Runtime.shared
        let st = _slot("\(path)#ColorPicker") { _ColorPickerState() }
        if !rt.pageIsActive || rt.disabled { st.open = false }
        if !st.open { st.hexDraft = nil }
        let color = get()
        let labelNode = rt.styles.labelsHidden ? nil : _optionalNode(label, path + "/0")
        let well = _colorWell(path + "/well", color)
        var kids: [Node] = []
        if let labelNode { kids += [labelNode, ["kind": "spacer", "id": path + "/s", "minLength": NSNull()] as Node] }
        kids.append(well)
        let row: Node = ["kind": "hstack", "id": path + "/h", "spacing": NSNull(), "alignment": "center", "children": kids]
        let open = rt.disabled ? 0 : rt.registerAction { st.open = true; rt.dirty = true }
        let title = labelNode.flatMap(_plainText) ?? ""
        if st.open {
            let close = { st.open = false; rt.dirty = true }
            rt.layers.append(["kind": "sheet", "content": _colorSheet(path + "/sheet", title: title, color: color, set: set,
                                                                      opacity: supportsOpacity, state: st, close: close),
                              "detents": [["kind": "height", "value": supportsOpacity ? _CP.sheetH : _CP.sheetHNoOpacity] as Node,
                                          ["kind": "large"] as Node] as [Node],
                              "dismiss": rt.registerAction(close)])
        }
        var b = _plainButton(path, open, row, disabled: rt.disabled)
        b["value"] = _colorName(color)
        b["modifiers"] = [["rowMinHeight": _CP.rowH]] as [Node]
        return [b]
    }
}
extension ColorPicker where Label == Text {
    public init<S: StringProtocol>(_ title: S, selection: Binding<Color>, supportsOpacity: Bool = true) {
        self.init(selection: selection, supportsOpacity: supportsOpacity) { Text(String(title)) }
    }
}
#if canImport(CoreGraphics)
import CoreGraphics
extension ColorPicker {
    public init(selection: Binding<CGColor>, supportsOpacity: Bool = true, @ViewBuilder label: () -> Label) {
        get = {
            let c = selection.wrappedValue.converted(to: CGColorSpace(name: CGColorSpace.sRGB)!, intent: .defaultIntent, options: nil)?.components ?? [0, 0, 0, 1]
            return c.count >= 4 ? Color(red: Double(c[0]), green: Double(c[1]), blue: Double(c[2]), opacity: Double(c[3]))
                                : Color(white: Double(c.first ?? 0), opacity: Double(c.last ?? 1))
        }
        set = { c in
            let (r, g, b, a) = _rgba(c)
            selection.wrappedValue = CGColor(srgbRed: r, green: g, blue: b, alpha: a)
        }
        self.supportsOpacity = supportsOpacity; self.label = label()
    }
}
extension ColorPicker where Label == Text {
    public init<S: StringProtocol>(_ title: S, selection: Binding<CGColor>, supportsOpacity: Bool = true) {
        self.init(selection: selection, supportsOpacity: supportsOpacity) { Text(String(title)) }
    }
}
#endif

/// Accessibility value of a color: its hex code.
func _colorName(_ c: Color) -> String {
    let (r, g, b, a) = _rgba(c)
    func h(_ v: Double) -> String { let n = Int((min(1, max(0, v)) * 255).rounded()); return (n < 16 ? "0" : "") + String(n, radix: 16, uppercase: true) }
    return "#" + h(r) + h(g) + h(b) + (a < 1 ? h(a) : "")
}

/// The well: a hue ring around the color (a 2 pt gap between them).
func _colorWell(_ id: String, _ color: Color) -> Node {
    let n = _CP.ringSegments, ro = _CP.well / 2, ri = ro - _CP.ring, c = ro
    var kids: [Node] = []
    for i in 0..<n {
        // Counter-clockwise from the right, each wedge a little wider so neighbours overlap (no seams).
        let a0 = Double(i) / Double(n) * 2 * .pi - 0.01, a1 = Double(i + 1) / Double(n) * 2 * .pi + 0.01
        var p = Path()
        p.move(to: CGPoint(x: c + ro * cos(a0), y: c - ro * sin(a0)))
        p.addLine(to: CGPoint(x: c + ro * cos(a1), y: c - ro * sin(a1)))
        p.addLine(to: CGPoint(x: c + ri * cos(a1), y: c - ri * sin(a1)))
        p.addLine(to: CGPoint(x: c + ri * cos(a0), y: c - ri * sin(a0)))
        p.closeSubpath()
        let t = Double(i) + 0.5, k = t * 36 / Double(n)
        let lo = Int(k) % 36, hi = (lo + 1) % 36, f = k - Double(Int(k))
        func mix(_ s: UInt32) -> Double {
            let x = Double(_hueRing[lo] >> s & 0xFF), y = Double(_hueRing[hi] >> s & 0xFF)
            return (x + (y - x) * f) / 255
        }
        kids.append(["kind": "shape", "id": "\(id)/r\(i)", "shape": "path", "path": p._wireOps(),
                     "fill": Color(red: mix(16), green: mix(8), blue: mix(0)).json])
    }
    kids.append(_frame(["kind": "shape", "id": id + "/c", "shape": "circle", "fill": color.json], w: _CP.swatch, h: _CP.swatch))
    return _frame(["kind": "zstack", "id": id, "alignment": "center", "children": kids], w: _CP.well, h: _CP.well)
}

/// The color sheet: close and eyedropper buttons around the title, Grid / Spectrum / Sliders, the grid, opacity,
/// and the current color beside the favorites.
func _colorSheet(_ path: String, title: String, color: Color, set setColor: @escaping (Color) -> Void, opacity: Bool,
                 state st: _ColorPickerState, close: @escaping () -> Void) -> Node {
    let rt = _Runtime.shared
    let (r, g, b, a) = _rgba(color)
    func set(_ next: Color) {
        if opacity {
            setColor(next)
        } else {
            let (r, g, b, _) = _rgba(next)
            setColor(Color(red: r, green: g, blue: b))
        }
    }
    let w = min(rt.viewport.width, 402), inner = max(0, w - 2 * _CP.margin)
    var kids: [(Double, Double, Node)] = []   // (x, y, node) on the 402 pt layout
    // Header.
    func roundButton(_ id: String, _ symbol: String, _ action: UInt64) -> Node {
        let icon = _symbol(id + "/i", symbol, 20, "medium", Color.primary)
        var n = _plainButton(id, action, _frame(icon, w: _CP.headerButton, h: _CP.headerButton))
        n["modifiers"] = [_glassNode(Glass.regular.interactive(), Circle())] as [Node]
        return n
    }
    kids.append((_CP.margin, _CP.headerY - _CP.headerButton / 2, roundButton(path + "/close", "xmark", rt.registerAction(close))))
    kids.append((w - _CP.margin - _CP.headerButton, _CP.headerY - _CP.headerButton / 2, roundButton(path + "/eyedropper", "eyedropper", 0)))
    kids.append((0, _CP.headerY - 11, _frame(_text(path + "/title", title, 17, "semibold"), w: w, h: 22)))
    // Grid / Spectrum / Sliders.
    let tabs = _ColorPickerState.Tab.allCases
    let segW = inner / 3
    var segKids: [Node] = []
    for (i, tab) in tabs.enumerated() {
        var label = _frame(_text("\(path)/seg/\(i)/t", tab.rawValue, 14, tab == st.tab ? "semibold" : "medium"), w: segW - 4, h: _CP.segmentH - 4)
        if tab == st.tab {
            label["modifiers"] = (label["modifiers"] as! [Node]) + [["background": ["kind": "shape", "id": "\(path)/seg/\(i)/bg", "shape": "capsule",
                                                                                   "fill": _CP.control.json] as Node]]
        }
        segKids.append(_frame(_plainButton("\(path)/seg/\(i)", rt.registerAction {
            if st.tab != tab { st.hexDraft = nil }
            st.tab = tab
            rt.dirty = true
        }, label, value: tab == st.tab ? "selected" : nil),
                              w: segW, h: _CP.segmentH))
    }
    let seg: [Node] = [_frame(["kind": "shape", "id": path + "/seg/bg", "shape": "capsule", "fill": _CP.segmentFill.json], w: inner, h: _CP.segmentH),
           ["kind": "hstack", "id": path + "/seg/h", "spacing": 0.0, "alignment": "center", "children": segKids]]
    kids.append((_CP.margin, _CP.segmentTop, ["kind": "zstack", "id": path + "/seg", "alignment": "center", "children": seg]))
    let cell = inner / Double(_CP.gridCols)
    let paletteHeight = cell * Double(_CP.gridRows)
    let extraHeight = paletteHeight - (402 - 2 * _CP.margin) / Double(_CP.gridCols) * Double(_CP.gridRows)
    let palette: Node
    switch st.tab {
    case .grid: palette = _colorPalette(path, color: color, width: inner + 2.2, cellHeight: cell, set: set)
    case .spectrum: palette = _colorSpectrum(path + "/spectrum", width: inner, height: paletteHeight, alpha: a, set: set)
    case .sliders: palette = _colorSliders(path + "/sliders", color: color, width: inner, state: st, set: set)
    }
    kids.append((_CP.margin, _CP.gridTop, palette))
    var y = _CP.gridTop + cell * Double(_CP.gridRows)
    if opacity {
        kids.append((_CP.margin, _CP.opacityTop + extraHeight, _text(path + "/op/t", "OPACITY", 13, "semibold", Color(json: ["semantic": "secondary"]))))
        let trackW = max(0, inner - _CP.fieldW - 16)
        let trackY = _CP.opacityTop + extraHeight + 20
        kids.append((_CP.margin, trackY, _opacityTrack(path + "/op/track", r, g, b, a, width: trackW, set: set)))
        var pct = _frame(_text(path + "/op/v", "\(Int((a * 100).rounded()))%", 17), w: _CP.fieldW, h: _CP.fieldH)
        pct["modifiers"] = (pct["modifiers"] as! [Node]) + [["background": ["kind": "shape", "id": path + "/op/v/bg", "shape": "capsule", "fill": _CP.control.json] as Node]]
        kids.append((w - _CP.margin - _CP.fieldW, trackY + (_CP.trackH - _CP.fieldH) / 2, pct))
        y = _CP.dividerY + extraHeight
    } else {
        y += 24
    }
    let dy = y - _CP.dividerY
    kids.append((_CP.margin, y, _frame(["kind": "divider", "id": path + "/div"], w: inner, h: 1)))
    // Current color, favorites, add.
    kids.append((_CP.margin, _CP.swatchTop + dy, _frame(["kind": "shape", "id": path + "/cur", "shape": "roundedRectangle", "cornerRadius": 16.0,
                                                         "fill": color.json], w: _CP.bigSwatch, h: _CP.bigSwatch)))
    let favY = _CP.swatchTop + dy + 15.2 - _CP.favorite / 2
    let favX = 127.8 - _CP.favorite / 2
    let favorites = st.favorites.enumerated().map { i, color -> Node in
        let id = "\(path)/fav/\(i)"
        let swatch = _frame(["kind": "shape", "id": id + "/c", "shape": "circle", "fill": color.json], w: _CP.favorite, h: _CP.favorite)
        return _plainButton(id, rt.registerAction { set(color); rt.dirty = true }, swatch, value: _colorName(color))
    }
    var plus = _frame(_symbol(path + "/add/i", "plus", 17, "semibold", Color(json: ["semantic": "secondary"])), w: _CP.favorite, h: _CP.favorite)
    plus["modifiers"] = (plus["modifiers"] as! [Node]) + [["background": ["kind": "shape", "id": path + "/add/bg", "shape": "circle", "fill": _CP.plusFill.json] as Node]]
    let add = _plainButton(path + "/add", rt.registerAction {
        if !st.favorites.contains(where: { _colorName($0) == _colorName(color) }) {
            st.favorites.insert(color, at: 0)
        }
        rt.dirty = true
    }, plus, value: "Add favorite")
    let row: Node = ["kind": "hstack", "id": path + "/favorites/row", "spacing": _CP.favoriteStep - _CP.favorite,
                     "alignment": "center", "children": favorites + [add]]
    let scroller: Node = ["kind": "scroll", "id": path + "/favorites", "axis": "horizontal", "indicators": "hidden", "child": row]
    kids.append((favX, favY, _frame(scroller, w: max(0, w - favX - _CP.margin), h: _CP.favorite)))
    // Place everything with offsets in a top-leading zstack the width of the screen.
    let placed: [Node] = kids.map { x, y, n in
        var n = n
        n["modifiers"] = ((n["modifiers"] as? [Node]) ?? []) + [["offset": ["x": x, "y": y] as Node]]
        return n
    }
    var root: Node = ["kind": "zstack", "id": path, "alignment": "topLeading", "children": placed]
    root["modifiers"] = [["frame": ["width": w, "maxHeight": "inf", "alignment": "topLeading"] as Node],
                         ["background": ["kind": "color", "id": path + "/bg", "color": _CP.sheetFill.json] as Node]] as [Node]
    return root
}

func _colorPalette(_ path: String, color: Color, width: Double, cellHeight: Double, set: @escaping (Color) -> Void) -> Node {
    let rt = _Runtime.shared
    let (r, g, b, a) = _rgba(color)
    let cellWidth = width / Double(_CP.gridCols)
    var rows: [Node] = []
    for row in 0..<_CP.gridRows {
        var cells: [Node] = []
        for col in 0..<_CP.gridCols {
            let v = _colorGrid[row * _CP.gridCols + col]
            let c = _hex(v, a)
            let chosen = abs(Double(v >> 16 & 0xFF) / 255 - r) < 0.003 && abs(Double(v >> 8 & 0xFF) / 255 - g) < 0.003 && abs(Double(v & 0xFF) / 255 - b) < 0.003
            var sw: Node = _frame(["kind": "color", "id": "\(path)/g/\(row)/\(col)/c", "color": _hex(v).json], w: cellWidth, h: cellHeight)
            if chosen {
                sw = ["kind": "zstack", "id": "\(path)/g/\(row)/\(col)/z", "alignment": "center", "children": [
                    sw, _frame(["kind": "shape", "id": "\(path)/g/\(row)/\(col)/sel", "shape": "roundedRectangle", "cornerRadius": 4.0,
                                "stroke": ["color": ["r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0] as Node, "lineWidth": 3.0] as Node], w: cellWidth - 3, h: cellHeight - 3),
                ] as [Node]]
            }
            cells.append(_plainButton("\(path)/g/\(row)/\(col)", rt.registerAction { set(c); rt.dirty = true }, sw,
                                      value: chosen ? "selected" : nil))
        }
        rows.append(["kind": "hstack", "id": "\(path)/g/\(row)", "spacing": 0.0, "alignment": "center", "children": cells])
    }
    var grid: Node = ["kind": "vstack", "id": path + "/g", "spacing": 0.0, "alignment": "center", "children": rows]
    grid["modifiers"] = [["cornerRadius": 10.0]] as [Node]
    return grid
}

func _colorSliders(_ path: String, color: Color, width: Double, state: _ColorPickerState, set: @escaping (Color) -> Void) -> Node {
    let (r, g, b, a) = _rgba(color)
    let channels = [("Red", r), ("Green", g), ("Blue", b)]
    let controls = channels.enumerated().map { index, channel -> Node in
        let (name, value) = channel
        let binding = Binding(get: { value * 255 }, set: { value in
            var rgb = [r, g, b]
            rgb[index] = value / 255
            set(Color(red: rgb[0], green: rgb[1], blue: rgb[2], opacity: a))
        })
        return _eval(VStack(alignment: .leading, spacing: 4) {
            Text(name)
            Slider(value: binding, in: 0...255, step: 1)
        }, "\(path)/\(index)")
    }
    let rgb = _colorName(Color(red: r, green: g, blue: b))
    if state.hexDraft?.rgb != rgb { state.hexDraft = nil }
    let hex = Binding(get: { state.hexDraft?.text ?? rgb }, set: { input in
        state.hexDraft = (input, rgb)
        _Runtime.shared.dirty = true
        let raw = input.first == "#" ? input.dropFirst() : input[...]
        guard raw.count == 6, raw.allSatisfy({ $0.isHexDigit }), let value = UInt32(raw, radix: 16) else { return }
        state.hexDraft = (input, _colorName(_hex(value)))
        set(_hex(value, a))
    })
    let kids = controls + [_eval(TextField("Hex color", text: hex), path + "/hex")]
    return _frame(["kind": "vstack", "id": path, "spacing": 15.0, "alignment": "leading", "children": kids], w: width, h: nil)
}

/// A hue across the horizontal axis, shaded from white to the hue to black vertically.
func _spectrumColor(_ x: Double, _ y: Double, alpha: Double) -> Color {
    let h = min(1, max(0, x)) * 6
    let sy = min(1, max(0, y)) * 2
    let value = min(1, 2 - sy)
    let saturation = min(1, sy)
    func component(_ offset: Double) -> Double {
        let phase = (h + offset).truncatingRemainder(dividingBy: 6)
        return value * (1 - saturation * min(1, max(0, min(phase, 4 - phase))))
    }
    return Color(red: component(5), green: component(3), blue: component(1), opacity: alpha)
}

func _colorSpectrum(_ path: String, width: Double, height: Double, alpha: Double, set: @escaping (Color) -> Void) -> Node {
    // Draw the spectrum independently from selection, which uses the actual pointer position.
    let columns = 48, rows = 24
    let bands: [Node] = (0..<rows).map { row in
        let cells: [Node] = (0..<columns).map { col in
            let c = _spectrumColor(Double(col) / Double(columns - 1), Double(row) / Double(rows - 1), alpha: 1)
            return _frame(c._emit("\(path)/\(row)/\(col)"), w: width / Double(columns), h: height / Double(rows))
        }
        return ["kind": "hstack", "id": "\(path)/\(row)", "spacing": 0.0, "alignment": "center", "children": cells]
    }
    let node: Node = ["kind": "vstack", "id": path, "spacing": 0.0, "alignment": "center", "children": bands,
                      "modifiers": [["cornerRadius": 10.0]] as [Node]]
    func pick(_ point: CGPoint) {
        set(_spectrumColor(Double(point.x) / width, Double(point.y) / height, alpha: alpha))
        _Runtime.shared.dirty = true
    }
    let gesture = DragGesture(minimumDistance: 0).onChanged { pick($0.location) }.onEnded { pick($0.location) }
    return _attachGesture([node], path, .normal) { gesture._recognizer($0) }[0]
}

/// Opacity slider: a checkerboard under the color fading in from clear, and a white thumb at the opacity.
func _opacityTrack(_ id: String, _ r: Double, _ g: Double, _ b: Double, _ a: Double, width: Double, set: @escaping (Color) -> Void) -> Node {
    let rt = _Runtime.shared, h = _CP.trackH
    var kids: [Node] = []
    let sq = h / 3
    let cols = Int((width / sq).rounded(.up))
    var checker: [Node] = []
    for row in 0..<3 {
        var line: [Node] = []
        for col in 0..<cols {
            let dark = (row + col) % 2 == 0
            line.append(_frame(["kind": "color", "id": "\(id)/k/\(row)/\(col)", "color": (dark ? _hex(0xD9D9DD) : _hex(0xEEEEF0)).json], w: sq, h: sq))
        }
        checker.append(["kind": "hstack", "id": "\(id)/k/\(row)", "spacing": 0.0, "alignment": "center", "children": line])
    }
    kids.append(_frame(["kind": "vstack", "id": id + "/k", "spacing": 0.0, "alignment": "leading", "children": checker], w: width, h: h, align: "leading"))
    // The fade: thin strips of the color at rising alpha.
    let strips = 48
    let sw = width / Double(strips)
    kids.append(["kind": "hstack", "id": id + "/f", "spacing": 0.0, "alignment": "center", "children": (0..<strips).map { i -> Node in
        _frame(["kind": "color", "id": "\(id)/f/\(i)", "color": Color(red: r, green: g, blue: b, opacity: (Double(i) + 0.5) / Double(strips)).json], w: sw + 0.01, h: h)
    }])
    // Taps along the track set the opacity (in 10% steps).
    kids.append(["kind": "hstack", "id": id + "/t", "spacing": 0.0, "alignment": "center", "children": (0...10).map { i -> Node in
        let v = Double(i) / 10
        return _plainButton("\(id)/t/\(i)", rt.registerAction { set(Color(red: r, green: g, blue: b, opacity: v)); rt.dirty = true },
                            _frame(["kind": "empty", "id": "\(id)/t/\(i)/e"], w: width / 11, h: h), value: "\(i * 10)%")
    }])
    let tx = h / 2 + 2 + (width - h - 4) * a - _CP.thumb / 2
    var thumb = _frame(["kind": "shape", "id": id + "/thumb/c", "shape": "circle", "fill": Color(red: r, green: g, blue: b).json],
                       w: _CP.thumb - 6, h: _CP.thumb - 6)
    thumb["modifiers"] = (thumb["modifiers"] as! [Node]) + [
        ["padding": ["leading": 3.0, "trailing": 3.0, "top": 3.0, "bottom": 3.0] as Node],
        ["background": ["kind": "shape", "id": id + "/thumb/bg", "shape": "circle", "fill": ["r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0] as Node] as Node],
        ["offset": ["x": tx, "y": (h - _CP.thumb) / 2] as Node],
    ]
    kids.append(thumb)
    var track: Node = ["kind": "zstack", "id": id, "alignment": "topLeading", "children": kids]
    track["modifiers"] = [["frame": ["width": width, "height": h, "alignment": "topLeading"] as Node],
                          ["cornerRadius": h / 2]] as [Node]
    guard width > h else { return track }
    func pick(_ x: CGFloat) {
        let value = min(1, max(0, (Double(x) - h / 2) / (width - h)))
        set(Color(red: r, green: g, blue: b, opacity: value))
        rt.dirty = true
    }
    let drag = DragGesture().onChanged { pick($0.location.x) }.onEnded { pick($0.location.x) }
    return _attachGesture([track], id, .normal) { drag._recognizer($0) }[0]
}
