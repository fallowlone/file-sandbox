import Database from "better-sqlite3";
import { mkdirSync } from "fs";
import { dirname } from "path";
import type { VirusCheckResult, VirusVerdict } from "./virus-checker.ts";

export class JobConflictError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "JobConflictError";
  }
}

export type JobStatus =
  | "received"
  | "in_quarantine"
  | "scanning"
  | "restored"
  | "quarantine_kept"
  | "deleted"
  | "cancelled"
  | "failed";

export type ScanStage =
  | "received"
  | "cache_check"
  | "local_scan"
  | "vt_upload"
  | "vt_poll"
  | "done"
  | "error";

export type JobRow = {
  id: string;
  source_path: string;
  original_name: string;
  quarantine_path: string | null;
  final_path: string | null;
  status: JobStatus;
  vt_verdict: VirusVerdict | null;
  pompelmi_verdict: "clean" | "malicious" | "error" | null;
  scan_stage: ScanStage | null;
  detail: string | null;
  created_at: number;
  updated_at: number;
};

export class JobStore {
  private readonly db: Database.Database;

  constructor(dbPath: string) {
    mkdirSync(dirname(dbPath), { recursive: true });
    this.db = new Database(dbPath);
    this.db.pragma("journal_mode = WAL");
    this.db
      .prepare(
        `CREATE TABLE IF NOT EXISTS jobs (
        id TEXT PRIMARY KEY NOT NULL,
        source_path TEXT NOT NULL,
        original_name TEXT NOT NULL,
        quarantine_path TEXT,
        final_path TEXT,
        status TEXT NOT NULL,
        vt_verdict TEXT,
        detail TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      )`,
      )
      .run();
    this.db
      .prepare(
        `CREATE INDEX IF NOT EXISTS idx_jobs_created ON jobs (created_at DESC)`,
      )
      .run();

    // Idempotent column add for pompelmi_verdict
    try {
      this.db.exec("ALTER TABLE jobs ADD COLUMN pompelmi_verdict TEXT");
    } catch (e) {
      // Already exists - sqlite throws "duplicate column name"
      if (!String(e).includes("duplicate column name")) throw e;
    }

    // Idempotent column add for scan_stage
    try {
      this.db.exec("ALTER TABLE jobs ADD COLUMN scan_stage TEXT");
    } catch (e) {
      if (!String(e).includes("duplicate column name")) throw e;
    }
  }

  insertReceived(jobId: string, sourcePath: string, originalName: string) {
    const now = Date.now();
    this.db
      .prepare(
        `INSERT INTO jobs (id, source_path, original_name, quarantine_path, final_path, status, vt_verdict, pompelmi_verdict, detail, created_at, updated_at)
         VALUES (?, ?, ?, NULL, NULL, 'received', NULL, NULL, NULL, ?, ?)`,
      )
      .run(jobId, sourcePath, originalName, now, now);
  }

  setInQuarantine(jobId: string, quarantinePath: string) {
    const now = Date.now();
    this.db
      .prepare(
        `UPDATE jobs SET quarantine_path = ?, status = 'in_quarantine', updated_at = ? WHERE id = ?`,
      )
      .run(quarantinePath, now, jobId);
  }

  setScanning(jobId: string) {
    const now = Date.now();
    this.db
      .prepare(
        `UPDATE jobs SET status = 'scanning', updated_at = ? WHERE id = ?`,
      )
      .run(now, jobId);
  }

  setScanResult(jobId: string, result: VirusCheckResult) {
    const now = Date.now();
    if (result.verdict === "clean") {
      this.db
        .prepare(
          `UPDATE jobs SET vt_verdict = ?, detail = ?, updated_at = ? WHERE id = ?`,
        )
        .run(result.verdict, result.message, now, jobId);
    } else {
      this.db
        .prepare(
          `UPDATE jobs SET vt_verdict = ?, detail = ?, status = 'quarantine_kept', updated_at = ? WHERE id = ?`,
        )
        .run(result.verdict, result.message, now, jobId);
    }
  }

  setPompelmiVerdict(
    jobId: string,
    verdict: "clean" | "malicious" | "error",
    detail?: string,
  ): void {
    const now = Date.now();
    this.db
      .prepare(
        `UPDATE jobs SET pompelmi_verdict = ?, detail = COALESCE(?, detail), updated_at = ? WHERE id = ?`,
      )
      .run(verdict, detail ?? null, now, jobId);
  }

  setStage(jobId: string, stage: ScanStage): void {
    const now = Date.now();
    this.db
      .prepare(
        `UPDATE jobs SET scan_stage = ?, updated_at = ? WHERE id = ?`,
      )
      .run(stage, now, jobId);
  }

  setRestored(jobId: string, finalPath: string) {
    const now = Date.now();
    this.db
      .prepare(
        `UPDATE jobs SET final_path = ?, status = 'restored', updated_at = ? WHERE id = ?`,
      )
      .run(finalPath, now, jobId);
  }

  fail(jobId: string, message: string) {
    const now = Date.now();
    this.db
      .prepare(
        `UPDATE jobs SET status = 'failed', detail = ?, updated_at = ? WHERE id = ?`,
      )
      .run(message, now, jobId);
  }

  listRecent(limit = 100): JobRow[] {
    const rows = this.db
      .prepare(
        `SELECT id, source_path, original_name, quarantine_path, final_path, status, vt_verdict, pompelmi_verdict, scan_stage, detail, created_at, updated_at
         FROM jobs ORDER BY created_at DESC LIMIT ?`,
      )
      .all(limit) as JobRow[];
    return rows;
  }

  getJob(jobId: string): JobRow | undefined {
    return this.db
      .prepare(
        `SELECT id, source_path, original_name, quarantine_path, final_path, status, vt_verdict, pompelmi_verdict, scan_stage, detail, created_at, updated_at
         FROM jobs WHERE id = ?`,
      )
      .get(jobId) as JobRow | undefined;
  }

  setDeleted(jobId: string, detail = "Deleted by user") {
    const now = Date.now();
    const result = this.db
      .prepare(
        `UPDATE jobs SET status = 'deleted', detail = ?, updated_at = ? WHERE id = ? AND status = 'quarantine_kept'`,
      )
      .run(detail, now, jobId);
    if (result.changes === 0) {
      throw new JobConflictError(
        `Job ${jobId} cannot be deleted: not in quarantine_kept status (may have been deleted or processed)`,
      );
    }
  }

  /** Inconclusive rows kept in quarantine with `created_at` before cutoff (ms epoch). */
  listInconclusiveOlderThan(cutoffMs: number): JobRow[] {
    return this.db
      .prepare(
        `SELECT id, source_path, original_name, quarantine_path, final_path, status, vt_verdict, pompelmi_verdict, scan_stage, detail, created_at, updated_at
         FROM jobs
         WHERE status = 'quarantine_kept' AND vt_verdict = 'inconclusive' AND created_at < ?`,
      )
      .all(cutoffMs) as JobRow[];
  }

  cancelJob(jobId: string) {
    const now = Date.now();
    this.db
      .prepare(
        `UPDATE jobs SET status = 'cancelled', detail = 'Cancelled by user', updated_at = ? WHERE id = ? AND status = 'scanning'`,
      )
      .run(now, jobId);
  }

  /**
   * Delete settled job rows. Rows with active status
   * (`received`, `in_quarantine`, `scanning`) are kept so in-flight
   * pipeline stages (move → scan → restore) keep their row reference.
   */
  clearAll(): { deleted: number; skipped: number } {
    const active: JobStatus[] = ["received", "in_quarantine", "scanning"];
    const placeholders = active.map(() => "?").join(",");
    const skippedRow = this.db
      .prepare(
        `SELECT COUNT(*) AS n FROM jobs WHERE status IN (${placeholders})`,
      )
      .get(...active) as { n: number };
    const res = this.db
      .prepare(`DELETE FROM jobs WHERE status NOT IN (${placeholders})`)
      .run(...active);
    return { deleted: res.changes, skipped: skippedRow.n };
  }

  close() {
    this.db.close();
  }
}
