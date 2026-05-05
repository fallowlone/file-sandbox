import XCTest
@testable import FileSandboxMenuBar

final class IdleMonitorTests: XCTestCase {
    final class FakeClock: Clock {
        var now: Date = Date(timeIntervalSince1970: 0)
        func currentDate() -> Date { now }
    }

    func testTimeoutFiresAfterIdle() {
        let clock = FakeClock()
        var firedSoft = false, firedHard = false
        let m = IdleMonitor(
            idleTimeoutMinutes: 30, hardCapMinutes: 240, clock: clock,
            onSoftWarning: { firedSoft = true },
            onTimeout: { firedHard = true }
        )
        m.start()
        clock.now = clock.now.addingTimeInterval(25 * 60)
        m.tick()
        XCTAssertFalse(firedSoft); XCTAssertFalse(firedHard)
        clock.now = clock.now.addingTimeInterval(60) // 26 min
        m.tick()
        XCTAssertTrue(firedSoft, "soft warning at T-5 = 25 min in")
        clock.now = clock.now.addingTimeInterval(5 * 60) // 31 min
        m.tick()
        XCTAssertTrue(firedHard)
    }

    func testActivityResetsTimer() {
        let clock = FakeClock()
        var firedHard = false
        let m = IdleMonitor(
            idleTimeoutMinutes: 30, hardCapMinutes: 240, clock: clock,
            onSoftWarning: {}, onTimeout: { firedHard = true })
        m.start()
        clock.now = clock.now.addingTimeInterval(20 * 60)
        m.recordActivity()
        clock.now = clock.now.addingTimeInterval(20 * 60) // 40 min total, but only 20 since reset
        m.tick()
        XCTAssertFalse(firedHard)
    }

    func testHardCapFiresEvenWithActivity() {
        let clock = FakeClock()
        var firedHard = false
        let m = IdleMonitor(
            idleTimeoutMinutes: 30, hardCapMinutes: 60, clock: clock,
            onSoftWarning: {}, onTimeout: { firedHard = true })
        m.start()
        for _ in 0..<10 {
            clock.now = clock.now.addingTimeInterval(7 * 60)
            m.recordActivity()
            m.tick()
        }
        XCTAssertTrue(firedHard, "hard cap should fire at >60min regardless of activity")
    }
}
