import SwiftUI
import AppKit

struct AppFooter: View {
    @ObservedObject var store: JobStore

    var body: some View {
        VStack(spacing: 4) {
            if let error = store.lastActionError {
                Text(error)
                    .font(.caption)
                    .foregroundColor(.red)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 14)
                    .transition(.opacity)
            }
            HStack(spacing: 0) {
                if store.isConnected {
                    LinkText(text: "Restart daemon") { store.stopDaemon() }
                    Text(" · ")
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                    LinkText(text: "View logs") { openLogs() }
                }
                Spacer()
                Button("Quit") {
                    NSApp.terminate(nil)
                }
                .buttonStyle(.plain)
                .font(.system(size: 11))
                .foregroundColor(Theme.verdictRedFg)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 7)
        }
        .background(Theme.subtleBg)
        .overlay(Divider(), alignment: .top)
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

private struct LinkText: View {
    let text: String
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            Text(text)
                .font(.system(size: 10))
                .foregroundColor(.secondary)
                .underline(hovered, color: .secondary)
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
    }
}
