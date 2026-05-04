import SwiftUI
import AppKit

struct SandboxView: View {
    @ObservedObject var store: SandboxStore
    @State private var showImporter = false
    @State private var pendingNetwork = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Sandbox").font(.headline)
                Spacer()
                Toggle("Network", isOn: $pendingNetwork).toggleStyle(.switch)
                Button {
                    pickFileAndOpen()
                } label: {
                    Label("New", systemImage: "plus")
                }
            }
            if let err = store.loadError {
                Text(err).font(.caption).foregroundColor(.red)
            }
            ForEach(store.sessions) { s in
                SandboxRow(session: s, onDiscard: { store.discard(s.id) })
            }
        }
        .padding(8)
        .onAppear { store.fetch() }
    }

    private func pickFileAndOpen() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url {
            store.create(filePath: url.path, sourceJobId: nil, network: pendingNetwork) { _ in }
        }
    }
}

struct SandboxRow: View {
    let session: SandboxSession
    let onDiscard: () -> Void
    var body: some View {
        HStack {
            Image(systemName: session.networkEnabled ? "network" : "network.slash")
                .foregroundColor(session.networkEnabled ? .yellow : .secondary)
            VStack(alignment: .leading) {
                Text((session.sourceFilePath as NSString).lastPathComponent)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text("\(session.vmName) · \(session.status)")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            Spacer()
            Button("Discard", action: onDiscard)
                .buttonStyle(.borderless)
        }
    }
}
