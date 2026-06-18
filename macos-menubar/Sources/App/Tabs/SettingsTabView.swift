import SwiftUI
import Combine

struct SettingsTabView: View {
    @ObservedObject var settingsStore: SettingsStore
    @ObservedObject var store: JobStore
    @AppStorage("filesandbox.locale") private var localeRaw: String = AppLocale.auto.rawValue

    /// Debounced auto-save: any @Published change triggers `save()` 400 ms later.
    @State private var saveTimer: AnyCancellable? = nil

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                SettingGroupHeader(title: "Watcher")
                SettingRow(label: "Mode") {
                    Picker("", selection: $store.mode) {
                        Text("Active").tag(WatcherMode.active)
                        Text("Paused").tag(WatcherMode.scanPaused)
                        Text("Off").tag(WatcherMode.monitoringDisabled)
                    }
                    .pickerStyle(.segmented)
                    .frame(width: 220)
                    .onChange(of: store.mode) { _, m in store.setMode(m) }
                }
                SettingRow(label: "Watch path") {
                    TextField("", text: $settingsStore.watchPath, onCommit: scheduleSave)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(width: 180)
                }
                SettingRow(label: "Quarantine path") {
                    TextField("", text: $settingsStore.quarantinePath, onCommit: scheduleSave)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(width: 180)
                }

                SettingGroupHeader(title: "Scanners")
                SettingRow(label: "Local (pompelmi)") {
                    AppSwitch(isOn: bind($settingsStore.pompelmiEnabled))
                }
                if settingsStore.pompelmiEnabled {
                    SettingRow(label: "clamd socket", indent: 16) {
                        TextField("", text: $settingsStore.pompelmiSocketPath, onCommit: scheduleSave)
                            .textFieldStyle(.roundedBorder)
                            .font(.system(size: 11, design: .monospaced))
                            .frame(width: 180)
                    }
                    SettingRow(label: "On scan error", indent: 16) {
                        Picker("", selection: bind($settingsStore.pompelmiFailureMode)) {
                            Text("Bypass to VT").tag("bypass")
                            Text("Mark inconclusive").tag("inconclusive")
                        }
                        .pickerStyle(.menu)
                        .frame(width: 220)
                        .labelsHidden()
                    }
                }
                SettingRow(label: "VirusTotal") {
                    AppSwitch(isOn: bind($settingsStore.vtEnabled))
                }
                if !settingsStore.vtEnabled && !settingsStore.pompelmiEnabled {
                    Text("No active scanners - every new file will be quarantined as inconclusive.")
                        .font(.system(size: 10))
                        .foregroundColor(Theme.verdictRedFg)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 7)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .overlay(Divider(), alignment: .bottom)
                }

                SettingGroupHeader(title: "Advanced")
                SettingRow(label: "Max scan size (MiB)") {
                    Stepper(value: bind($settingsStore.maxScanMegabytes), in: 1...10000, step: 50) {
                        Text("\(settingsStore.maxScanMegabytes)")
                            .font(.system(size: 11, design: .monospaced))
                    }
                }
                SettingRow(label: "Max concurrent VT scans") {
                    Stepper(value: bind($settingsStore.maxConcurrentScans), in: 1...8, step: 1) {
                        Text("\(settingsStore.maxConcurrentScans)")
                            .font(.system(size: 11, design: .monospaced))
                    }
                }
                SettingRow(label: "Use separate VT process") {
                    AppSwitch(isOn: bind($settingsStore.useSeparateVtProcess))
                }
                SettingRow(label: "Inconclusive retention (days)") {
                    Stepper(value: bind($settingsStore.inconclusiveRetentionDays), in: 0...90, step: 1) {
                        Text("\(settingsStore.inconclusiveRetentionDays)")
                            .font(.system(size: 11, design: .monospaced))
                    }
                }
                SettingRow(label: "API token") {
                    SecureField("", text: $settingsStore.apiAuthToken, onCommit: scheduleSave)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(width: 180)
                }
                SettingRow(label: "VT API key") {
                    SecureField("", text: $settingsStore.vtApiKey, onCommit: scheduleSave)
                        .textFieldStyle(.roundedBorder)
                        .font(.system(size: 11, design: .monospaced))
                        .frame(width: 180)
                }
                SettingRow(label: "Language") {
                    Picker("", selection: $localeRaw) {
                        ForEach(AppLocale.allCases) { loc in
                            Text(loc.displayName).tag(loc.rawValue)
                        }
                    }
                    .pickerStyle(.menu)
                    .frame(width: 180)
                    .labelsHidden()
                }
            }
        }
        .frame(maxHeight: 520)
        .onAppear { settingsStore.fetch() }
    }

    /// Wraps a Binding so any change schedules a debounced save.
    private func bind<V: Equatable>(_ source: Binding<V>) -> Binding<V> {
        Binding(
            get: { source.wrappedValue },
            set: { newValue in
                source.wrappedValue = newValue
                scheduleSave()
            }
        )
    }

    private func scheduleSave() {
        saveTimer?.cancel()
        saveTimer = Just(())
            .delay(for: .milliseconds(400), scheduler: DispatchQueue.main)
            .sink { _ in settingsStore.save() }
    }
}
