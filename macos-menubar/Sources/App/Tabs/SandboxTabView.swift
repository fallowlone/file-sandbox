import SwiftUI
import AppKit

struct SandboxTabView: View {
    @ObservedObject var store: SandboxStore
    @ObservedObject var settingsStore: SettingsStore
    @State private var newSessionNetwork: Bool = false
    @State private var didInitNetwork = false

    var body: some View {
        VStack(spacing: 0) {
            topStrip
            Divider()
            if !settingsStore.sandboxEnabled {
                Text("Sandbox is disabled in Settings")
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 28)
            } else if let err = store.loadError {
                Text(err)
                    .font(.caption)
                    .foregroundColor(.red)
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else if store.sessions.isEmpty {
                emptyState
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(store.sessions) { s in
                            SandboxRowView(session: s) {
                                store.discard(s.id)
                            }
                        }
                    }
                }
                .frame(maxHeight: 520)
            }
        }
        .onAppear {
            store.fetch()
            if !didInitNetwork {
                newSessionNetwork = settingsStore.sandboxNetworkDefault
                didInitNetwork = true
            }
        }
    }

    private var topStrip: some View {
        HStack(spacing: 10) {
            Button {
                pickFileAndOpen()
            } label: {
                Label("New session", systemImage: "plus")
                    .font(.system(size: 11, weight: .medium))
                    .padding(.horizontal, 11)
                    .padding(.vertical, 6)
                    .overlay(
                        RoundedRectangle(cornerRadius: Theme.cornerRadiusButton)
                            .strokeBorder(Theme.separator, lineWidth: 1)
                    )
            }
            .buttonStyle(.plain)
            .disabled(!store.canOpen)
            .help(store.canOpen ? "Pick a file and open it in a fresh sandbox VM" : "Install Tart to enable")

            Spacer()
            Text("Network")
                .font(.system(size: 11, weight: .medium))
            AppSwitch(isOn: $newSessionNetwork)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
    }

    private var emptyState: some View {
        VStack(spacing: 6) {
            Image(systemName: "shield")
                .font(.system(size: 22))
                .foregroundColor(.secondary)
            Text("No sandbox sessions")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
            Text("Click + New session to spawn a VM")
                .font(.system(size: 10))
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 28)
    }

    private func pickFileAndOpen() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            store.create(filePath: url.path, sourceJobId: nil, network: newSessionNetwork) { _ in }
        }
    }
}

private struct SandboxRowView: View {
    let session: SandboxSession
    let onDiscard: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Image(systemName: fileSymbol(for: session.sourceFilePath))
                        .font(.system(size: 13))
                        .foregroundColor(.secondary)
                    Text((session.sourceFilePath as NSString).lastPathComponent)
                        .font(.system(size: 12, weight: .medium))
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                HStack(spacing: 6) {
                    Text(session.vmName)
                        .font(.system(size: 9, design: .monospaced))
                        .foregroundColor(.secondary)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 1)
                        .background(Theme.subtleBg)
                        .overlay(
                            RoundedRectangle(cornerRadius: 4)
                                .strokeBorder(Theme.separator, lineWidth: 1)
                        )
                        .clipShape(RoundedRectangle(cornerRadius: 4))
                    Text("·")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    Text(ageString(from: session.lastActiveAt))
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    Text("·")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    Text(session.networkEnabled ? "network on" : "network off")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            HStack(spacing: 4) {
                IconButton(symbol: "plus.viewfinder", help: "Show window") {
                    // Future: focus the VM window. Spec leaves stub OK.
                }
                IconButton(symbol: "square.and.arrow.up", help: "Export") {
                    // Future: export session output dir.
                }
                IconButton(symbol: "xmark.circle", help: "Discard", isDanger: true, action: onDiscard)
            }

            SessionStatePill(status: session.status)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .overlay(Divider(), alignment: .bottom)
    }

    private func fileSymbol(for path: String) -> String {
        let ext = (path as NSString).pathExtension.lowercased()
        switch ext {
        case "zip", "tar", "gz", "rar", "7z": return "archivebox"
        case "dmg", "iso":                    return "shippingbox"
        case "doc", "docx", "pdf", "txt":     return "doc.text"
        default:                              return "doc"
        }
    }
}

private struct IconButton: View {
    let symbol: String
    let help: LocalizedStringKey
    var isDanger: Bool = false
    let action: () -> Void

    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .regular))
                .foregroundColor(isDanger ? Theme.verdictRedFg : .primary)
                .frame(width: 28, height: 28)
                .background(hovered && isDanger ? Theme.discardHoverBg : Theme.panelBg)
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .strokeBorder(
                            hovered && isDanger ? Theme.discardHoverBorder : Theme.separator,
                            lineWidth: 1
                        )
                )
                .clipShape(RoundedRectangle(cornerRadius: 6))
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .help(help)
    }
}

private func ageString(from iso: String) -> String {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let date = formatter.date(from: iso) ?? ISO8601DateFormatter().date(from: iso)
    guard let d = date else { return iso }
    let delta = max(0, Int(Date().timeIntervalSince(d)))
    if delta < 60 { return "\(delta)s" }
    if delta < 3600 { return "\(delta / 60)m" }
    if delta < 86_400 { return "\(delta / 3600)h" }
    return "\(delta / 86_400)d"
}
