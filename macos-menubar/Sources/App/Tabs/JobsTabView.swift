import SwiftUI
import AppKit

struct JobsTabView: View {
    @ObservedObject var store: JobStore
    @ObservedObject var settingsStore: SettingsStore

    @AppStorage("filesandbox.jobs.collapsed.scanning")  private var scanningCollapsed = false
    @AppStorage("filesandbox.jobs.collapsed.quarantine") private var quarantineCollapsed = false
    @AppStorage("filesandbox.jobs.collapsed.restored")   private var restoredCollapsed = true

    @State private var expandedJobId: String? = nil

    var body: some View {
        if store.jobs.isEmpty && store.isConnected {
            emptyPlaceholder
        } else if !store.isConnected {
            offlineMessage
        } else {
            ScrollView {
                LazyVStack(spacing: 0) {
                    JobsGroup(
                        title: "Scanning",
                        jobs: store.scanningJobs,
                        emptyMessage: "No active scans",
                        collapsed: $scanningCollapsed,
                        expandedJobId: $expandedJobId,
                        store: store
                    )
                    JobsGroup(
                        title: "Quarantine",
                        jobs: store.quarantinedJobs,
                        emptyMessage: "Nothing quarantined",
                        collapsed: $quarantineCollapsed,
                        expandedJobId: $expandedJobId,
                        store: store
                    )
                    if !store.restoredJobs.isEmpty {
                        JobsGroup(
                            title: "Restored",
                            jobs: store.restoredJobs,
                            emptyMessage: "",
                            collapsed: $restoredCollapsed,
                            expandedJobId: $expandedJobId,
                            store: store
                        )
                    }
                }
            }
            .frame(maxHeight: 520)
        }
    }

    private var emptyPlaceholder: some View {
        VStack(spacing: 6) {
            Image(systemName: "tray.and.arrow.down")
                .font(.system(size: 26))
                .foregroundColor(.secondary)
            Text("Watching \(settingsStore.watchPath.isEmpty ? "-" : settingsStore.watchPath)")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            Text("Drop files here or run a test scan")
                .font(.system(size: 10))
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 28)
    }

    private var offlineMessage: some View {
        VStack(spacing: 8) {
            Image(systemName: "wifi.exclamationmark")
                .font(.system(size: 22))
                .foregroundColor(.secondary)
            Text("Daemon offline")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
            if !settingsStore.daemonProjectPath.isEmpty {
                Button("Start daemon") {
                    store.startDaemon(
                        projectPath: settingsStore.daemonProjectPath,
                        nodeBin: settingsStore.daemonNodeBin
                    )
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 28)
    }
}

private struct JobsGroup: View {
    let title: String
    let jobs: [SandboxJob]
    let emptyMessage: String
    @Binding var collapsed: Bool
    @Binding var expandedJobId: String?
    @ObservedObject var store: JobStore

    var body: some View {
        Button {
            collapsed.toggle()
        } label: {
            HStack(spacing: 6) {
                Image(systemName: collapsed ? "chevron.right" : "chevron.down")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundColor(.secondary)
                Text(title.uppercased())
                    .font(.system(size: 10, weight: .semibold))
                    .tracking(0.5)
                    .foregroundColor(.secondary)
                Text("\(jobs.count)")
                    .font(.system(size: 10, weight: .regular))
                    .foregroundColor(Color.secondary.opacity(0.7))
                Spacer()
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 7)
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
            .background(Theme.subtleBg)
        }
        .buttonStyle(.plain)
        .overlay(Divider(), alignment: .bottom)

        if !collapsed {
            if jobs.isEmpty {
                if !emptyMessage.isEmpty {
                    Text(emptyMessage)
                        .font(.system(size: 10))
                        .foregroundColor(.secondary)
                        .padding(.leading, 42)
                        .padding(.vertical, 9)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .overlay(Divider(), alignment: .bottom)
                }
            } else {
                ForEach(jobs) { job in
                    JobRow(
                        job: job,
                        expandedJobId: $expandedJobId,
                        store: store
                    )
                }
            }
        }
    }
}

private struct JobRow: View {
    let job: SandboxJob
    @Binding var expandedJobId: String?
    @ObservedObject var store: JobStore

    private var isExpanded: Bool { expandedJobId == job.id }

    var body: some View {
        VStack(spacing: 0) {
            collapsedRow
            if isExpanded {
                expandedDetail
            }
        }
        .overlay(Divider(), alignment: .bottom)
    }

    private var collapsedRow: some View {
        Button {
            expandedJobId = isExpanded ? nil : job.id
        } label: {
            HStack(spacing: 8) {
                Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundColor(.secondary)
                    .frame(width: 14)
                Image(systemName: fileSymbol(for: job.original_name))
                    .font(.system(size: 13))
                    .foregroundColor(.secondary)
                Text(job.original_name)
                    .font(.system(size: 12))
                    .foregroundColor(.primary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                if let stage = job.stageEnum, stage != .done, stage != .error {
                    StagePill(stage: stage)
                } else if let pill = VerdictPill.forJobVerdict(
                    vt: job.vt_verdict,
                    pompelmi: job.pompelmi_verdict,
                    status: job.status
                ) {
                    pill
                }
                Text(ageString(from: job.created_at))
                    .font(.system(size: 11))
                    .foregroundColor(.secondary)
                    .frame(minWidth: 28, alignment: .trailing)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private var expandedDetail: some View {
        VStack(alignment: .leading, spacing: 10) {
            if let big = bigVerdictPill() {
                HStack(spacing: 8) {
                    big
                    if let detail = job.detail, !detail.isEmpty {
                        Text(detail)
                            .font(.system(size: 11))
                            .foregroundColor(.secondary)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
            }

            StageRow(job: job)

            HStack(spacing: 6) {
                MetaPill(
                    symbol: "doc",
                    text: URL(fileURLWithPath: job.final_path ?? job.original_name).lastPathComponent,
                    tooltip: job.final_path
                )
                MetaPill(symbol: "clock", text: ageString(from: job.created_at))
            }

            HStack(spacing: 6) {
                if job.status == "quarantine_kept" {
                    Button {
                        store.restoreFile(job.id)
                    } label: {
                        Label("Restore", systemImage: "arrow.uturn.backward")
                            .font(.system(size: 11))
                            .padding(.horizontal, 10)
                            .padding(.vertical, 5)
                            .overlay(
                                RoundedRectangle(cornerRadius: Theme.cornerRadiusButton)
                                    .strokeBorder(Theme.separator, lineWidth: 1)
                            )
                    }
                    .buttonStyle(.plain)

                    Button {
                        store.deleteFile(job.id)
                    } label: {
                        Label("Delete", systemImage: "trash")
                            .font(.system(size: 11))
                            .foregroundColor(Theme.verdictRedFg)
                            .padding(.horizontal, 10)
                            .padding(.vertical, 5)
                            .overlay(
                                RoundedRectangle(cornerRadius: Theme.cornerRadiusButton)
                                    .strokeBorder(Theme.verdictRedFg.opacity(0.4), lineWidth: 1)
                            )
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .padding(.leading, 28)
        .background(Theme.subtleBg)
    }

    private func bigVerdictPill() -> VerdictPill? {
        let v = (job.vt_verdict ?? "").lowercased()
        switch v {
        case "infected", "malicious":
            return VerdictPill(text: L.verdictBig(v), variant: .red, size: .big, symbol: "exclamationmark.triangle.fill")
        case "inconclusive":
            return VerdictPill(text: L.verdictBig("inconclusive"), variant: .orange, size: .big, symbol: "questionmark.circle.fill")
        case "oversized":
            return VerdictPill(text: L.verdictBig("oversized"), variant: .grey, size: .big, symbol: "arrow.down.circle")
        case "clean":
            return VerdictPill(text: L.verdictBig("clean"), variant: .green, size: .big, symbol: "checkmark.circle.fill")
        default: return nil
        }
    }

    private func fileSymbol(for name: String) -> String {
        let ext = (name as NSString).pathExtension.lowercased()
        switch ext {
        case "zip", "tar", "gz", "rar", "7z": return "archivebox"
        case "dmg", "iso":                    return "shippingbox"
        case "doc", "docx", "pdf", "txt":     return "doc.text"
        default:                              return "doc"
        }
    }
}

private func ageString(from epoch: Int) -> String {
    let now = Int(Date().timeIntervalSince1970)
    let delta = max(0, now - epoch)
    if delta < 60 { return "\(delta)s" }
    if delta < 3600 { return "\(delta / 60)m" }
    if delta < 86_400 { return "\(delta / 3600)h" }
    return "\(delta / 86_400)d"
}
