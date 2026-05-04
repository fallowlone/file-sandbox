import SwiftUI
import AppKit

struct AppHeader: View {
    @ObservedObject var store: JobStore

    var body: some View {
        HStack(spacing: 8) {
            Text("FileSandbox")
                .font(.system(size: 13, weight: .semibold))

            Spacer()

            StatusChip(
                mode: store.mode,
                isConnected: store.isConnected,
                onSelect: { store.setMode($0) }
            )

            Menu {
                Button("Refresh") { store.fetch() }
                if store.isConnected {
                    Button("Restart daemon") { restartDaemon() }
                    Button("View logs") { openLogs() }
                }
                Divider()
                if !store.jobs.isEmpty {
                    Button("Clear settled jobs") { store.clearJobs() }
                }
                Button("Quit FileSandbox") { NSApp.terminate(nil) }
            } label: {
                Image(systemName: "ellipsis")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundColor(.secondary)
                    .frame(width: 22, height: 22)
                    .contentShape(Rectangle())
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .overlay(Divider(), alignment: .bottom)
    }

    private func restartDaemon() {
        store.stopDaemon()
    }

    private func openLogs() {
        let url = URL(fileURLWithPath: NSString("~/Library/Logs/FileSandbox/daemon.log").expandingTildeInPath)
        if FileManager.default.fileExists(atPath: url.path) {
            NSWorkspace.shared.open(url)
        } else {
            NSWorkspace.shared.open(url.deletingLastPathComponent())
        }
    }
}
