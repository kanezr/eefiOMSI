import SwiftUI

// The launcher's other pages, as the desktop launcher has them (`launcher/ui/js/pages/*.js`).

/// Two cards side by side where there is room, one under the other where there is not.
struct TwoColumns<Left: View, Right: View>: View {
    var rightWidth: CGFloat? = nil
    @ViewBuilder var left: Left
    @ViewBuilder var right: Right
    @Environment(\.compactLayout) private var compact

    @State var width: CGFloat = 1000

    var body: some View {
        Group {
            if !compact && width >= 780 {
                HStack(alignment: .top, spacing: 20) {
                    VStack(spacing: 16) { left }.frame(maxWidth: .infinity)
                    VStack(spacing: 16) { right }.frame(maxWidth: rightWidth ?? .infinity)
                }
            } else {
                VStack(spacing: 16) { left; right }
            }
        }
        .frame(maxWidth: .infinity)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
    }
}

// --- Multiplayer -----------------------------------------------------------------------------

struct MultiplayerPage: View {
    @Environment(LauncherStore.self) private var store
    @State private var tab = "code"
    @State private var address = ""
    @State private var name = ""
    @State private var code = ""

    var body: some View {
        PageScroll("Multiplayer", subtitle: "Drive with friends by a code, or on servers that are always on.") {
            UnderlineTabs(items: [("code", "Connect by Code"), ("servers", "Servers")], selection: $tab)
            if tab == "code" { byCode } else { servers }
        }
        .onAppear { for s in store.snapshot.servers { store.act("server_ask", ["address": s.address, "every": 10]) } }
    }

    private var byCode: some View {
        TwoColumns {
            Card("Host a game") {
                Text("Others join with the session code the game shows (Esc menu → copy the code).").font(.inter(14)).foregroundStyle(Theme.muted)
                ToggleRow(label: "Host the next drive", help: "The duty you start on the Drive page is opened to others", isOn: store.choice.lan_mode == "host") { on in
                    store.act("lan", ["mode": on ? "host" : "off"])
                }
            }
        } right: {
            Card("Connect by Code") {
                Field("Session code") {
                    InputField(prompt: "OMSI-XXXX-… or an address", text: $code) { store.act("lan", ["mode": "join", "addr": code]) }
                }
                if store.choice.lan_mode == "join", !store.snapshot.join.text.isEmpty {
                    Notice(text: store.snapshot.join.text, kind: store.snapshot.join.ok ? .info : .danger)
                }
                HStack(spacing: 8) {
                    Button { store.act("lan", ["mode": "join", "addr": code]) } label: { Label("Join", systemImage: "arrow.right.circle.fill") }
                        .buttonStyle(.primary).disabled(code.isEmpty)
                    if store.choice.lan_mode != "off" {
                        Button("Play alone") { store.act("server_leave"); store.act("lan", ["mode": "off"]) }.buttonStyle(.secondary)
                    }
                }
            }
        }
    }

    private var servers: some View {
        VStack(spacing: 16) {
            Card("Add server") {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 8) { nameField.frame(width: 180); addressField; addButton }
                    VStack(spacing: 8) { nameField; addressField; addButton.frame(maxWidth: .infinity, alignment: .trailing) }
                }
            }
            if store.snapshot.servers.isEmpty {
                Card { EmptyState(icon: "globe", text: "No servers yet. Add a server by the address its owner gives you.") }
            }
            ForEach(store.snapshot.servers) { s in
                ServerCard(server: s, joined: store.snapshot.joined_server == s.address)
            }
        }
    }

    private var nameField: some View { InputField(prompt: "Name (optional)", text: $name) }
    private var addressField: some View { InputField(prompt: "Address: https://… or host:port", text: $address) }
    private var addButton: some View {
        Button { store.act("server_add", ["name": name, "address": address]); address = ""; name = "" } label: { Label("Add", systemImage: "plus") }
            .buttonStyle(.secondary).disabled(address.trimmingCharacters(in: .whitespaces).isEmpty)
    }
}

struct ServerCard: View {
    @Environment(LauncherStore.self) private var store
    var server: Server
    var joined: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: server.official == true ? "checkmark.seal.fill" : "server.rack").font(.system(size: 18)).foregroundStyle(joined ? Theme.accent : Theme.muted)
                .frame(width: 36, height: 36).background(Theme.surface2, in: RoundedRectangle(cornerRadius: Theme.radiusSm, style: .continuous))
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 8) {
                    Text(server.info?.name?.isEmpty == false ? server.info!.name! : (server.name.isEmpty ? server.address : server.name)).font(.inter(15, .semibold)).lineLimit(1)
                    if joined { Tag(text: "Joined", kind: .accent) }
                    if server.info?.password == true { Image(systemName: "lock.fill").font(.system(size: 11)).foregroundStyle(Theme.muted) }
                }
                if let i = server.info {
                    if i.ok {
                        Text("\(i.players ?? 0)/\(i.max_players ?? 0) players - \(i.map ?? "") - \(i.time ?? "") \(i.weather ?? "")").font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(1)
                        if let m = i.motd, !m.isEmpty { Text(m).font(.inter(13)).foregroundStyle(Theme.text2).lineLimit(2) }
                    } else {
                        Text(i.error ?? String(localized: "No answer")).font(.inter(12.5)).foregroundStyle(Theme.danger).lineLimit(1)
                    }
                } else {
                    Text("Asking…").font(.inter(12.5)).foregroundStyle(Theme.muted)
                }
            }
            Spacer(minLength: 8)
            if joined {
                Button("Leave") { store.act("server_leave") }.buttonStyle(.launcher(.plain, small: true))
            } else {
                Button("Join") { store.act("server_join", ["address": server.address]); store.page = .drive; store.phoneTab = "drive" }.buttonStyle(.launcher(.primary, small: true))
            }
            Button { store.act("server_ask", ["address": server.address, "every": 10]) } label: { Image(systemName: "arrow.clockwise") }
                .buttonStyle(.launcher(.ghost, small: true)).help("Refresh")
            if server.official != true {
                Button { store.act("server_remove", ["address": server.address]) } label: { Image(systemName: "trash") }
                    .buttonStyle(.launcher(.ghost, small: true)).help("Remove from the list")
            }
        }
        .padding(.horizontal, 20).padding(.vertical, 18)
        .surface()
    }
}

// --- Profile ---------------------------------------------------------------------------------

struct ProfilePage: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact
    @State private var newName = ""
    @State private var confirmDelete = false

    var body: some View {
        PageScroll("Profile", subtitle: "The driver whose personnel file the game writes: hours, kilometres, punctuality, tickets.") {
            driver
            if let p = store.snapshot.profile {
                Card {
                    HStack(spacing: 18) {
                        Avatar(name: p.name, size: 64)
                        VStack(alignment: .leading, spacing: 4) {
                            HStack(spacing: 8) {
                                Text(p.name).font(.inter(22, .semibold))
                                if !p.exists { Text("(no personnel file yet)").font(.inter(12.5)).foregroundStyle(Theme.muted) }
                            }
                            Text("Level \(p.level) - \(p.xp) XP, \(max(0, p.next_level_xp - p.xp)) to level \(p.level + 1)").font(.inter(14)).foregroundStyle(Theme.muted)
                            ProgressBar(value: progress(p)).frame(maxWidth: 420).padding(.top, 6)
                        }
                    }
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: compact ? 140 : 150), spacing: 12)], spacing: 12) {
                    StatTile(icon: "clock.fill", value: String(format: "%d h %02d min", Int(p.hours), Int((p.hours * 60).truncatingRemainder(dividingBy: 60))), label: "hours driven")
                    StatTile(icon: "point.topleft.down.to.point.bottomright.curvepath.fill", value: String(format: "%.1f km", p.km), label: "distance")
                    StatTile(icon: "mappin.circle.fill", value: "\(p.stops)", label: "stops served")
                    StatTile(icon: "clock.arrow.circlepath", value: "\(p.early) / \(p.late)", label: "early / late")
                    StatTile(icon: "ticket.fill", value: "\(Int(p.tickets))", label: "tickets sold")
                    StatTile(icon: "banknote.fill", value: String(format: "%.2f", p.cash), label: "takings")
                    StatTile(icon: "exclamationmark.triangle.fill", value: "\(p.crashes)", label: "crashes")
                    StatTile(icon: "figure.walk", value: "\(p.hurt)", label: "pedestrians hurt")
                    StatTile(icon: "bus.fill", value: "\(Int(p.rating_driving)) %", label: "driving")
                    StatTile(icon: "person.2.fill", value: "\(Int(p.rating_comfort)) %", label: "comfort")
                    StatTile(icon: "ticket.fill", value: "\(Int(p.rating_tickets)) %", label: "ticket selling")
                    StatTile(icon: "gamecontroller.fill", value: "\(p.sessions.count)", label: "runs")
                }
                runs(p.sessions)
            } else {
                Card { EmptyState(icon: "person.fill", title: "No driver yet", text: "Create a driver to start a personnel file.") }
            }
        }
    }

    private var driver: some View {
        Card("Driver") {
            let columns = compact ? [GridItem(.flexible())] : [GridItem(.flexible(), spacing: 16), GridItem(.flexible(), spacing: 16)]
            LazyVGrid(columns: columns, alignment: .leading, spacing: 12) {
                HStack(spacing: 8) {
                    SelectField(options: store.snapshot.profiles.map { (value: $0, label: $0) }, selection: store.snapshot.config.profile, onChange: { store.act("profile", ["name": $0]) }, placeholder: "Choose a driver")
                    Button { confirmDelete = true } label: { Image(systemName: "trash") }.buttonStyle(.destructive).help("Delete this driver")
                        .disabled(store.snapshot.config.profile.isEmpty)
                        .confirmationDialog("Delete \(store.snapshot.config.profile)?", isPresented: $confirmDelete) {
                            Button("Delete", role: .destructive) { store.act("profile_delete", ["name": store.snapshot.config.profile]) }
                        } message: {
                            Text("The personnel file of \(store.snapshot.config.profile) is deleted with all its figures.")
                        }
                }
                HStack(spacing: 8) {
                    InputField(prompt: "New driver's name", text: $newName) { create() }
                    Button { create() } label: { Label("Create", systemImage: "plus") }
                        .buttonStyle(.secondary).disabled(newName.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }

    private func create() {
        guard !newName.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        store.act("profile_create", ["name": newName])
        newName = ""
    }

    private func runs(_ sessions: [Session]) -> some View {
        Card("Recent runs") {
            if sessions.isEmpty {
                EmptyState(icon: "clock.arrow.circlepath", text: "No runs yet. Drive a duty and it shows up here.")
            } else if compact {
                VStack(alignment: .leading, spacing: 12) {
                    ForEach(sessions.prefix(30), id: \.self) { s in
                        HStack(spacing: 10) {
                            if let l = s.line { LineBadge(line: l) }
                            VStack(alignment: .leading, spacing: 2) {
                                Text("\(s.bus.stem) - \(mapName(s.map))").font(.inter(14, .medium)).lineLimit(1)
                                Text("\(ago(s.time)) - \(duration(s.seconds)) - \(String(format: "%.1f", s.metres / 1000)) km - \(s.stops) stops").font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(1)
                            }
                        }
                    }
                }
            } else {
                Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 0) {
                    GridRow {
                        ForEach(["When", "Duty", "Bus", "Time", "Distance", "Stops", "Tickets", "Crashes"], id: \.self) { h in
                            Text(LocalizedStringKey(h)).font(.inter(12, .semibold)).foregroundStyle(Theme.muted).padding(.vertical, 8)
                        }
                    }
                    ForEach(Array(sessions.prefix(50).enumerated()), id: \.offset) { i, s in
                        GridRow {
                            Text(ago(s.time)).foregroundStyle(Theme.muted)
                            HStack(spacing: 6) {
                                if let l = s.line { LineBadge(line: l); if let t = s.tour { Text("/ \(t)") } } else { Text("Free drive") }
                                Text(mapName(s.map)).foregroundStyle(Theme.muted)
                            }
                            Text(s.bus.stem).lineLimit(1)
                            Text(duration(s.seconds)).monospacedDigit()
                            Text(String(format: "%.1f km", s.metres / 1000)).monospacedDigit()
                            Text("\(s.stops)").monospacedDigit()
                            Text("\(s.tickets)").monospacedDigit()
                            Text("\(s.crashes)").monospacedDigit()
                        }
                        .font(.inter(14))
                        .padding(.vertical, 8)
                        .background(i % 2 == 1 ? Color.white.opacity(0.018) : .clear)
                    }
                }
            }
        }
    }

    private func mapName(_ m: String) -> String {
        m.replacingOccurrences(of: "\\", with: "/").replacingOccurrences(of: "/global.cfg", with: "").replacingOccurrences(of: "maps/", with: "")
    }

    private func duration(_ s: Double) -> String {
        String(format: "%d:%02d", Int(s) / 3600, (Int(s) % 3600) / 60)
    }

    private func ago(_ t: UInt64) -> String {
        Date(timeIntervalSince1970: TimeInterval(t)).formatted(.relative(presentation: .named))
    }

    private func progress(_ p: Profile) -> Double {
        guard p.next_level_xp > 0 else { return 0 }
        let prev = (p.level - 1) * (p.level - 1) * 250
        return min(1, max(0, Double(p.xp - prev) / Double(max(1, p.next_level_xp - prev))))
    }
}

// --- Controls --------------------------------------------------------------------------------

struct ControlsPage: View {
    @Environment(LauncherStore.self) private var store

    var body: some View {
        let touch = store.platform == "ios"
        PageScroll("Controls", subtitle: touch ? "Touch, tilt and game controllers." : "Keys, wheels and gamepads.") {
            TwoColumns {
                if touch {
                    Card("Touch controls") {
                        SettingSelect(label: "Door buttons", key: "touch_door_mode", options: [("standard", "Standard (physical doors)"), ("direct", "Direct OMSI triggers")])
                        Text("Tilt steering is switched in the game (the steering wheel button). A game controller (Xbox, PlayStation, MFi) works as soon as it is connected.").font(.inter(13)).foregroundStyle(Theme.muted)
                    }
                }
                Card("Key bindings") {
                    Notice(text: String(localized: "The keys of the game are OMSI's Inputs/keyboard.cfg: the openOMSI launcher of Windows and Linux sets them on its Controls page, which reads and writes the same file."))
                    SettingSelect(label: "Driving keys", key: "drive_keys", options: [("omsi", "Custom controls"), ("simple", "W A S D + arrows"), ("wasd", "W A S D only"), ("arrows", "Arrow keys only")])
                }
            } right: {
                Card("Game controllers") {
                    SettingSelect(label: "Gamepad type", key: "pad_type", options: [("auto", "Automatic"), ("xbox", "Xbox"), ("ps4", "PlayStation 4"), ("ps5", "PlayStation 5")])
                    SettingToggle(label: "Default gamepad buttons (indicators, doors, views, gears, menu, pause)", key: "pad_buttons", fallback: true)
                    SettingToggle(label: "Force feedback and vibration", key: "ff_enabled", fallback: true)
                }
            }
        }
    }
}

// --- Running games ---------------------------------------------------------------------------

struct GamesPage: View {
    @Environment(LauncherStore.self) private var store
    @State private var log: [String] = []
    @State private var logPid: UInt32?

    var body: some View {
        PageScroll("Running games", subtitle: "Any number of games can run at once; each writes its own log.", maxWidth: 980) {
            if store.snapshot.instances.isEmpty {
                Card { EmptyState(icon: "gamecontroller.fill", title: "No game is running", text: "Start one on the Drive page.") }
            }
            ForEach(store.snapshot.instances) { i in
                Card {
                    HStack(spacing: 12) {
                        Circle().fill(i.running == true ? Theme.ok : Theme.muted).frame(width: 8, height: 8)
                        VStack(alignment: .leading, spacing: 3) {
                            HStack(spacing: 8) {
                                if let l = i.line { LineBadge(line: l) }
                                Text("\(i.bus.stem) - \(i.map.replacingOccurrences(of: "/global.cfg", with: "").replacingOccurrences(of: "maps/", with: ""))").font(.inter(15, .semibold)).lineLimit(1)
                            }
                            Text("process \(i.pid) - \(i.profile) - \(i.lan)").font(.inter(12.5)).foregroundStyle(Theme.muted)
                        }
                        Spacer()
                        Button {
                            if logPid == i.pid { logPid = nil; return }
                            logPid = i.pid
                            log = (try? JSONDecoder().decode([String].self, from: Bridge.call("act", ["do": "log", "pid": i.pid]))) ?? []
                        } label: { Label(logPid == i.pid ? "Hide log" : "Log", systemImage: "doc.text") }
                            .buttonStyle(.launcher(.plain, small: true))
                        Button { Platform.reveal(i.log) } label: { Image(systemName: "folder") }
                            .buttonStyle(.launcher(.ghost, small: true)).help("Show the log file")
                        if i.running == true {
                            Button("Stop") { store.act("stop", ["pid": i.pid]) }.buttonStyle(.launcher(.danger, small: true))
                        }
                    }
                    if logPid == i.pid { LogBlock(text: log.joined(separator: "\n")) }
                }
            }
        }
    }
}

// --- Mods ------------------------------------------------------------------------------------

struct ModsPage: View {
    @Environment(LauncherStore.self) private var store
    @State private var search = ""
    @State private var kind = "all"
    @State private var mode = "auto"

    var body: some View {
        PageScroll("Mods", subtitle: "A bus, a map, scenery, a whole OMSI folder - as a folder or a .zip, .7z or .rar. The original OMSI 2 folder is never written to.") {
            TwoColumns(rightWidth: 360) {
                installed
                if let w = store.snapshot.mods?.waiting, !w.isEmpty {
                    Card("Waiting for their bus", hint: "Packs installed before the bus they belong to: they go in once it is there.") {
                        ForEach(w, id: \.self) { Text($0).font(.inter(14)).foregroundStyle(Theme.text2) }
                    }
                }
            } right: {
                install
                jobs
                if let m = store.snapshot.mods {
                    Card("The Mods folder") {
                        Text("Anything put into this folder is installed by itself once it has finished copying.").font(.inter(13.5)).foregroundStyle(Theme.muted)
                        HStack(spacing: 8) {
                            Text(m.inbox).font(.inter(12.5)).foregroundStyle(Theme.text2).lineLimit(1).truncationMode(.middle).textSelection(.enabled)
                            Spacer(minLength: 4)
                            Button { Platform.reveal(m.inbox) } label: { Image(systemName: "folder") }.buttonStyle(.launcher(.plain, small: true)).help("Open")
                        }
                    }
                }
            }
        }
        .onAppear { store.act("mods") }
    }

    private var install: some View {
        Card("Install a mod") {
            HStack(spacing: 8) {
                Button { pick(folder: true) } label: { Label("Folder", systemImage: "folder.fill") }.buttonStyle(.launcher(.primary, wide: true))
                Button { pick(folder: false) } label: { Label("Archive", systemImage: "archivebox.fill") }.buttonStyle(.launcher(.plain, wide: true))
            }
            Field("Archive install mode") {
                Segmented(items: [("auto", "Auto"), ("extract", "Unpacked"), ("inplace", "Used in place")], selection: mode, full: true) { mode = $0 }
            }
        }
    }

    private var jobs: some View {
        Card("Installs") {
            if store.snapshot.jobs.isEmpty {
                Text("Nothing installed since the launcher started. Big archives are checked against the free disk space before anything is unpacked.").font(.inter(13.5)).foregroundStyle(Theme.muted)
            }
            ForEach(store.snapshot.jobs) { j in
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Text(j.name).font(.inter(14, .medium)).lineLimit(1)
                        Spacer()
                        Tag(text: j.state, kind: j.state == "failed" ? .danger : (j.state == "done" ? .ok : .accent))
                    }
                    if j.finished == nil && j.bytes_total > 0 {
                        ProgressBar(value: Double(j.bytes_done) / Double(max(1, j.bytes_total)))
                    }
                    if !j.message.isEmpty { Text(j.message).font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(3) }
                }
            }
        }
    }

    private var installed: some View {
        let mods = store.snapshot.mods?.installed ?? []
        let shown = mods.filter { m in
            (search.isEmpty || m.name.localizedCaseInsensitiveContains(search)) && (kind == "all" || (kind == "off" ? !m.enabled : (m.kind ?? "Other").lowercased() == kind))
        }
        return Card("Installed mods", hint: "\(mods.count) mods, \(mods.filter(\.enabled).count) on - \(formatBytes(store.snapshot.mods?.free_bytes ?? 0)) free") {
            SearchField(prompt: "Search mods...", text: $search)
            Scroll(.horizontal) {
                Segmented(items: [("all", "All"), ("bus", "Buses"), ("map", "Maps"), ("archive", "Archives"), ("other", "Other"), ("off", "Off")], selection: kind) { kind = $0 }
            }
            .scrollIndicators(.never)
            if shown.isEmpty {
                EmptyState(icon: "puzzlepiece.extension.fill", text: search.isEmpty ? "No mods yet. Choose a folder or an archive." : "No mod matches.")
            }
            VStack(spacing: 0) {
                ForEach(shown) { m in
                    HStack(spacing: 12) {
                        Image(systemName: icon(m)).font(.system(size: 15)).foregroundStyle(m.enabled ? Theme.text2 : Theme.muted).frame(width: 22)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(m.name).font(.inter(14, .medium)).foregroundStyle(m.enabled ? Theme.text : Theme.muted).lineLimit(1)
                            Text("\(m.paths.prefix(2).joined(separator: ", "))\(m.paths.count > 2 ? " +\(m.paths.count - 2)" : "") - \(formatBytes(m.bytes ?? 0))").font(.inter(12.5)).foregroundStyle(Theme.muted).lineLimit(1)
                        }
                        Spacer()
                        if store.snapshot.mod_busy == m.id {
                            ProgressView().controlSize(.small).tint(Theme.accent)
                        } else {
                            Toggle("", isOn: Binding(get: { m.enabled }, set: { store.act("mod_toggle", ["id": m.id, "on": $0]) })).labelsHidden().toggleStyle(.launcher)
                            Button { store.act("mod_remove", ["id": m.id]) } label: { Image(systemName: "trash") }
                                .buttonStyle(.launcher(.ghost, small: true)).help("Delete this mod")
                        }
                    }
                    .padding(.horizontal, 4).padding(.vertical, 8)
                }
            }
            if let dir = store.snapshot.mods?.content_dir {
                HStack {
                    Text(dir).font(.inter(12)).foregroundStyle(Theme.muted).lineLimit(1).truncationMode(.middle)
                    Spacer()
                    Button { Platform.reveal(dir) } label: { Label("Open", systemImage: "arrow.up.forward.square") }.buttonStyle(.launcher(.ghost, small: true))
                }
            }
        }
    }

    private func icon(_ m: ModEntry) -> String {
        switch (m.kind ?? "").lowercased() {
        case "bus": "bus.fill"
        case "map": "map.fill"
        case "archive": "archivebox.fill"
        default: "puzzlepiece.extension.fill"
        }
    }

    private func pick(folder: Bool) {
        Platform.pick(folder: folder, types: folder ? [] : ["zip", "7z", "rar"]) { url in
            guard let url else { return }
            Task { @MainActor in
                let path = Platform.importForInstall(url)
                store.act("mod_install", ["path": path, "mode": mode])
            }
        }
    }
}

// --- Tutorials -------------------------------------------------------------------------------

struct TutorialsPage: View {
    @Environment(LauncherStore.self) private var store
    @Environment(\.compactLayout) private var compact

    var body: some View {
        PageScroll("Tutorials", subtitle: "OMSI 2's lessons: driving the bus, the stops, the ticket printer and the timetable.") {
            if store.tutorials.isEmpty {
                Card { EmptyState(icon: "book.fill", title: "No tutorial found", text: "The tutorials come with OMSI 2 (its Tutorials folder).") }
            }
            LazyVGrid(columns: compact ? [GridItem(.flexible())] : [GridItem(.flexible(), spacing: 16), GridItem(.flexible(), spacing: 16)], alignment: .leading, spacing: 16) {
                ForEach(store.tutorials) { t in
                    Card("\(t.n). \(t.title)") {
                        Text(t.text).font(.inter(14)).foregroundStyle(Theme.text2).lineLimit(6).frame(maxWidth: .infinity, minHeight: 60, alignment: .topLeading)
                        Button { store.tutorial(t.n) } label: { Label("Start", systemImage: "play.fill") }.buttonStyle(.primary)
                            .disabled(store.snapshot.in_game || store.starting)
                    }
                }
            }
        }
        .onAppear { store.loadTutorials() }
    }
}

// --- Setup -----------------------------------------------------------------------------------

struct SetupPage: View {
    @Environment(LauncherStore.self) private var store
    @State private var root = ""

    var body: some View {
        PageScroll("Setup", subtitle: "openOMSI plays the content of an OMSI 2 installation: the original game must be installed (Steam or another copy).", maxWidth: 900) {
            Card("Folders") {
                if !store.snapshot.root_ok {
                    Notice(text: store.snapshot.root_problem.isEmpty ? String(localized: "The original OMSI 2 was not found.") : store.snapshot.root_problem, kind: .danger)
                }
                Field("OMSI 2 folder", help: Platform.isPhone
                    ? String(localized: "Copy a complete OMSI 2 (the folder with Omsi.exe, maps and Vehicles) into openOMSI in the Files app (On My iPhone / iPad), or from a Mac's Finder over a cable, and choose it here. A copy on iCloud Drive or a USB drive can be chosen as well.")
                    : String(localized: "The OMSI 2 folder is the one with maps and Vehicles in it (any complete installation).")) {
                    HStack(spacing: 8) {
                        InputField(prompt: "/path/to/OMSI 2", text: $root) { store.act("root", ["path": root]) }
                        Button("Browse") {
                            Platform.pick(folder: true) { url in
                                guard let url else { return }
                                Task { @MainActor in
                                    root = Platform.keepAccess(url)
                                    store.act("root", ["path": root])
                                }
                            }
                        }
                        .buttonStyle(.secondary)
                        Button { store.act("root", ["path": root]) } label: { Label("Save", systemImage: "checkmark") }.buttonStyle(.primary)
                    }
                }
            }
            Card("Where things are") {
                Facts(rows: [
                    ("OMSI 2", store.snapshot.config.root.isEmpty ? "-" : store.snapshot.config.root),
                    ("Content folder", store.snapshot.content_dir ?? "-"),
                    ("Driver", store.snapshot.config.profile.isEmpty ? "-" : store.snapshot.config.profile),
                    ("Version", "\(store.version) (\(store.build))"),
                ])
                if let c = store.snapshot.content_dir {
                    HStack(spacing: 8) {
                        Button { Platform.reveal(c) } label: { Label("Show", systemImage: "folder") }.buttonStyle(.secondary)
                        Button { store.act("reload") } label: { Label("Read the content again", systemImage: "arrow.clockwise") }.buttonStyle(.secondary)
                    }
                }
            }
        }
        .onAppear { root = store.snapshot.config.root }
    }
}
