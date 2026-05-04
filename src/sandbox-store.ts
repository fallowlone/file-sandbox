import Database from "better-sqlite3";

export type SandboxSessionStatus =
  | "starting"
  | "running"
  | "stopped"
  | "failed"
  | "discarded";

export interface SandboxSession {
  id: string;
  vmName: string;
  sourceJobId: string | null;
  sourceFilePath: string;
  sessionDir: string;
  pid: number | null;
  networkEnabled: boolean;
  status: SandboxSessionStatus;
  detail: string | null;
  createdAt: string;
  lastActiveAt: string;
  exitedAt: string | null;
}

export interface InsertInput {
  id: string;
  vmName: string;
  sourceJobId: string | null;
  sourceFilePath: string;
  sessionDir: string;
  networkEnabled: boolean;
}

interface Row {
  id: string;
  vm_name: string;
  source_job_id: string | null;
  source_file_path: string;
  session_dir: string;
  pid: number | null;
  network_enabled: number;
  status: SandboxSessionStatus;
  detail: string | null;
  created_at: string;
  last_active_at: string;
  exited_at: string | null;
}

function toSession(r: Row): SandboxSession {
  return {
    id: r.id,
    vmName: r.vm_name,
    sourceJobId: r.source_job_id,
    sourceFilePath: r.source_file_path,
    sessionDir: r.session_dir,
    pid: r.pid,
    networkEnabled: r.network_enabled === 1,
    status: r.status,
    detail: r.detail,
    createdAt: r.created_at,
    lastActiveAt: r.last_active_at,
    exitedAt: r.exited_at,
  };
}

export class SandboxStore {
  private readonly db: Database.Database;

  constructor(path: string) {
    this.db = new Database(path);
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS sandbox_sessions (
        id                TEXT    PRIMARY KEY,
        vm_name           TEXT    NOT NULL UNIQUE,
        source_job_id     TEXT,
        source_file_path  TEXT    NOT NULL,
        session_dir       TEXT    NOT NULL,
        pid               INTEGER,
        network_enabled   INTEGER NOT NULL DEFAULT 0,
        status            TEXT    NOT NULL,
        detail            TEXT,
        created_at        TEXT    NOT NULL,
        last_active_at    TEXT    NOT NULL,
        exited_at         TEXT
      );
      CREATE INDEX IF NOT EXISTS idx_sandbox_status ON sandbox_sessions(status);
      CREATE INDEX IF NOT EXISTS idx_sandbox_created ON sandbox_sessions(created_at);
    `);
  }

  insert(i: InsertInput): void {
    const now = new Date().toISOString();
    this.db
      .prepare(
        `INSERT INTO sandbox_sessions
         (id, vm_name, source_job_id, source_file_path, session_dir, pid, network_enabled, status, detail, created_at, last_active_at, exited_at)
         VALUES (?, ?, ?, ?, ?, NULL, ?, 'starting', NULL, ?, ?, NULL)`,
      )
      .run(
        i.id,
        i.vmName,
        i.sourceJobId,
        i.sourceFilePath,
        i.sessionDir,
        i.networkEnabled ? 1 : 0,
        now,
        now,
      );
  }

  setRunning(id: string, pid: number): void {
    const now = new Date().toISOString();
    this.db
      .prepare(
        `UPDATE sandbox_sessions SET pid = ?, status = 'running', last_active_at = ? WHERE id = ? AND status IN ('starting','running')`,
      )
      .run(pid, now, id);
  }

  setStopped(id: string, detail?: string): void {
    const now = new Date().toISOString();
    this.db
      .prepare(
        `UPDATE sandbox_sessions SET status = 'stopped', detail = COALESCE(?, detail), exited_at = ?, last_active_at = ? WHERE id = ? AND status NOT IN ('discarded')`,
      )
      .run(detail ?? null, now, now, id);
  }

  setFailed(id: string, detail: string): void {
    const now = new Date().toISOString();
    this.db
      .prepare(
        `UPDATE sandbox_sessions SET status = 'failed', detail = ?, exited_at = ?, last_active_at = ? WHERE id = ?`,
      )
      .run(detail, now, now, id);
  }

  markDiscarded(id: string, detail?: string): void {
    const now = new Date().toISOString();
    this.db
      .prepare(
        `UPDATE sandbox_sessions SET status = 'discarded', detail = COALESCE(?, detail), exited_at = COALESCE(exited_at, ?), last_active_at = ? WHERE id = ?`,
      )
      .run(detail ?? null, now, now, id);
  }

  touch(id: string): void {
    const now = new Date().toISOString();
    this.db
      .prepare(`UPDATE sandbox_sessions SET last_active_at = ? WHERE id = ?`)
      .run(now, id);
  }

  get(id: string): SandboxSession | null {
    const r = this.db
      .prepare(`SELECT * FROM sandbox_sessions WHERE id = ?`)
      .get(id) as Row | undefined;
    return r ? toSession(r) : null;
  }

  listActive(): SandboxSession[] {
    const rows = this.db
      .prepare(
        `SELECT * FROM sandbox_sessions WHERE status IN ('starting','running') ORDER BY created_at DESC`,
      )
      .all() as Row[];
    return rows.map(toSession);
  }

  listSessions(opts?: { limit?: number }): SandboxSession[] {
    const limit = Math.max(1, Math.min(opts?.limit ?? 50, 500));
    const rows = this.db
      .prepare(
        `SELECT * FROM sandbox_sessions ORDER BY created_at DESC LIMIT ?`,
      )
      .all(limit) as Row[];
    return rows.map(toSession);
  }

  listIdle(maxLastActiveISO: string): SandboxSession[] {
    const rows = this.db
      .prepare(
        `SELECT * FROM sandbox_sessions WHERE status IN ('starting','running') AND last_active_at < ?`,
      )
      .all(maxLastActiveISO) as Row[];
    return rows.map(toSession);
  }
}
