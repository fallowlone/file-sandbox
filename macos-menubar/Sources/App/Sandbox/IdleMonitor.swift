import Foundation

public protocol Clock {
    func currentDate() -> Date
}

public struct SystemClock: Clock {
    public init() {}
    public func currentDate() -> Date { Date() }
}

public final class IdleMonitor {
    private let idleTimeout: TimeInterval
    private let hardCap: TimeInterval
    private let clock: Clock
    private let onSoftWarning: () -> Void
    private let onTimeout: () -> Void

    private var startedAt: Date?
    private var lastActivityAt: Date?
    private var softFired = false
    private var hardFired = false

    public init(
        idleTimeoutMinutes: Int,
        hardCapMinutes: Int,
        clock: Clock = SystemClock(),
        onSoftWarning: @escaping () -> Void,
        onTimeout: @escaping () -> Void
    ) {
        self.idleTimeout = TimeInterval(idleTimeoutMinutes) * 60
        self.hardCap = TimeInterval(hardCapMinutes) * 60
        self.clock = clock
        self.onSoftWarning = onSoftWarning
        self.onTimeout = onTimeout
    }

    public func start() {
        startedAt = clock.currentDate()
        lastActivityAt = startedAt
    }

    public func recordActivity() {
        lastActivityAt = clock.currentDate()
        softFired = false
    }

    public func tick() {
        guard !hardFired, let started = startedAt, let active = lastActivityAt else { return }
        let now = clock.currentDate()
        if now.timeIntervalSince(started) >= hardCap {
            hardFired = true; onTimeout(); return
        }
        let idle = now.timeIntervalSince(active)
        if !softFired, idle > max(0, idleTimeout - 5 * 60) {
            softFired = true; onSoftWarning()
        }
        if idle >= idleTimeout {
            hardFired = true; onTimeout()
        }
    }
}
