import SwiftUI

struct SettingsView: View {
    @ObservedObject var store: SettingsStore
    @State private var selection: SettingsSection = .paths
    @State private var showVtKey = false

    enum SettingsSection: String, CaseIterable, Identifiable, Hashable {
        case paths, network, daemon, scan, virustotal, watcher, scanners
        var id: String { rawValue }

        var title: String {
            switch self {
            case .paths: return "Paths"
            case .network: return "Network"
            case .daemon: return "Daemon launch"
            case .scan: return "Watch & scan"
            case .virustotal: return "VirusTotal"
            case .watcher: return "Watcher"
            case .scanners: return "Scanners"
            }
        }

        var subtitle: String {
            switch self {
            case .paths:      return "Where files live — watch, quarantine, and database."
            case .network:    return "HTTP endpoint and API token for menubar ↔ daemon."
            case .daemon:     return "How the menubar launches the background service."
            case .scan:       return "Watch scope, scan limits, and inconclusive retention."
            case .virustotal: return "API key used to scan files against VirusTotal."
            case .watcher:    return "Watcher mode controls file handling behavior."
            case .scanners:   return "Configure local and cloud virus scanning."
            }
        }

        var icon: String {
            switch self {
            case .paths:      return "folder.fill"
            case .network:    return "network"
            case .daemon:     return "bolt.horizontal.fill"
            case .scan:       return "eye.fill"
            case .virustotal: return "shield.lefthalf.filled"
            case .watcher:    return "clock.fill"
            case .scanners:   return "magnifyingglass.circle.fill"
            }
        }

        var tint: Color {
            switch self {
            case .paths:      return .blue
            case .network:    return .indigo
            case .daemon:     return .orange
            case .scan:       return .teal
            case .virustotal: return .red
            case .watcher:    return .purple
            case .scanners:   return .green
            }
        }
    }

    private var inconclusiveEnabledBinding: Binding<Bool> {
        Binding(
            get: { store.inconclusiveRetentionDays > 0 },
            set: { on in
                if on {
                    if store.inconclusiveRetentionDays < 1 {
                        store.inconclusiveRetentionDays = 7
                    }
                } else {
                    store.inconclusiveRetentionDays = 0
                }
            }
        )
    }

    var body: some View {
        VStack(spacing: 0) {
            NavigationSplitView {
                sidebar
                    .navigationSplitViewColumnWidth(min: 190, ideal: 210, max: 240)
            } detail: {
                detail
                    .navigationSplitViewColumnWidth(min: 480, ideal: 560)
            }
            .navigationSplitViewStyle(.balanced)

            Divider()
            footerBar
        }
        .frame(minWidth: 740, idealWidth: 800, minHeight: 560, idealHeight: 620)
        .onAppear { store.fetch() }
    }

    // MARK: - Sidebar

    private var sidebar: some View {
        List(selection: $selection) {
            Section {
                ForEach(SettingsSection.allCases) { section in
                    Label {
                        Text(section.title)
                            .font(.system(size: 13))
                    } icon: {
                        iconBadge(section.icon, tint: section.tint, size: 22, cornerRadius: 6)
                    }
                    .padding(.vertical, 2)
                    .tag(section)
                }
            } header: {
                Text("Settings")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundColor(.secondary)
                    .textCase(.uppercase)
            }
        }
        .listStyle(.sidebar)
    }

    // MARK: - Detail

    @ViewBuilder
    private var detail: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                hero(for: selection)

                Group {
                    switch selection {
                    case .paths:      pathsContent
                    case .network:    networkContent
                    case .daemon:     daemonContent
                    case .scan:       scanContent
                    case .virustotal: virusTotalContent
                    case .watcher:    watcherContent
                    case .scanners:   scannersContent
                    }
                }
            }
            .padding(.horizontal, 28)
            .padding(.top, 28)
            .padding(.bottom, 20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private func hero(for section: SettingsSection) -> some View {
        HStack(alignment: .center, spacing: 14) {
            iconBadge(section.icon, tint: section.tint, size: 44, cornerRadius: 11)
            VStack(alignment: .leading, spacing: 3) {
                Text(section.title)
                    .font(.system(size: 22, weight: .bold))
                Text(section.subtitle)
                    .font(.system(size: 12))
                    .foregroundColor(.secondary)
            }
            Spacer()
        }
    }

    private func iconBadge(
        _ name: String,
        tint: Color,
        size: CGFloat,
        cornerRadius: CGFloat
    ) -> some View {
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(tint.gradient)
            .frame(width: size, height: size)
            .overlay(
                Image(systemName: name)
                    .font(.system(size: size * 0.48, weight: .semibold))
                    .foregroundColor(.white)
            )
            .shadow(color: tint.opacity(0.25), radius: 2, y: 1)
    }

    private func card<C: View>(@ViewBuilder content: () -> C) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            content()
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(Color(nsColor: .controlBackgroundColor))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
        )
    }

    private func sectionHeading(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 11, weight: .semibold))
            .foregroundColor(.secondary)
            .textCase(.uppercase)
    }

    // MARK: - Panes

    private var pathsContent: some View {
        card {
            stackedTextField(title: "Watch path",
                             text: $store.watchPath,
                             prompt: "/Users/you/Downloads")
            divider
            stackedTextField(title: "Quarantine path",
                             text: $store.quarantinePath,
                             prompt: "/Users/you/.file-sandbox/quarantine")
            divider
            stackedTextField(title: "Database path",
                             text: $store.databasePath,
                             prompt: "./data/jobs.sqlite")
            helpText("Prompts show only when the field is empty; they are not saved.")
        }
    }

    private var networkContent: some View {
        VStack(alignment: .leading, spacing: 16) {
            card {
                sectionHeading("Endpoint")
                HStack(alignment: .bottom, spacing: 14) {
                    VStack(alignment: .leading, spacing: 5) {
                        fieldLabel("Port")
                        TextField("", text: $store.httpPort,
                                  prompt: Text("3847").foregroundStyle(.tertiary))
                            .textFieldStyle(.roundedBorder)
                            .multilineTextAlignment(.center)
                            .frame(width: 96)
                    }
                    VStack(alignment: .leading, spacing: 5) {
                        fieldLabel("Host")
                        TextField("", text: $store.httpHost,
                                  prompt: Text("127.0.0.1").foregroundStyle(.tertiary))
                            .textFieldStyle(.roundedBorder)
                    }
                    .frame(maxWidth: .infinity)
                }
            }

            card {
                sectionHeading("Authentication")
                stackedSecureField(title: "API token",
                                   text: $store.apiAuthToken,
                                   prompt: "optional")
                helpText("When set, sent as `Authorization: Bearer`. Matches `apiToken` in config.")
            }
        }
    }

    private var daemonContent: some View {
        card {
            stackedTextField(title: "Project path",
                             text: $store.daemonProjectPath,
                             prompt: "/Users/you/dev/file-sandbox")
            divider
            stackedTextField(title: "Node binary",
                             text: $store.daemonNodeBin,
                             prompt: "leave empty to auto-detect via PATH")
            helpText("Used by the Start button. Leave node binary empty to auto-detect via login shell. Runs: `node src/index.ts` (config from config.json).")
            HStack {
                Spacer()
                Button {
                    store.saveDaemonLocal()
                } label: {
                    Label("Save launch settings", systemImage: "tray.and.arrow.down")
                }
                .buttonStyle(.bordered)
                .controlSize(.regular)
            }
        }
    }

    private var scanContent: some View {
        VStack(alignment: .leading, spacing: 16) {
            card {
                sectionHeading("Watch")
                Toggle(isOn: $store.watchRecursive) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Watch subfolders")
                            .font(.system(size: 13, weight: .medium))
                        Text("Off = only files directly inside the watch folder.")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
                .toggleStyle(.switch)
            }

            card {
                sectionHeading("Scan limits")
                rowLabel("Max scan size") {
                    Stepper(value: $store.maxScanMegabytes, in: 1...8192, step: 1) {
                        Text("\(store.maxScanMegabytes) MB")
                            .monospacedDigit()
                            .font(.body.weight(.medium))
                            .frame(minWidth: 80, alignment: .trailing)
                    }
                }
                divider
                rowLabel("Concurrent scans") {
                    Stepper(value: $store.maxConcurrentScans, in: 1...16, step: 1) {
                        Text("\(store.maxConcurrentScans)")
                            .monospacedDigit()
                            .font(.body.weight(.medium))
                            .frame(minWidth: 32, alignment: .trailing)
                    }
                }
                divider
                Toggle(isOn: $store.useSeparateVtProcess) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Run VirusTotal in child process")
                            .font(.system(size: 13, weight: .medium))
                        Text("Isolates the scan — recovers cleanly on timeout.")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
                .toggleStyle(.switch)
                helpText("Stored as `maxScanBytes` (MiB × 1024²), `maxConcurrentScans`, `useSeparateVtProcess`.")
            }

            card {
                sectionHeading("Inconclusive quarantine")
                Toggle(isOn: inconclusiveEnabledBinding) {
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Auto-remove inconclusive files")
                            .font(.system(size: 13, weight: .medium))
                        Text("Purges files that never got a clean/dirty verdict.")
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                }
                .toggleStyle(.switch)

                if store.inconclusiveRetentionDays > 0 {
                    VStack(alignment: .leading, spacing: 10) {
                        HStack {
                            Text("Delete after")
                                .foregroundColor(.secondary)
                            Spacer()
                            Text("\(store.inconclusiveRetentionDays) day\(store.inconclusiveRetentionDays == 1 ? "" : "s")")
                                .monospacedDigit()
                                .font(.body.weight(.semibold))
                        }
                        Slider(
                            value: Binding(
                                get: { Double(store.inconclusiveRetentionDays) },
                                set: { store.inconclusiveRetentionDays = max(1, min(365, Int($0.rounded()))) }
                            ),
                            in: 1...365,
                            step: 1
                        )
                        HStack(spacing: 8) {
                            ForEach([7, 14, 30, 90], id: \.self) { quickDayButton($0) }
                            Spacer()
                        }
                    }
                    .padding(.top, 2)
                } else {
                    Text("Inconclusive items stay until you delete them.")
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                helpText("`inconclusiveRetentionDays` — hourly purge on the daemon.")
            }
        }
    }

    private var virusTotalContent: some View {
        card {
            sectionHeading("API key")
            VStack(alignment: .leading, spacing: 6) {
                fieldLabel("VirusTotal API key")
                HStack(spacing: 8) {
                    Group {
                        if showVtKey {
                            TextField("", text: $store.vtApiKey, prompt: promptKeyHint)
                                .textFieldStyle(.roundedBorder)
                        } else {
                            SecureField("", text: $store.vtApiKey, prompt: promptKeyHint)
                                .textFieldStyle(.roundedBorder)
                        }
                    }
                    .frame(maxWidth: .infinity)

                    Button(action: { showVtKey.toggle() }) {
                        Image(systemName: showVtKey ? "eye.slash.fill" : "eye.fill")
                            .foregroundColor(.secondary)
                            .font(.system(size: 14))
                            .frame(width: 28, height: 24)
                    }
                    .buttonStyle(.plain)
                    .help(showVtKey ? "Hide key" : "Show key")
                }
            }

            HStack(spacing: 6) {
                Image(systemName: "link")
                    .font(.caption)
                    .foregroundColor(.secondary)
                Link("Get a free key at virustotal.com",
                     destination: URL(string: "https://www.virustotal.com")!)
                    .font(.caption)
            }
        }
    }

    private var watcherContent: some View {
        card {
            VStack(alignment: .leading, spacing: 14) {
                sectionHeading("Mode")
                Picker("Mode", selection: $store.watcherMode) {
                    ForEach(WatcherMode.allCases, id: \.self) { m in
                        Text(m.displayName).tag(m)
                    }
                }
                .pickerStyle(.segmented)
                Text(modeExplainer(store.watcherMode))
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
        }
    }

    private var scannersContent: some View {
        VStack(alignment: .leading, spacing: 16) {
            card {
                VStack(alignment: .leading, spacing: 14) {
                    sectionHeading("Local scanner")
                    Toggle(isOn: $store.pompelmiEnabled) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Local scanner (pompelmi/ClamAV)")
                                .font(.system(size: 13, weight: .medium))
                        }
                    }
                    .toggleStyle(.switch)

                    if store.pompelmiEnabled {
                        stackedTextField(title: "clamd socket path",
                                       text: $store.pompelmiSocketPath,
                                       prompt: "/tmp/clamd.sock")
                        divider
                        Picker("On scan error", selection: $store.pompelmiFailureMode) {
                            Text("Bypass to VT").tag("bypass")
                            Text("Mark inconclusive").tag("inconclusive")
                        }
                    }
                }
            }

            card {
                VStack(alignment: .leading, spacing: 14) {
                    sectionHeading("Cloud scanner")
                    Toggle(isOn: $store.vtEnabled) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text("VirusTotal cloud")
                                .font(.system(size: 13, weight: .medium))
                        }
                    }
                    .toggleStyle(.switch)

                    if !store.pompelmiEnabled && !store.vtEnabled {
                        Text("No active scanners - every new file will be quarantined as inconclusive.")
                            .font(.caption)
                            .foregroundColor(.red)
                    }
                }
            }
        }
    }

    // MARK: - Helpers

    private var divider: some View {
        Rectangle()
            .fill(Color.primary.opacity(0.06))
            .frame(height: 1)
    }

    private var promptKeyHint: Text {
        Text("Paste key from virustotal.com").foregroundStyle(.tertiary)
    }

    private func stackedTextField(
        title: String,
        text: Binding<String>,
        prompt: String
    ) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            fieldLabel(title)
            TextField("", text: text, prompt: Text(prompt).foregroundStyle(.tertiary))
                .textFieldStyle(.roundedBorder)
                .lineLimit(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func stackedSecureField(
        title: String,
        text: Binding<String>,
        prompt: String
    ) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            fieldLabel(title)
            SecureField("", text: text, prompt: Text(prompt).foregroundStyle(.tertiary))
                .textFieldStyle(.roundedBorder)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func fieldLabel(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 11, weight: .semibold))
            .foregroundColor(.secondary)
            .textCase(.uppercase)
    }

    private func helpText(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundColor(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }

    private func rowLabel<V: View>(_ title: String, @ViewBuilder content: () -> V) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title)
                .font(.system(size: 13))
            Spacer()
            content()
        }
    }

    private func quickDayButton(_ days: Int) -> some View {
        let on = store.inconclusiveRetentionDays == days
        return Button {
            store.inconclusiveRetentionDays = days
        } label: {
            Text("\(days)d")
                .font(.caption.weight(.semibold))
                .padding(.horizontal, 12)
                .padding(.vertical, 5)
                .background(
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .fill(on ? Color.accentColor : Color.secondary.opacity(0.15))
                )
                .foregroundColor(on ? .white : .primary)
        }
        .buttonStyle(.plain)
    }

    private func modeExplainer(_ m: WatcherMode) -> String {
        switch m {
        case .active: return "Files are quarantined and scanned."
        case .scanPaused: return "Files are quarantined but not scanned. Restore manually after review."
        case .monitoringDisabled: return "Watcher ignores new files entirely. Advanced - files are not protected."
        }
    }

    // MARK: - Footer

    private var footerBar: some View {
        HStack(spacing: 12) {
            statusView
            Spacer()
            Button {
                store.save()
            } label: {
                Text("Save")
                    .frame(minWidth: 72)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .disabled(store.isSaving || store.isLoading)
            .keyboardShortcut(.return, modifiers: [.command])
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 14)
        .background(.ultraThinMaterial)
    }

    @ViewBuilder
    private var statusView: some View {
        if store.isLoading {
            inlineProgress("Loading…")
        } else if store.isSaving {
            inlineProgress("Saving…")
        } else if let result = store.saveResult {
            if result == "ok" {
                statusPill(icon: "checkmark.circle.fill",
                           color: .green,
                           text: "Saved — restart daemon to apply")
            } else {
                statusPill(icon: "xmark.octagon.fill",
                           color: .red,
                           text: result.replacingOccurrences(of: "err:", with: ""))
            }
        } else if let err = store.loadError {
            statusPill(icon: "wifi.exclamationmark",
                       color: .orange,
                       text: err)
        } else {
            Text("⌘↩ to save")
                .font(.caption)
                .foregroundColor(.secondary)
        }
    }

    private func inlineProgress(_ text: String) -> some View {
        HStack(spacing: 6) {
            ProgressView().controlSize(.small)
            Text(text).font(.caption).foregroundColor(.secondary)
        }
    }

    private func statusPill(icon: String, color: Color, text: String) -> some View {
        HStack(spacing: 6) {
            Image(systemName: icon).foregroundColor(color)
            Text(text)
                .font(.caption)
                .foregroundColor(.primary)
                .lineLimit(1)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .background(
            Capsule().fill(color.opacity(0.12))
        )
        .overlay(
            Capsule().strokeBorder(color.opacity(0.25), lineWidth: 0.5)
        )
    }
}
