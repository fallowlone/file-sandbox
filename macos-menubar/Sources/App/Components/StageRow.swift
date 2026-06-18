import SwiftUI

/// Always-five-badges row that renders the cache → local → vt-upload → vt-poll
/// → done pipeline. Each badge's state is computed from the job's
/// `stageEnum`, `pompelmi_verdict`, and `vt_verdict`.
struct StageRow: View {
    let job: SandboxJob

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(ScanStage.pipeline, id: \.self) { stage in
                    StageBadge(stage: stage, state: state(for: stage))
                }
            }
        }
    }

    // MARK: - State computation

    private func state(for badge: ScanStage) -> StageBadge.State {
        let current = job.stageEnum
        guard let order = ScanStage.pipeline.firstIndex(of: badge) else {
            return .pending  // unreachable
        }
        let currentOrder = current.flatMap { ScanStage.pipeline.firstIndex(of: $0) }

        // Error replaces the current stage badge with red.
        if current == .error, currentOrder == order {
            return .error(detail: job.detail)
        }

        // Terminal job — every badge is either `done` or `skipped`.
        if isTerminal(job) {
            return wasExecuted(badge) ? .done(verdictText: verdictText(for: badge)) : .skipped
        }

        // Mid-flight: stages strictly before current are done, equal is current,
        // strictly after are pending.
        guard let cur = currentOrder else { return .pending }
        if order < cur { return .done(verdictText: verdictText(for: badge)) }
        if order == cur { return .current(stage: badge) }
        return .pending
    }

    private func isTerminal(_ job: SandboxJob) -> Bool {
        if job.stageEnum == .done { return true }
        if job.status == "quarantine_kept" || job.status == "restored" { return true }
        return false
    }

    private func wasExecuted(_ badge: ScanStage) -> Bool {
        switch badge {
        case .cacheCheck:
            return true  // every job touches the cache
        case .localScan:
            return job.pompelmi_verdict != nil
        case .vtUpload:
            // oversized counts as "executed" so the user sees that VT was
            // considered but bailed.
            return job.vt_verdict != nil
        case .vtPoll:
            // oversized = upload phase decided to bail, no poll happened.
            let v = (job.vt_verdict ?? "").lowercased()
            return job.vt_verdict != nil && v != "oversized"
        case .done:
            return job.status == "quarantine_kept" || job.status == "restored" || job.vt_verdict != nil
        default:
            return false
        }
    }

    private func verdictText(for badge: ScanStage) -> String? {
        switch badge {
        case .cacheCheck:
            // Heuristic: if either engine produced a verdict, this was a cache miss.
            return (job.pompelmi_verdict != nil || job.vt_verdict != nil) ? "miss" : "hit"
        case .localScan:
            return job.pompelmi_verdict
        case .vtUpload:
            let v = (job.vt_verdict ?? "").lowercased()
            return v == "oversized" ? "oversized" : nil
        case .vtPoll:
            return job.vt_verdict
        case .done:
            return job.vt_verdict ?? job.pompelmi_verdict
        default:
            return nil
        }
    }
}
