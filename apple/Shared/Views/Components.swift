import SwiftUI

// The launcher's controls, as the desktop launcher has them (`launcher/ui/js/ui.js`,
// `styles.css`): cards on the surface colour, controls on a lighter one, no outlines anywhere.

/// A card: a title (and a hint under it) with its actions on the right, then the body.
struct Card<Content: View>: View {
    var title: LocalizedStringKey?
    var hint: LocalizedStringKey?
    var actions: AnyView?
    var padded = true
    @ViewBuilder var content: Content

    init(_ title: LocalizedStringKey? = nil, hint: LocalizedStringKey? = nil, actions: AnyView? = nil, padded: Bool = true, @ViewBuilder content: () -> Content) {
        self.title = title
        self.hint = hint
        self.actions = actions
        self.padded = padded
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if title != nil || actions != nil {
                HStack(alignment: .center, spacing: 12) {
                    VStack(alignment: .leading, spacing: 2) {
                        if let title { Text(title).font(.inter(15, .semibold)).foregroundStyle(Theme.text) }
                        if let hint { Text(hint).font(.inter(12.5)).foregroundStyle(Theme.muted) }
                    }
                    Spacer(minLength: 8)
                    actions
                }
                .padding(.horizontal, 20)
                .padding(.top, 16)
            }
            VStack(alignment: .leading, spacing: 14) { content }
                .padding(.horizontal, padded ? 20 : 0)
                .padding(.vertical, padded ? 18 : 0)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .surface()
    }
}

/// The small upper-case title of a group inside a card.
struct SectionTitle: View {
    var text: LocalizedStringKey
    init(_ text: LocalizedStringKey) { self.text = text }
    var body: some View {
        Text(text)
            .textCase(.uppercase)
            .font(.inter(12, .semibold))
            .tracking(0.7)
            .foregroundStyle(Theme.muted)
            .padding(.top, 8)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A page's heading and the line under it, with the page's actions on the right.
struct PageHeader<Actions: View>: View {
    var title: LocalizedStringKey
    var subtitle: LocalizedStringKey?
    @ViewBuilder var actions: Actions

    init(title: LocalizedStringKey, subtitle: LocalizedStringKey? = nil, @ViewBuilder actions: () -> Actions = { EmptyView() }) {
        self.title = title
        self.subtitle = subtitle
        self.actions = actions()
    }

    var body: some View {
        HStack(alignment: .bottom, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.inter(26, .bold)).tracking(-0.5).foregroundStyle(Theme.text)
                if let subtitle {
                    Text(subtitle).font(.inter(14)).foregroundStyle(Theme.muted).fixedSize(horizontal: false, vertical: true)
                }
            }
            .frame(maxWidth: 640, alignment: .leading)
            Spacer(minLength: 0)
            actions
        }
        .padding(.bottom, 6)
    }
}

/// A page: its heading, then its content scrolling under it.
struct PageScroll<Content: View, Actions: View>: View {
    var title: LocalizedStringKey
    var subtitle: LocalizedStringKey?
    var maxWidth: CGFloat? = nil
    @ViewBuilder var actions: Actions
    @ViewBuilder var content: Content
    @Environment(\.compactLayout) private var compact

    init(_ title: LocalizedStringKey, subtitle: LocalizedStringKey? = nil, maxWidth: CGFloat? = nil, @ViewBuilder actions: () -> Actions = { EmptyView() }, @ViewBuilder content: () -> Content) {
        self.title = title
        self.subtitle = subtitle
        self.maxWidth = maxWidth
        self.actions = actions()
        self.content = content()
    }

    var body: some View {
        Scroll {
            VStack(alignment: .leading, spacing: 16) {
                PageHeader(title: title, subtitle: subtitle) { actions }
                content
            }
            .frame(maxWidth: maxWidth ?? .infinity, alignment: .leading)
            .padding(.horizontal, compact ? 16 : 32)
            .padding(.top, compact ? 12 : 28)
            .padding(.bottom, compact ? 24 : 40)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollIndicators(.automatic)
    }
}

/// A label over its control (the desktop launcher's `Field`), with a help line under it.
struct Field<Control: View>: View {
    var label: LocalizedStringKey
    var help: String?
    @ViewBuilder var control: Control

    init(_ label: LocalizedStringKey, help: String? = nil, @ViewBuilder control: () -> Control) {
        self.label = label
        self.help = help
        self.control = control()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(.inter(12.5, .medium)).foregroundStyle(Theme.text2)
            control
            if let help, !help.isEmpty {
                Text(help).font(.inter(12)).foregroundStyle(Theme.muted).lineLimit(3)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A settings row: the name (and a line under it) on the left, the control on the right.
struct LabeledRow<Control: View>: View {
    var label: LocalizedStringKey
    var detail: String?
    @ViewBuilder var control: Control
    @Environment(\.compactLayout) private var compact

    init(_ label: LocalizedStringKey, detail: String? = nil, @ViewBuilder control: () -> Control) {
        self.label = label
        self.detail = detail
        self.control = control()
    }

    var body: some View {
        HStack(spacing: 16) {
            VStack(alignment: .leading, spacing: 2) {
                Text(label).font(.inter(14)).foregroundStyle(Theme.text).fixedSize(horizontal: false, vertical: true)
                if let detail, !detail.isEmpty {
                    Text(detail).font(.inter(12)).foregroundStyle(Theme.muted).lineLimit(3)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            control
                .frame(width: compact ? 170 : 210, alignment: .trailing)
        }
        .padding(.vertical, 1)
    }
}

/// A choice of a list, as a menu on a control's ground with a chevron.
struct SelectField: View {
    var options: [(value: String, label: String)]
    var selection: String
    var onChange: (String) -> Void
    var placeholder: String = "Choose…"

    var body: some View {
        Menu {
            ForEach(options, id: \.value) { o in
                Button {
                    onChange(o.value)
                } label: {
                    if o.value == selection { Label(o.label, systemImage: "checkmark") } else { Text(o.label) }
                }
            }
        } label: {
            HStack(spacing: 6) {
                Text(options.first { $0.value == selection }?.label ?? (selection.isEmpty ? placeholder : selection))
                    .font(.inter(14))
                    .foregroundStyle(Theme.text)
                    .lineLimit(1)
                Spacer(minLength: 4)
                Image(systemName: "chevron.down").font(.system(size: 10, weight: .bold)).foregroundStyle(Theme.muted)
            }
            .padding(.horizontal, 11)
            .frame(maxWidth: .infinity, minHeight: 36)
            .fieldBackground()
            .contentShape(Rectangle())
        }
        .menuStyle(.button)
        .menuIndicator(.hidden)
        .buttonStyle(.plain)
    }
}

/// A text field on a control's ground.
struct InputField: View {
    var prompt: LocalizedStringKey
    @Binding var text: String
    var mono = false
    var onSubmit: () -> Void = {}

    var body: some View {
        TextField("", text: $text, prompt: Text(prompt).foregroundStyle(Theme.muted))
            .textFieldStyle(.plain)
            .font(mono ? .inter(14).monospacedDigit() : .inter(14))
            .foregroundStyle(Theme.text)
            .padding(.horizontal, 11)
            .frame(minHeight: 36)
            .fieldBackground()
            .onSubmit(onSubmit)
            #if os(iOS)
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            #endif
    }
}

/// The launcher's switch: a 36 x 20 track, amber when on.
struct SwitchToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button {
            configuration.isOn.toggle()
        } label: {
            HStack(spacing: 12) {
                ZStack(alignment: configuration.isOn ? .trailing : .leading) {
                    Capsule().fill(configuration.isOn ? Theme.accent : Theme.surface4).frame(width: 36, height: 20)
                    Circle().fill(configuration.isOn ? Theme.accentText : Theme.text2).frame(width: 14, height: 14).padding(.horizontal, 3)
                }
                .animation(.easeOut(duration: 0.12), value: configuration.isOn)
                configuration.label
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

extension ToggleStyle where Self == SwitchToggleStyle {
    static var launcher: SwitchToggleStyle { SwitchToggleStyle() }
}

/// A switch with its name and a help line beside it (the On the road card).
struct ToggleRow: View {
    var label: LocalizedStringKey
    var help: LocalizedStringKey?
    var isOn: Bool
    var onChange: (Bool) -> Void

    var body: some View {
        Toggle(isOn: Binding(get: { isOn }, set: onChange)) {
            VStack(alignment: .leading, spacing: 1) {
                Text(label).font(.inter(14, .medium)).foregroundStyle(Theme.text)
                if let help { Text(help).font(.inter(12)).foregroundStyle(Theme.muted) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .toggleStyle(.launcher)
        .padding(.vertical, 4)
    }
}

/// The launcher's slider: a 4 point track, amber up to the value, a light round thumb.
struct LauncherSlider: View {
    var value: Double
    var range: ClosedRange<Double>
    var step: Double
    var onChange: (Double) -> Void
    var onEnd: () -> Void

    var body: some View {
        GeometryReader { geo in
            let span = range.upperBound - range.lowerBound
            let f = span > 0 ? (min(range.upperBound, max(range.lowerBound, value)) - range.lowerBound) / span : 0
            let w = geo.size.width - 16
            ZStack(alignment: .leading) {
                Capsule().fill(Theme.surface3).frame(height: 4).padding(.horizontal, 8)
                Capsule().fill(Theme.accent).frame(width: max(0, w * f), height: 4).padding(.leading, 8)
                Circle().fill(Theme.text).frame(width: 16, height: 16).offset(x: w * f)
            }
            .frame(height: 20)
            .contentShape(Rectangle())
            .gesture(DragGesture(minimumDistance: 0).onChanged { g in
                let k = min(1, max(0, (g.location.x - 8) / max(1, w)))
                let v = range.lowerBound + (k * span / step).rounded() * step
                onChange(min(range.upperBound, max(range.lowerBound, v)))
            }.onEnded { _ in onEnd() })
        }
        .frame(height: 20)
    }
}

/// A slider with its name and the value shown above it.
struct SliderField: View {
    var label: LocalizedStringKey
    var value: Double
    var range: ClosedRange<Double>
    var step: Double
    var format: (Double) -> String
    var commit: (Double) -> Void
    @State private var dragging: Double?

    var body: some View {
        let v = dragging ?? value
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(label).font(.inter(14)).foregroundStyle(Theme.text)
                Spacer()
                Text(format(v)).font(.inter(14).monospacedDigit()).foregroundStyle(Theme.text2)
            }
            LauncherSlider(value: v, range: range, step: step, onChange: { dragging = $0 }, onEnd: {
                if let d = dragging { commit(d) }
                dragging = nil
            })
        }
        .padding(.vertical, 4)
    }
}

/// A setting of the settings file shown as a select.
struct SettingSelect: View {
    @Environment(LauncherStore.self) private var store
    var label: LocalizedStringKey
    var key: String
    var options: [(String, String)]
    var detail: String? = nil

    var body: some View {
        let current = store.setting(key)?.string ?? options.first?.0 ?? ""
        LabeledRow(label, detail: detail) {
            SelectField(options: options.map { (value: $0.0, label: NSLocalizedString($0.1, comment: "")) }, selection: matching(current), onChange: { v in
                store.set(key, typed(v))
            })
        }
    }

    /// The option the stored value means ("2" and 2.0 are the same choice).
    private func matching(_ current: String) -> String {
        if options.contains(where: { $0.0 == current }) { return current }
        if let d = Double(current), let o = options.first(where: { Double($0.0) == d }) { return o.0 }
        return current
    }

    /// Numbers go to the settings as numbers, words as words.
    private func typed(_ v: String) -> Any {
        if let i = Int(v) { return i }
        if let d = Double(v) { return d }
        return v
    }
}

/// A switch of the settings file.
struct SettingToggle: View {
    @Environment(LauncherStore.self) private var store
    var label: LocalizedStringKey
    var key: String
    var fallback = false
    var detail: String? = nil

    var body: some View {
        LabeledRow(label, detail: detail) {
            Toggle("", isOn: Binding(get: { store.flag(key, fallback) }, set: { store.set(key, $0) }))
                .labelsHidden()
                .toggleStyle(.launcher)
        }
    }
}

/// A number of the settings file on a slider (saved when the hand lets go).
struct SettingSlider: View {
    @Environment(LauncherStore.self) private var store
    var label: LocalizedStringKey
    var key: String
    var range: ClosedRange<Double>
    var step: Double
    var fallback: Double
    var format: (Double) -> String = { String(format: "%.2f", $0) }

    var body: some View {
        SliderField(label: label, value: store.setting(key)?.double ?? fallback, range: range, step: step, format: format) { store.set(key, $0) }
            .padding(.vertical, 2)
    }
}

/// A search field: the magnifier inside the control's ground.
struct SearchField: View {
    var prompt: LocalizedStringKey
    @Binding var text: String

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").font(.system(size: 13, weight: .semibold)).foregroundStyle(Theme.muted)
            TextField("", text: $text, prompt: Text(prompt).foregroundStyle(Theme.muted))
                .textFieldStyle(.plain)
                .font(.inter(14))
                .foregroundStyle(Theme.text)
                #if os(iOS)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                #endif
            if !text.isEmpty {
                Button { text = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(Theme.muted) }.buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 11)
        .frame(minHeight: 36)
        .fieldBackground()
    }
}

/// A row of choices, one of them taken (the desktop launcher's segmented control).
struct Segmented: View {
    var items: [(String, LocalizedStringKey)]
    var selection: String
    var full = false
    var onChange: (String) -> Void

    init(items: [(String, LocalizedStringKey)], selection: String, full: Bool = false, onChange: @escaping (String) -> Void) {
        self.items = items
        self.selection = selection
        self.full = full
        self.onChange = onChange
    }

    var body: some View {
        HStack(spacing: 2) {
            ForEach(items, id: \.0) { item in
                let on = item.0 == selection
                Button { onChange(item.0) } label: {
                    Text(item.1)
                        .font(.inter(13.5, .medium))
                        .foregroundStyle(on ? Theme.text : Theme.text2)
                        .lineLimit(1)
                        .padding(.horizontal, 12)
                        .frame(maxWidth: full ? .infinity : nil, minHeight: 28)
                        .background(on ? Theme.surface4 : .clear, in: RoundedRectangle(cornerRadius: 4, style: .continuous))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(3)
        .fieldBackground()
        .fixedSize(horizontal: !full, vertical: false)
    }
}

/// Tabs underlined in amber (Settings, Multiplayer).
struct UnderlineTabs: View {
    var items: [(String, LocalizedStringKey)]
    @Binding var selection: String

    var body: some View {
        Scroll(.horizontal) {
            HStack(spacing: 22) {
                ForEach(items, id: \.0) { item in
                    let on = item.0 == selection
                    Button { selection = item.0 } label: {
                        VStack(spacing: 0) {
                            Text(item.1).font(.inter(14, .medium)).foregroundStyle(on ? Theme.text : Theme.muted)
                                .frame(minHeight: 36)
                            Rectangle().fill(on ? Theme.accent : .clear).frame(height: 2)
                        }
                        .fixedSize()
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .scrollIndicators(.never)
        .padding(.bottom, 4)
    }
}

/// A row of a list: chosen ones on the accent's soft ground.
struct ListItem<Trailing: View>: View {
    var title: String
    var subtitle: String = ""
    var icon: String? = nil
    var selected: Bool
    var action: () -> Void
    @ViewBuilder var trailing: Trailing

    init(title: String, subtitle: String = "", icon: String? = nil, selected: Bool, action: @escaping () -> Void, @ViewBuilder trailing: () -> Trailing = { EmptyView() }) {
        self.title = title
        self.subtitle = subtitle
        self.icon = icon
        self.selected = selected
        self.action = action
        self.trailing = trailing()
    }

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                if let icon {
                    Image(systemName: icon).font(.system(size: 15)).foregroundStyle(selected ? Theme.accent : Theme.muted).frame(width: 22)
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.inter(14, .medium)).foregroundStyle(Theme.text).lineLimit(1)
                    if !subtitle.isEmpty {
                        Text(subtitle).font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(2)
                    }
                }
                Spacer(minLength: 6)
                trailing
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(selected ? Theme.accentSoft : .clear, in: RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// A small label: graphite, or tinted for a state.
struct Tag: View {
    enum Kind { case plain, accent, ok, danger, warn }
    var text: String
    var kind: Kind = .plain

    var body: some View {
        let (fg, bg): (Color, Color) = switch kind {
        case .plain: (Theme.text2, Theme.surface3)
        case .accent: (Theme.accent, Theme.accentSoft)
        case .ok: (Theme.ok, Theme.ok.opacity(0.14))
        case .danger: (Theme.danger, Theme.dangerSoft)
        case .warn: (Theme.warn, Theme.warn.opacity(0.14))
        }
        Text(LocalizedStringKey(text)).font(.inter(12, .medium)).foregroundStyle(fg).lineLimit(1)
            .padding(.horizontal, 8).frame(height: 22)
            .background(bg, in: RoundedRectangle(cornerRadius: 4, style: .continuous))
    }
}

/// A notice inside a card: an icon and a few lines, tinted for a warning or an error.
struct Notice: View {
    enum Kind { case info, warn, danger }
    var text: String
    var kind: Kind = .info
    var icon: String? = nil

    var body: some View {
        let (fg, bg): (Color, Color) = switch kind {
        case .info: (Theme.muted, Theme.surface2)
        case .warn: (Theme.warn, Theme.warn.opacity(0.1))
        case .danger: (Theme.danger, Theme.dangerSoft)
        }
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: icon ?? (kind == .info ? "info.circle.fill" : "exclamationmark.triangle.fill")).font(.system(size: 15)).foregroundStyle(fg)
            Text(text).font(.inter(13.5)).foregroundStyle(Theme.text).frame(maxWidth: .infinity, alignment: .leading).fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 14).padding(.vertical, 12)
        .background(bg, in: RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
    }
}

/// Nothing here yet: an icon, a title and a line, centred.
struct EmptyState: View {
    var icon: String
    var title: LocalizedStringKey?
    var text: LocalizedStringKey?

    var body: some View {
        VStack(spacing: 6) {
            Image(systemName: icon).font(.system(size: 30)).foregroundStyle(Theme.muted.opacity(0.7)).padding(.bottom, 6)
            if let title { Text(title).font(.inter(14, .semibold)).foregroundStyle(Theme.text2) }
            if let text { Text(text).font(.inter(13.5)).foregroundStyle(Theme.muted).multilineTextAlignment(.center) }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 36)
        .padding(.horizontal, 20)
    }
}

/// A line number as the stops' signs show it: dark figures on light.
struct LineBadge: View {
    var line: String
    var body: some View {
        Text(line).font(.inter(13, .bold).monospacedDigit()).foregroundStyle(Theme.bg).lineLimit(1)
            .padding(.horizontal, 7).frame(minWidth: 34, minHeight: 24)
            .background(Theme.text, in: RoundedRectangle(cornerRadius: 4, style: .continuous))
    }
}

/// The destination sign: the line number and the terminus in amber on near-black.
struct DestinationSign: View {
    var number: String?
    var icon: String? = nil
    var to: String

    var body: some View {
        HStack(spacing: 10) {
            Group {
                if let icon { Image(systemName: icon).font(.system(size: 17, weight: .bold)) } else { Text(number ?? "").font(.inter(20, .heavy).monospacedDigit()) }
            }
            .frame(minWidth: 34, alignment: .leading)
            Text(to.uppercased()).font(.inter(13, .semibold)).tracking(0.3).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
        }
        .foregroundStyle(Theme.signText)
        .padding(.horizontal, 12).padding(.vertical, 10)
        .background(Theme.signBg, in: RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
    }
}

/// A driver's round initial.
struct Avatar: View {
    var name: String
    var size: CGFloat = 34
    var body: some View {
        Text(String(name.first ?? "?").uppercased())
            .font(.inter(size * 0.4, .bold))
            .foregroundStyle(Theme.text2)
            .frame(width: size, height: size)
            .background(Theme.surface3, in: Circle())
    }
}

/// Names and values in two columns (the Summary's facts, Setup's places).
struct Facts: View {
    var rows: [(String, String)]
    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 14, verticalSpacing: 8) {
            ForEach(rows.indices, id: \.self) { i in
                GridRow {
                    Text(LocalizedStringKey(rows[i].0)).font(.inter(13)).foregroundStyle(Theme.muted)
                    Text(rows[i].1).font(.inter(13).monospacedDigit()).foregroundStyle(Theme.text).lineLimit(1).truncationMode(.middle)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                }
            }
        }
    }
}

/// A figure tile of the Profile page.
struct StatTile: View {
    var icon: String
    var value: String
    var label: LocalizedStringKey

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: icon).font(.system(size: 13))
                Text(label).font(.inter(12.5)).lineLimit(1)
            }
            .foregroundStyle(Theme.muted)
            Text(value).font(.inter(22, .bold).monospacedDigit()).tracking(-0.2).foregroundStyle(Theme.text).lineLimit(1).minimumScaleFactor(0.7)
        }
        .padding(.horizontal, 16).padding(.vertical, 14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .surface()
    }
}

/// A thin progress bar in amber.
struct ProgressBar: View {
    var value: Double
    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(Theme.surface3)
                Capsule().fill(Theme.accent).frame(width: g.size.width * min(1, max(0, value)))
            }
        }
        .frame(height: 6)
    }
}

/// A log's text in a monospaced block.
struct LogBlock: View {
    var text: String
    var body: some View {
        Scroll {
            Text(text).font(.system(size: 12, design: .monospaced)).foregroundStyle(Theme.text2).textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(maxHeight: 280)
        .padding(.horizontal, 12).padding(.vertical, 10)
        .fieldBackground()
    }
}

/// The launcher's message (what it is doing, what went wrong), as a toast in the corner.
struct StatusToast: View {
    @Environment(LauncherStore.self) private var store

    var body: some View {
        if let s = store.status {
            HStack(alignment: .top, spacing: 10) {
                Image(systemName: s.error ? "exclamationmark.circle.fill" : "info.circle.fill")
                    .font(.system(size: 15))
                    .foregroundStyle(s.error ? Theme.danger : Theme.muted)
                Text(s.text).font(.inter(13.5)).foregroundStyle(Theme.text).lineLimit(3).textSelection(.enabled)
            }
            .padding(.horizontal, 14).padding(.vertical, 12)
            .frame(maxWidth: 420, alignment: .leading)
            .background(Theme.surface2, in: RoundedRectangle(cornerRadius: Theme.radius, style: .continuous))
            .shadow(color: .black.opacity(0.35), radius: 15, y: 10)
            .transition(.move(edge: .bottom).combined(with: .opacity))
        }
    }
}

/// The phone's narrower layout, as the pages ask for it.
private struct CompactLayoutKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var compactLayout: Bool {
        get { self[CompactLayoutKey.self] }
        set { self[CompactLayoutKey.self] = newValue }
    }
}

extension String {
    /// A path's last part without its extension (a weather file's name ...).
    var stem: String { ((self as NSString).lastPathComponent as NSString).deletingPathExtension }
}

/// A scroll view; drawn as its plain content when the pages are rendered into pictures
/// (`launcher-shots`: SwiftUI's renderer leaves a scroll view's content out).
struct Scroll<Content: View>: View {
    var axes: Axis.Set = .vertical
    @ViewBuilder var content: Content

    init(_ axes: Axis.Set = .vertical, @ViewBuilder content: () -> Content) {
        self.axes = axes
        self.content = content()
    }

    var body: some View {
        if Theme.renderFlat {
            if axes == .horizontal { HStack(spacing: 0) { content }.fixedSize(horizontal: true, vertical: false).frame(maxWidth: .infinity, alignment: .leading).clipped() }
            else { VStack(spacing: 0) { content }.frame(maxHeight: .infinity, alignment: .top).clipped() }
        } else {
            ScrollView(axes) { content }
        }
    }
}
