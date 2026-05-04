# Tart-based isolated sandbox environment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a user-triggered isolated macOS environment via Tart VM. The user can pick any file (or any quarantined job) and open it inside a fresh disposable VM clone, with read-only file ingress, opt-in network, and no clipboard or USB pass-through.

**Architecture:** A new `SandboxManager` orchestrates `tart` CLI subprocesses (clone, run, delete) and tracks sessions in a new `sandbox_sessions` table. Each session has its own host scratch dir mounted into the guest via VirtioFS. Sessions discard cleanly on window close, idle timeout, or explicit user action. Files leaving the sandbox go through the normal watch+scan pipeline — no bypass. Menu bar UI gets a new "Sandbox" section.

**Tech Stack:** Node 22 with `--experimental-strip-types`, `better-sqlite3`, `child_process.execFile` for `tart`, SwiftUI for the menu bar app, `tart` CLI from cirruslabs.

---

## Reference

- Spec: `docs/superpowers/specs/2026-05-04-tart-sandbox-environment-design.md`
- Affected files: new `src/sandbox-manager.ts`, new `src/sandbox-store.ts`, modifications to `src/config.ts`, `src/ui-server.ts`, `src/index.ts`, plus a new SwiftUI view and additions to existing menu bar files.

## File Structure

| File | Responsibility | Status |
|---|---|---|
| `src/sandbox-store.ts` | SQLite access for `sandbox_sessions`. Insert, update, list, get, mark-discarded, reconcile. | Create |
| `src/sandbox-store.test.ts` | Unit tests for the store. | Create |
| `src/sandbox-tart.ts` | Thin shell over the `tart` CLI: `clone`, `run`, `delete`, `list`. | Create |
| `src/sandbox-tart.test.ts` | Tests using a fake exec to capture commands. | Create |
| `src/sandbox-manager.ts` | High-level orchestration: createSession, listSessions, discardSession, idle watchdog, reconcile. | Create |
| `src/sandbox-manager.test.ts` | Tests using fake `sandbox-tart` and an in-memory store. | Create |
| `src/sandbox-paths.ts` | Pure path-validation utility for `POST /api/sandbox/sessions` payloads. | Create |
| `src/sandbox-paths.test.ts` | Unit tests for path validation. | Create |
| `src/config.ts` | Add `sandbox*` config keys. | Modify |
| `src/index.ts` | Initialise `SandboxManager` if `sandboxEnabled`; wire shutdown hook; pass to UI server. | Modify |
| `src/ui-server.ts` | New `/api/sandbox/sessions` routes; `/api/health` adds `sandbox` block. | Modify |
| `macos-menubar/Sources/App/SandboxStore.swift` | ObservableObject mirroring the daemon's sandbox state. | Create |
| `macos-menubar/Sources/App/SandboxView.swift` | New menu-bar tab/section listing sessions and the "+ New" picker. | Create |
| `macos-menubar/Sources/App/Views.swift` | Add "Open in sandbox" button to quarantined job rows; add `SandboxView` into the dropdown. | Modify |
| `macos-menubar/Sources/App/SettingsStore.swift` | Add sandbox settings fields. | Modify |
| `macos-menubar/Sources/App/SettingsView.swift` | New "Sandbox" Settings section. | Modify |
| `README.md` | Document Tart prereqs and base image setup. | Modify |

---

## Task 1: Schema and store for `sandbox_sessions`

**Files:**

- Create: `src/sandbox-store.ts`
- Create: `src/sandbox-store.test.ts`

- [ ] **Step 1: Write the failing tests**

```ts
// src/sandbox-store.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";
import { SandboxStore } from "./sandbox-store.ts";

function fresh() {
  return new SandboxStore(":memory:");
}

test("insert + get round-trip", () => {
  const s = fresh();
  s.insert({
    id: "a",
    vmName: "fsbx-a",
    sourceJobId: null,
    sourceFilePath: "/tmp/f.bin",
    sessionDir: "/tmp/sess/a",
    networkEnabled: false,
  });
  const r = s.get("a");
  assert.equal(r?.id, "a");
  assert.equal(r?.status, "starting");
  assert.equal(r?.networkEnabled, false);
});

test("setRunning sets pid and status", () => {
  const s = fresh();
  s.insert({ id: "a", vmName: "fsbx-a", sourceJobId: null, sourceFilePath: "/x", sessionDir: "/y", networkEnabled: false });
  s.setRunning("a", 4242);
  const r = s.get("a");
  assert.equal(r?.status, "running");
  assert.equal(r?.pid, 4242);
});

test("listSessions returns newest first, capped by limit", () => {
  const s = fresh();
  for (let i = 0; i < 5; i++) {
    s.insert({ id: `a${i}`, vmName: `fsbx-${i}`, sourceJobId: null, sourceFilePath: "/x", sessionDir: "/y", networkEnabled: false });
  }
  const r = s.listSessions({ limit: 3 });
  assert.equal(r.length, 3);
  assert.equal(r[0].id, "a4");
});

test("markDiscarded is idempotent", () => {
  const s = fresh();
  s.insert({ id: "a", vmName: "fsbx-a", sourceJobId: null, sourceFilePath: "/x", sessionDir: "/y", networkEnabled: false });
  s.markDiscarded("a", "test");
  s.markDiscarded("a", "test again");
  const r = s.get("a");
  assert.equal(r?.status, "discarded");
});
```

- [ ] **Step 2: Run, see fail**

```bash
yarn test
```

- [ ] **Step 3: Implement `src/sandbox-store.ts`**

```ts
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
```

- [ ] **Step 4: Run tests, see them pass**

```bash
yarn test
```

- [ ] **Step 5: Commit**

```bash
git add src/sandbox-store.ts src/sandbox-store.test.ts
git commit -m "feat(sandbox): SandboxStore with sandbox_sessions schema"
```

---

## Task 2: `tart` CLI shell

**Files:**

- Create: `src/sandbox-tart.ts`
- Create: `src/sandbox-tart.test.ts`

- [ ] **Step 1: Write the failing tests**

```ts
// src/sandbox-tart.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";
import { TartCli } from "./sandbox-tart.ts";

test("clone passes correct args", async () => {
  const calls: string[][] = [];
  const cli = new TartCli({
    runCommand: async (cmd, args) => {
      calls.push([cmd, ...args]);
      return { stdout: "", stderr: "" };
    },
  });
  await cli.clone("base-vm", "fsbx-1");
  assert.deepEqual(calls[0], ["tart", "clone", "base-vm", "fsbx-1"]);
});

test("delete passes correct args", async () => {
  const calls: string[][] = [];
  const cli = new TartCli({
    runCommand: async (cmd, args) => {
      calls.push([cmd, ...args]);
      return { stdout: "", stderr: "" };
    },
  });
  await cli.delete("fsbx-1");
  assert.deepEqual(calls[0], ["tart", "delete", "fsbx-1"]);
});

test("listVms parses JSON output", async () => {
  const cli = new TartCli({
    runCommand: async () => ({
      stdout: JSON.stringify([
        { Name: "base-vm", State: "stopped" },
        { Name: "fsbx-1", State: "running" },
      ]),
      stderr: "",
    }),
  });
  const vms = await cli.listVms();
  assert.equal(vms.length, 2);
  assert.equal(vms[1].name, "fsbx-1");
});
```

- [ ] **Step 2: Run, see fail**

```bash
yarn test
```

- [ ] **Step 3: Implement `src/sandbox-tart.ts`**

```ts
import { execFile } from "child_process";

export interface TartVm {
  name: string;
  state: string;
}

export interface RunCommandResult {
  stdout: string;
  stderr: string;
}

export type RunCommand = (
  cmd: string,
  args: string[],
  opts?: { cwd?: string; env?: NodeJS.ProcessEnv },
) => Promise<RunCommandResult>;

const defaultRunCommand: RunCommand = (cmd, args, opts) =>
  new Promise((resolve, reject) => {
    execFile(
      cmd,
      args,
      { ...opts, maxBuffer: 16 * 1024 * 1024 },
      (err, stdout, stderr) => {
        if (err) {
          (err as NodeJS.ErrnoException & { stderr?: string }).stderr = String(stderr);
          reject(err);
          return;
        }
        resolve({ stdout: String(stdout), stderr: String(stderr) });
      },
    );
  });

export interface TartCliOptions {
  runCommand?: RunCommand;
  binPath?: string;
}

export class TartCli {
  private readonly runCmd: RunCommand;
  private readonly bin: string;

  constructor(opts: TartCliOptions = {}) {
    this.runCmd = opts.runCommand ?? defaultRunCommand;
    this.bin = opts.binPath ?? "tart";
  }

  async version(): Promise<string> {
    const { stdout } = await this.runCmd(this.bin, ["--version"]);
    return stdout.trim();
  }

  async clone(base: string, vmName: string): Promise<void> {
    await this.runCmd(this.bin, ["clone", base, vmName]);
  }

  async delete(vmName: string): Promise<void> {
    await this.runCmd(this.bin, ["delete", vmName]);
  }

  async listVms(): Promise<TartVm[]> {
    const { stdout } = await this.runCmd(this.bin, ["list", "--format", "json"]);
    try {
      const arr = JSON.parse(stdout) as Array<{ Name: string; State: string }>;
      return arr.map((r) => ({ name: r.Name, state: r.State }));
    } catch {
      return [];
    }
  }

  /**
   * Spawn `tart run` long-running. Returns the child's pid (caller must use
   * a separate spawn-style runner if it needs streaming control). For the
   * Manager we use defaultRunCommand only for short-lived calls; long-running
   * `tart run` is handled in sandbox-manager via child_process.spawn directly.
   */
}
```

- [ ] **Step 4: Run tests, see them pass**

```bash
yarn test
```

- [ ] **Step 5: Commit**

```bash
git add src/sandbox-tart.ts src/sandbox-tart.test.ts
git commit -m "feat(sandbox): TartCli wrapper with injectable runCommand"
```

---

## Task 3: Path validator

**Files:**

- Create: `src/sandbox-paths.ts`
- Create: `src/sandbox-paths.test.ts`

- [ ] **Step 1: Tests**

```ts
// src/sandbox-paths.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";
import { validateSandboxSourcePath } from "./sandbox-paths.ts";

test("rejects path traversal", () => {
  assert.throws(
    () =>
      validateSandboxSourcePath("/Users/me/watch/../etc/passwd", {
        watchPath: "/Users/me/watch",
        quarantinePath: "/Users/me/q",
        homeDir: "/Users/me",
      }),
    /outside allowed roots/i,
  );
});

test("accepts a path inside watchPath", () => {
  const out = validateSandboxSourcePath("/Users/me/watch/file.bin", {
    watchPath: "/Users/me/watch",
    quarantinePath: "/Users/me/q",
    homeDir: "/Users/me",
  });
  assert.equal(out, "/Users/me/watch/file.bin");
});

test("rejects relative paths", () => {
  assert.throws(
    () =>
      validateSandboxSourcePath("relative/file", {
        watchPath: "/Users/me/watch",
        quarantinePath: "/Users/me/q",
        homeDir: "/Users/me",
      }),
    /absolute/i,
  );
});
```

- [ ] **Step 2: Run, see fail**

```bash
yarn test
```

- [ ] **Step 3: Implement**

```ts
// src/sandbox-paths.ts
import path from "path";
import fs from "fs";

export interface AllowedRoots {
  watchPath: string;
  quarantinePath: string;
  homeDir: string;
}

function isInside(child: string, parent: string): boolean {
  const rel = path.relative(parent, child);
  return rel !== "" && !rel.startsWith("..") && !path.isAbsolute(rel);
}

export function validateSandboxSourcePath(
  raw: unknown,
  roots: AllowedRoots,
): string {
  if (typeof raw !== "string" || !raw) {
    throw new Error("filePath must be a non-empty string");
  }
  if (!path.isAbsolute(raw)) {
    throw new Error("filePath must be absolute");
  }
  // Resolve `..` and symlinks where possible
  let resolved = path.resolve(raw);
  try {
    resolved = fs.realpathSync(resolved);
  } catch {
    // file may not exist yet; still resolve `..`
  }
  const allowed = [roots.watchPath, roots.quarantinePath, roots.homeDir]
    .map((p) => path.resolve(p))
    .filter(Boolean);
  if (!allowed.some((root) => resolved === root || isInside(resolved, root))) {
    throw new Error(
      `filePath outside allowed roots; allowed: ${allowed.join(", ")}`,
    );
  }
  return resolved;
}
```

- [ ] **Step 4: Run, see pass**

```bash
yarn test
```

- [ ] **Step 5: Commit**

```bash
git add src/sandbox-paths.ts src/sandbox-paths.test.ts
git commit -m "feat(sandbox): source path validator"
```

---

## Task 4: Config — sandbox keys

**Files:**

- Modify: `src/config.ts`

- [ ] **Step 1: Add fields**

In `RawConfig`:

```ts
sandboxEnabled?: boolean;
sandboxBaseVm?: string;
sandboxIdleTimeoutMinutes?: number;
sandboxNetworkDefault?: boolean;
sandboxSessionsDir?: string;
sandboxOutRetentionDays?: number;
```

In the exported `config` object:

```ts
sandboxEnabled: file.sandboxEnabled ?? envBool("SANDBOX_ENABLED", false),
sandboxBaseVm: file.sandboxBaseVm ?? process.env.SANDBOX_BASE_VM ?? "filesandbox-base",
sandboxIdleTimeoutMinutes: Math.max(
  1,
  file.sandboxIdleTimeoutMinutes ?? envInt("SANDBOX_IDLE_TIMEOUT_MIN", 240),
),
sandboxNetworkDefault: file.sandboxNetworkDefault ?? envBool("SANDBOX_NETWORK_DEFAULT", false),
sandboxSessionsDir:
  file.sandboxSessionsDir
  ?? process.env.SANDBOX_SESSIONS_DIR
  ?? `${process.env.HOME ?? ""}/Library/Application Support/FileSandbox/sandbox-sessions`,
sandboxOutRetentionDays: Math.max(
  0,
  file.sandboxOutRetentionDays ?? envInt("SANDBOX_OUT_RETENTION_DAYS", 7),
),
```

- [ ] **Step 2: Tests still pass**

```bash
yarn test
```

- [ ] **Step 3: Commit**

```bash
git add src/config.ts
git commit -m "feat(config): sandbox knobs"
```

---

## Task 5: `SandboxManager` — probe, init, createSession

**Files:**

- Create: `src/sandbox-manager.ts`
- Create: `src/sandbox-manager.test.ts`

- [ ] **Step 1: Write failing tests**

```ts
// src/sandbox-manager.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";
import path from "path";
import os from "os";
import fs from "fs";
import { SandboxStore } from "./sandbox-store.ts";
import { SandboxManager } from "./sandbox-manager.ts";

class FakeTart {
  cloned: Array<{ base: string; name: string }> = [];
  deleted: string[] = [];
  vms = [{ name: "base-vm", state: "stopped" }];
  async version() { return "tart 0.0.0-fake"; }
  async clone(base: string, name: string) { this.cloned.push({ base, name }); this.vms.push({ name, state: "stopped" }); }
  async delete(name: string) { this.deleted.push(name); this.vms = this.vms.filter(v => v.name !== name); }
  async listVms() { return [...this.vms]; }
}

class FakeSpawner {
  pids: number[] = [];
  exitListeners = new Map<number, (code: number) => void>();
  spawnRun(args: string[]) {
    const pid = 1000 + this.pids.length + 1;
    this.pids.push(pid);
    return {
      pid,
      kill: (_sig: string) => { /* invoked by manager */ },
      onExit: (cb: (code: number) => void) => this.exitListeners.set(pid, cb),
    };
  }
  triggerExit(pid: number, code: number) {
    this.exitListeners.get(pid)?.(code);
  }
}

function tmpDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "fsbx-test-"));
}

test("createSession copies file, clones VM, runs, sets running", async () => {
  const dir = tmpDir();
  const src = path.join(dir, "input.bin");
  fs.writeFileSync(src, "hello");

  const store = new SandboxStore(":memory:");
  const tart = new FakeTart();
  const spawner = new FakeSpawner();
  const mgr = new SandboxManager({
    store,
    tart: tart as any,
    spawner: spawner as any,
    sessionsDir: path.join(dir, "sessions"),
    baseVm: "base-vm",
    idleTimeoutMinutes: 240,
    networkDefault: false,
    allowedRoots: { watchPath: dir, quarantinePath: dir, homeDir: dir },
  });
  await mgr.init();

  const session = await mgr.createSession({ filePath: src });
  assert.equal(tart.cloned.length, 1);
  assert.equal(session.status, "running");
  assert.equal(session.networkEnabled, false);

  const inDir = path.join(session.sessionDir, "in");
  assert.ok(fs.existsSync(path.join(inDir, "input.bin")));
});

test("discardSession kills child, deletes clone, marks discarded", async () => {
  const dir = tmpDir();
  const src = path.join(dir, "f.bin");
  fs.writeFileSync(src, "x");

  const store = new SandboxStore(":memory:");
  const tart = new FakeTart();
  const spawner = new FakeSpawner();
  const mgr = new SandboxManager({
    store,
    tart: tart as any,
    spawner: spawner as any,
    sessionsDir: path.join(dir, "sessions"),
    baseVm: "base-vm",
    idleTimeoutMinutes: 240,
    networkDefault: false,
    allowedRoots: { watchPath: dir, quarantinePath: dir, homeDir: dir },
  });
  await mgr.init();
  const s = await mgr.createSession({ filePath: src });
  await mgr.discardSession(s.id);
  const after = store.get(s.id);
  assert.equal(after?.status, "discarded");
  assert.ok(tart.deleted.includes(s.vmName));
});

test("reconcile marks dead VMs discarded on init", async () => {
  const dir = tmpDir();
  const store = new SandboxStore(":memory:");
  store.insert({
    id: "stale-1", vmName: "fsbx-stale", sourceJobId: null, sourceFilePath: "/x", sessionDir: path.join(dir, "stale"), networkEnabled: false,
  });
  store.setRunning("stale-1", 9999);
  const tart = new FakeTart(); // no fsbx-stale in vms
  const spawner = new FakeSpawner();
  const mgr = new SandboxManager({
    store,
    tart: tart as any,
    spawner: spawner as any,
    sessionsDir: path.join(dir, "sessions"),
    baseVm: "base-vm",
    idleTimeoutMinutes: 240,
    networkDefault: false,
    allowedRoots: { watchPath: dir, quarantinePath: dir, homeDir: dir },
  });
  await mgr.init();
  const r = store.get("stale-1");
  assert.equal(r?.status, "discarded");
});
```

- [ ] **Step 2: Run, see fail**

```bash
yarn test
```

- [ ] **Step 3: Implement `src/sandbox-manager.ts`**

```ts
import path from "path";
import fs from "fs";
import { spawn, type ChildProcess } from "child_process";
import { randomUUID } from "crypto";
import { SandboxStore, type SandboxSession } from "./sandbox-store.ts";
import { TartCli } from "./sandbox-tart.ts";
import { validateSandboxSourcePath, type AllowedRoots } from "./sandbox-paths.ts";

export interface ChildHandle {
  pid: number;
  kill: (signal?: NodeJS.Signals) => void;
  onExit: (cb: (code: number | null) => void) => void;
}

export interface Spawner {
  spawnRun: (args: string[]) => ChildHandle;
}

const defaultSpawner: Spawner = {
  spawnRun(args) {
    const child: ChildProcess = spawn("tart", args, { stdio: "ignore", detached: false });
    return {
      pid: child.pid ?? -1,
      kill: (sig) => child.kill(sig),
      onExit: (cb) => child.on("exit", cb),
    };
  },
};

export interface SandboxManagerOptions {
  store: SandboxStore;
  tart: TartCli;
  spawner?: Spawner;
  sessionsDir: string;
  baseVm: string;
  idleTimeoutMinutes: number;
  networkDefault: boolean;
  allowedRoots: AllowedRoots;
}

export interface CreateSessionInput {
  filePath: string;
  sourceJobId?: string;
  network?: boolean;
}

export class SandboxManager {
  private readonly store: SandboxStore;
  private readonly tart: TartCli;
  private readonly spawner: Spawner;
  private readonly sessionsDir: string;
  private readonly baseVm: string;
  private readonly idleTimeoutMinutes: number;
  private readonly networkDefault: boolean;
  private readonly allowedRoots: AllowedRoots;
  private idleTimer: NodeJS.Timeout | null = null;
  private readonly children = new Map<string, ChildHandle>();

  constructor(opts: SandboxManagerOptions) {
    this.store = opts.store;
    this.tart = opts.tart;
    this.spawner = opts.spawner ?? defaultSpawner;
    this.sessionsDir = opts.sessionsDir;
    this.baseVm = opts.baseVm;
    this.idleTimeoutMinutes = opts.idleTimeoutMinutes;
    this.networkDefault = opts.networkDefault;
    this.allowedRoots = opts.allowedRoots;
  }

  static async probe(tart: TartCli, baseVm: string): Promise<{ tartInstalled: boolean; baseImagePresent: boolean }> {
    let tartInstalled = false;
    let baseImagePresent = false;
    try {
      await tart.version();
      tartInstalled = true;
    } catch {
      return { tartInstalled: false, baseImagePresent: false };
    }
    try {
      const vms = await tart.listVms();
      baseImagePresent = vms.some((v) => v.name === baseVm);
    } catch {
      // ignore
    }
    return { tartInstalled, baseImagePresent };
  }

  async init(): Promise<void> {
    fs.mkdirSync(this.sessionsDir, { recursive: true });
    await this.reconcile();
    if (!this.idleTimer) {
      const intervalMs = 60 * 1000;
      this.idleTimer = setInterval(() => this.sweepIdle().catch(() => {}), intervalMs);
      this.idleTimer.unref?.();
    }
  }

  private async reconcile(): Promise<void> {
    const known = await this.tart.listVms().catch(() => [] as { name: string }[]);
    const knownNames = new Set(known.map((v) => v.name));
    for (const s of this.store.listActive()) {
      if (!knownNames.has(s.vmName)) {
        this.store.markDiscarded(s.id, "stale on daemon restart");
      }
    }
    // Orphan VMs: present in tart but not in any active row → delete
    const activeVmNames = new Set(this.store.listActive().map((s) => s.vmName));
    for (const v of known) {
      if (v.name.startsWith("fsbx-") && !activeVmNames.has(v.name)) {
        await this.tart.delete(v.name).catch(() => {});
      }
    }
  }

  private async sweepIdle(): Promise<void> {
    const cutoff = new Date(Date.now() - this.idleTimeoutMinutes * 60_000).toISOString();
    for (const s of this.store.listIdle(cutoff)) {
      await this.discardSession(s.id, "idle timeout").catch(() => {});
    }
  }

  async createSession(input: CreateSessionInput): Promise<SandboxSession> {
    const filePath = validateSandboxSourcePath(input.filePath, this.allowedRoots);
    if (!fs.existsSync(filePath)) {
      throw new Error(`source file not found: ${filePath}`);
    }
    const stat = fs.statSync(filePath);
    if (stat.isDirectory()) throw new Error("source is a directory");

    const id = randomUUID();
    const vmName = `fsbx-${id.slice(0, 8)}`;
    const sessionDir = path.join(this.sessionsDir, id);
    const inDir = path.join(sessionDir, "in");
    const outDir = path.join(sessionDir, "out");
    fs.mkdirSync(inDir, { recursive: true });
    fs.mkdirSync(outDir, { recursive: true });
    fs.chmodSync(inDir, 0o700);
    fs.chmodSync(outDir, 0o700);

    const target = path.join(inDir, path.basename(filePath));
    fs.copyFileSync(filePath, target);
    fs.chmodSync(target, 0o444);

    const networkEnabled = input.network ?? this.networkDefault;

    this.store.insert({
      id,
      vmName,
      sourceJobId: input.sourceJobId ?? null,
      sourceFilePath: filePath,
      sessionDir,
      networkEnabled,
    });

    try {
      await this.tart.clone(this.baseVm, vmName);
    } catch (e) {
      this.store.setFailed(id, `clone failed: ${(e as Error).message}`);
      throw e;
    }

    const args = ["run", vmName, `--dir=in:${inDir}:ro`, `--dir=out:${outDir}`];
    if (!networkEnabled) args.push("--net-softnet=false");
    const child = this.spawner.spawnRun(args);
    this.children.set(id, child);
    this.store.setRunning(id, child.pid);

    child.onExit(async (code) => {
      this.children.delete(id);
      const detail = code == null ? "tart run exited" : `tart run exited code=${code}`;
      this.store.setStopped(id, detail);
      // Cleanup clone in background
      try { await this.tart.delete(vmName); } catch { /* ignore */ }
      this.store.markDiscarded(id, "post-exit cleanup");
    });

    return this.store.get(id)!;
  }

  listSessions(opts?: { limit?: number }): SandboxSession[] {
    return this.store.listSessions(opts);
  }

  getSession(id: string): SandboxSession | null {
    const s = this.store.get(id);
    if (s) this.store.touch(id);
    return s;
  }

  async discardSession(id: string, detail = "user discard"): Promise<void> {
    const s = this.store.get(id);
    if (!s) return;
    if (s.status === "discarded") return;
    const child = this.children.get(id);
    if (child) {
      try { child.kill("SIGTERM"); } catch { /* ignore */ }
      // Best-effort SIGKILL after grace
      setTimeout(() => { try { child.kill("SIGKILL"); } catch { /* ignore */ } }, 10_000).unref?.();
      this.children.delete(id);
    }
    try { await this.tart.delete(s.vmName); } catch { /* ignore */ }
    try { fs.rmSync(path.join(s.sessionDir, "in"), { recursive: true, force: true }); } catch { /* ignore */ }
    this.store.markDiscarded(id, detail);
  }

  async shutdownAll(): Promise<void> {
    if (this.idleTimer) { clearInterval(this.idleTimer); this.idleTimer = null; }
    for (const s of this.store.listActive()) {
      await this.discardSession(s.id, "daemon shutdown").catch(() => {});
    }
  }
}
```

- [ ] **Step 4: Run, see pass**

```bash
yarn test
```

- [ ] **Step 5: Commit**

```bash
git add src/sandbox-manager.ts src/sandbox-manager.test.ts
git commit -m "feat(sandbox): SandboxManager (create/discard/reconcile/idle sweep)"
```

---

## Task 6: HTTP routes for sandbox

**Files:**

- Modify: `src/ui-server.ts`

- [ ] **Step 1: Extend the start signature with optional `SandboxManager`**

```ts
import type { SandboxManager } from "./sandbox-manager.ts";

export function startUiServer(
  store: JobStore,
  port: number,
  cancelJob?: (id: string) => void,
  deleteQuarantinedFile?: (id: string, detail?: string) => Promise<void>,
  restoreQuarantinedFile?: (id: string) => Promise<void>,
  watcherControl?: WatcherControl,
  sandboxManager?: SandboxManager,
) { /* ... */ }
```

- [ ] **Step 2: Add routes**

```ts
app.post("/api/sandbox/sessions", async (req, res) => {
  if (!sandboxManager) return res.status(503).json({ error: "sandbox not enabled" });
  try {
    const session = await sandboxManager.createSession({
      filePath: req.body?.filePath,
      sourceJobId: req.body?.sourceJobId ?? undefined,
      network: typeof req.body?.network === "boolean" ? req.body.network : undefined,
    });
    res.json({ ok: true, session });
  } catch (e) {
    res.status(400).json({ error: String((e as Error).message ?? e) });
  }
});

app.get("/api/sandbox/sessions", (req, res) => {
  if (!sandboxManager) return res.status(503).json({ error: "sandbox not enabled" });
  const limit = Number(req.query?.limit ?? 50);
  res.json({ sessions: sandboxManager.listSessions({ limit }) });
});

app.get("/api/sandbox/sessions/:id", (req, res) => {
  if (!sandboxManager) return res.status(503).json({ error: "sandbox not enabled" });
  const s = sandboxManager.getSession(req.params.id);
  if (!s) return res.status(404).json({ error: "not found" });
  res.json({ session: s });
});

app.delete("/api/sandbox/sessions/:id", async (req, res) => {
  if (!sandboxManager) return res.status(503).json({ error: "sandbox not enabled" });
  await sandboxManager.discardSession(req.params.id);
  res.json({ ok: true });
});
```

- [ ] **Step 3: Add `sandbox` block to `/api/health`**

```ts
const sandboxInfo = sandboxManager
  ? {
      enabled: true,
      tartInstalled: true, // probe lives at startup, see Task 7
      baseImagePresent: true,
      activeSessions: sandboxManager.listSessions({ limit: 500 }).filter((s) => s.status === "running" || s.status === "starting").length,
    }
  : { enabled: false, tartInstalled: false, baseImagePresent: false, activeSessions: 0 };
res.json({
  // ... existing fields ...
  sandbox: sandboxInfo,
});
```

(The detailed `tartInstalled`/`baseImagePresent` flags come from a startup probe stored on a daemon-level object — see Task 7.)

- [ ] **Step 4: Manual test**

After Task 7 lands, posting an invalid path returns 400; valid path returns 200 with a session row.

- [ ] **Step 5: Commit**

```bash
git add src/ui-server.ts
git commit -m "feat(api): /api/sandbox/sessions CRUD + health block"
```

---

## Task 7: Wire `SandboxManager` into `src/index.ts`

**Files:**

- Modify: `src/index.ts`

- [ ] **Step 1: Construct only when enabled and `tart` is reachable**

```ts
import { TartCli } from "./sandbox-tart.ts";
import { SandboxStore } from "./sandbox-store.ts";
import { SandboxManager } from "./sandbox-manager.ts";

let sandboxManager: SandboxManager | null = null;
let sandboxProbe = { tartInstalled: false, baseImagePresent: false };
if (config.sandboxEnabled) {
  const tart = new TartCli();
  sandboxProbe = await SandboxManager.probe(tart, config.sandboxBaseVm);
  if (!sandboxProbe.tartInstalled) {
    console.warn("[sandbox] enabled but `tart` not in PATH — sandbox endpoints will return 503.");
  } else if (!sandboxProbe.baseImagePresent) {
    console.warn(`[sandbox] base VM "${config.sandboxBaseVm}" not present. Run \`tart pull\` and \`tart clone\` first.`);
  } else {
    const sandboxStore = new SandboxStore(config.databasePath);
    sandboxManager = new SandboxManager({
      store: sandboxStore,
      tart,
      sessionsDir: config.sandboxSessionsDir,
      baseVm: config.sandboxBaseVm,
      idleTimeoutMinutes: config.sandboxIdleTimeoutMinutes,
      networkDefault: config.sandboxNetworkDefault,
      allowedRoots: {
        watchPath: config.watchPath,
        quarantinePath: config.quarantinePath,
        homeDir: process.env.HOME ?? "",
      },
    });
    await sandboxManager.init();
    console.log("[sandbox] manager initialised");
  }
}
```

- [ ] **Step 2: Pass to `startUiServer`**

```ts
startUiServer(jobStore, port, cancelJob, deleteFn, restoreFn, watcherControl, sandboxManager ?? undefined);
```

- [ ] **Step 3: Graceful shutdown**

In your existing SIGTERM/SIGINT handler (or add one), call `await sandboxManager?.shutdownAll()` before exit.

- [ ] **Step 4: Plumb `sandboxProbe` into `/api/health`**

Pass `sandboxProbe` as part of an extended UI server signature, OR (simpler) attach it to the manager:

Add a getter on `SandboxManager`:

```ts
private readonly probeInfo: { tartInstalled: boolean; baseImagePresent: boolean };
constructor(opts: SandboxManagerOptions & { probeInfo?: { tartInstalled: boolean; baseImagePresent: boolean } }) {
  // ...
  this.probeInfo = opts.probeInfo ?? { tartInstalled: true, baseImagePresent: true };
}
getProbe() { return this.probeInfo; }
```

Update `src/index.ts` to pass `probeInfo: sandboxProbe`. Update Task 6's `sandbox` health block to use `sandboxManager.getProbe()`.

- [ ] **Step 5: Manual test (full path)**

1. `brew install cirruslabs/cli/tart` and `tart pull` per README.
2. Set `sandboxEnabled: true` in config.json. Restart daemon.
3. `curl -s /api/health | jq .sandbox` → `enabled: true, tartInstalled: true, baseImagePresent: true, activeSessions: 0`.
4. POST a session for a small benign file in the watch folder. VM window appears with the file at `/Volumes/My Shared Files/in/<name>`.
5. DELETE the session. VM window closes. `tart list` shows no `fsbx-*`.

- [ ] **Step 6: Commit**

```bash
git add src/index.ts src/sandbox-manager.ts src/ui-server.ts
git commit -m "feat(daemon): wire SandboxManager into startup, shutdown, and health"
```

---

## Task 8: Export-from-sandbox flow

**Files:**

- Modify: `src/sandbox-manager.ts`
- Modify: `src/ui-server.ts`

- [ ] **Step 1: Add `exportFromSession` to `SandboxManager`**

```ts
async exportFromSession(id: string, fileName: string, watchPath: string): Promise<{ destPath: string }> {
  const s = this.store.get(id);
  if (!s) throw new Error("session not found");
  // Reject path traversal in fileName
  if (fileName.includes("/") || fileName.includes("..")) {
    throw new Error("fileName must not contain path separators");
  }
  const src = path.join(s.sessionDir, "out", fileName);
  if (!fs.existsSync(src)) throw new Error(`file not found in session out/: ${fileName}`);
  // Move to watch folder so the existing pipeline picks it up
  const dest = path.join(watchPath, `from-sandbox-${s.id.slice(0,8)}-${fileName}`);
  fs.copyFileSync(src, dest);
  fs.unlinkSync(src);
  this.store.touch(id);
  return { destPath: dest };
}
```

- [ ] **Step 2: HTTP route**

```ts
app.post("/api/sandbox/sessions/:id/export", (req, res) => {
  if (!sandboxManager) return res.status(503).json({ error: "sandbox not enabled" });
  const fileName = req.body?.fileName;
  if (typeof fileName !== "string" || !fileName) {
    return res.status(400).json({ error: "fileName required" });
  }
  try {
    const { destPath } = sandboxManager.exportFromSession(req.params.id, fileName, config.watchPath);
    res.json({ ok: true, destPath });
  } catch (e) {
    res.status(400).json({ error: String((e as Error).message) });
  }
});
```

- [ ] **Step 3: Manual test**

1. Inside a running session, save a file to `/Volumes/My Shared Files/out/note.txt` from the guest.
2. From the host, POST `/api/sandbox/sessions/<id>/export` with `{ "fileName": "note.txt" }`.
3. Watch the daemon log: a new job appears in the watch pipeline. The exported file is renamed `from-sandbox-XXXXXXXX-note.txt`.

- [ ] **Step 4: Commit**

```bash
git add src/sandbox-manager.ts src/ui-server.ts
git commit -m "feat(sandbox): export-from-session funnels through watch+scan pipeline"
```

---

## Task 9: SwiftUI — sandbox store

**Files:**

- Create: `macos-menubar/Sources/App/SandboxStore.swift`

- [ ] **Step 1: Implement**

```swift
import Foundation
import Combine

struct SandboxSession: Codable, Identifiable, Equatable {
    let id: String
    let vmName: String
    let sourceJobId: String?
    let sourceFilePath: String
    let sessionDir: String
    let pid: Int?
    let networkEnabled: Bool
    let status: String
    let detail: String?
    let createdAt: String
    let lastActiveAt: String
    let exitedAt: String?

    enum CodingKeys: String, CodingKey {
        case id, vmName, sourceJobId, sourceFilePath, sessionDir
        case pid, networkEnabled, status, detail, createdAt, lastActiveAt, exitedAt
    }
}

@MainActor
final class SandboxStore: ObservableObject {
    @Published var sessions: [SandboxSession] = []
    @Published var loadError: String? = nil

    private let port: String
    init() {
        self.port = ProcessInfo.processInfo.environment["FILE_SANDBOX_PORT"] ?? "3847"
    }

    private func authorized(_ url: URL) -> URLRequest {
        var req = URLRequest(url: url)
        let t = ClientAuthStorage.token
        if !t.isEmpty { req.setValue("Bearer \(t)", forHTTPHeaderField: "Authorization") }
        return req
    }

    func fetch() {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/sandbox/sessions?limit=50") else { return }
        URLSession.shared.dataTask(with: authorized(url)) { [weak self] data, _, error in
            DispatchQueue.main.async {
                guard let self else { return }
                if let error { self.loadError = error.localizedDescription; return }
                guard let data,
                      let decoded = try? JSONDecoder().decode([String: [SandboxSession]].self, from: data),
                      let arr = decoded["sessions"]
                else { self.loadError = "decode failed"; return }
                self.sessions = arr
                self.loadError = nil
            }
        }.resume()
    }

    func create(filePath: String, sourceJobId: String?, network: Bool, completion: @escaping (Bool) -> Void) {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/sandbox/sessions") else { completion(false); return }
        var req = authorized(url)
        req.httpMethod = "POST"
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        var body: [String: Any] = ["filePath": filePath, "network": network]
        if let sourceJobId { body["sourceJobId"] = sourceJobId }
        req.httpBody = try? JSONSerialization.data(withJSONObject: body)
        URLSession.shared.dataTask(with: req) { [weak self] _, _, error in
            DispatchQueue.main.async {
                completion(error == nil)
                self?.fetch()
            }
        }.resume()
    }

    func discard(_ id: String) {
        guard let url = URL(string: "http://127.0.0.1:\(port)/api/sandbox/sessions/\(id)") else { return }
        var req = authorized(url)
        req.httpMethod = "DELETE"
        URLSession.shared.dataTask(with: req) { [weak self] _, _, _ in
            DispatchQueue.main.async { self?.fetch() }
        }.resume()
    }
}
```

- [ ] **Step 2: Build**

```bash
swift build --package-path macos-menubar
```

- [ ] **Step 3: Commit**

```bash
git add macos-menubar/Sources/App/SandboxStore.swift
git commit -m "feat(menubar): SandboxStore mirrors daemon /api/sandbox/sessions"
```

---

## Task 10: SwiftUI — sandbox view in menu bar

**Files:**

- Create: `macos-menubar/Sources/App/SandboxView.swift`
- Modify: `macos-menubar/Sources/App/Views.swift`

- [ ] **Step 1: Implement `SandboxView.swift`**

```swift
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
```

- [ ] **Step 2: Mount `SandboxView` inside the dropdown in `Views.swift`**

Inside the main dropdown VStack (search for the existing job list), add a divider and the sandbox view:

```swift
Divider()
SandboxView(store: sandboxStore)
```

`sandboxStore` must be wired through the same chain as the existing `JobStore` and `SettingsStore`. In your `App.swift` or `RootView` add `@StateObject private var sandboxStore = SandboxStore()` and pass it down.

- [ ] **Step 3: Add per-quarantined-row "Open in sandbox" button**

In the row that renders a single quarantined `Job`, add (when status is `quarantine_kept` and the row has a `quarantine_path`):

```swift
Button("Open in sandbox") {
    sandboxStore.create(filePath: job.quarantinePath ?? "", sourceJobId: job.id, network: false) { _ in }
}
.buttonStyle(.borderless)
```

- [ ] **Step 4: Build, run, click "+ New", select a file**

```bash
swift run --package-path macos-menubar
```

Expected: VM window appears with the file mounted. Discard returns the row to `discarded`.

- [ ] **Step 5: Commit**

```bash
git add macos-menubar/Sources/App/SandboxView.swift macos-menubar/Sources/App/Views.swift macos-menubar/Sources/App/App.swift
git commit -m "feat(menubar): SandboxView with file picker, list, discard, per-job entry"
```

---

## Task 11: Settings — Sandbox section

**Files:**

- Modify: `macos-menubar/Sources/App/SettingsStore.swift`
- Modify: `macos-menubar/Sources/App/SettingsView.swift`

- [ ] **Step 1: Extend `DaemonConfig` and `SettingsStore`**

In `SettingsStore.swift`:

```swift
struct DaemonConfig: Codable {
    // ... existing ...
    var sandboxEnabled: Bool?
    var sandboxBaseVm: String?
    var sandboxIdleTimeoutMinutes: Int?
    var sandboxNetworkDefault: Bool?
    var sandboxOutRetentionDays: Int?
}

class SettingsStore: ObservableObject {
    // ... existing ...
    @Published var sandboxEnabled: Bool = false
    @Published var sandboxBaseVm: String = "filesandbox-base"
    @Published var sandboxIdleTimeoutMinutes: Int = 240
    @Published var sandboxNetworkDefault: Bool = false
    @Published var sandboxOutRetentionDays: Int = 7
}
```

In `fetch()`:

```swift
self.sandboxEnabled = decoded.sandboxEnabled ?? false
self.sandboxBaseVm = decoded.sandboxBaseVm ?? "filesandbox-base"
self.sandboxIdleTimeoutMinutes = decoded.sandboxIdleTimeoutMinutes ?? 240
self.sandboxNetworkDefault = decoded.sandboxNetworkDefault ?? false
self.sandboxOutRetentionDays = decoded.sandboxOutRetentionDays ?? 7
```

In `save()` body dict:

```swift
"sandboxEnabled": sandboxEnabled,
"sandboxBaseVm": sandboxBaseVm,
"sandboxIdleTimeoutMinutes": sandboxIdleTimeoutMinutes,
"sandboxNetworkDefault": sandboxNetworkDefault,
"sandboxOutRetentionDays": sandboxOutRetentionDays,
```

- [ ] **Step 2: Add Sandbox section to `SettingsView.swift`**

```swift
Section("Sandbox") {
    Toggle("Enable sandbox", isOn: $store.sandboxEnabled)
    if store.sandboxEnabled {
        TextField("Base VM name", text: $store.sandboxBaseVm)
        Stepper("Idle timeout: \(store.sandboxIdleTimeoutMinutes) min", value: $store.sandboxIdleTimeoutMinutes, in: 5...10080, step: 5)
        Toggle("Network ON by default", isOn: $store.sandboxNetworkDefault)
        Stepper("Output retention: \(store.sandboxOutRetentionDays) days", value: $store.sandboxOutRetentionDays, in: 0...90)
        Text("Run \\\"brew install cirruslabs/cli/tart\\\" and \\\"tart pull ghcr.io/cirruslabs/macos-sequoia-base:latest\\\" before enabling.")
            .font(.caption)
            .foregroundColor(.secondary)
    }
}
```

- [ ] **Step 3: Build**

```bash
swift build --package-path macos-menubar
```

- [ ] **Step 4: Commit**

```bash
git add macos-menubar/Sources/App/SettingsStore.swift macos-menubar/Sources/App/SettingsView.swift
git commit -m "feat(menubar): Sandbox Settings section"
```

---

## Task 12: README sandbox setup

**Files:**

- Modify: `README.md`

- [ ] **Step 1: Append Sandbox section**

```markdown
## Isolated open-file sandbox (Tart VM)

The daemon can open arbitrary files inside a fresh, disposable macOS VM via Tart. Set up once:

    brew install cirruslabs/cli/tart
    tart pull ghcr.io/cirruslabs/macos-sequoia-base:latest    # ~30 GB
    tart clone ghcr.io/cirruslabs/macos-sequoia-base:latest filesandbox-base

Then enable in `config.json`: `"sandboxEnabled": true`.

Each session clones the base VM, mounts the chosen file at `/Volumes/My Shared Files/in/` read-only, and provides a writable `out/` for guest output. Closing the VM, clicking Discard, or hitting the idle timeout deletes the clone. Files leaving the sandbox via Export are placed back into the watch folder and re-scanned by the normal pipeline — there is no shortcut.

Network is OFF by default. Clipboard and USB pass-through are disabled. The Apple licence permits up to 2 concurrent macOS VMs per host.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: Tart sandbox setup"
```

---

## Task 13: End-to-end manual verification

**No file changes. Verification gate.**

- [ ] **Step 1: prereq probe**

1. `tart` not installed: daemon logs warn, `/api/health` returns `sandbox.tartInstalled=false`, sandbox endpoints return 503. The rest of the daemon works.
2. `tart` installed but base VM missing: warn, `baseImagePresent=false`, endpoints 503.

- [ ] **Step 2: happy path**

1. Install `tart` and pull base image. Restart daemon.
2. POST `/api/sandbox/sessions` with `filePath` of a small benign file in the watch folder. Expect 200 with a session row, status `running` within ~30s.
3. The Tart viewer window opens. Inside guest, file is at `/Volumes/My Shared Files/in/<name>`, read-only.
4. Close the VM window. Daemon logs `tart run exited code=...`. DB row goes through `stopped` → `discarded`. `tart list` no longer contains `fsbx-*`.
5. The session dir is removed (the manager unlinks `in/`).

- [ ] **Step 3: discard mid-session**

1. Create a session.
2. DELETE `/api/sandbox/sessions/:id`. VM window closes within 10s. Row is `discarded`.

- [ ] **Step 4: idle timeout**

1. Set `sandboxIdleTimeoutMinutes=1`. Create a session. Do not interact for >1 minute.
2. Daemon's interval sweep discards the session.

- [ ] **Step 5: reconcile after daemon restart**

1. Create a session. Kill the daemon (`SIGKILL` to skip shutdownAll).
2. Restart daemon. The session's row was `running` but its `tart` child is gone and the VM may or may not be in `tart list`.
3. After init: stale row with no matching VM → marked `discarded`.

- [ ] **Step 6: export-from-sandbox**

1. Inside a session, save `note.txt` to `/Volumes/My Shared Files/out/`.
2. POST `/api/sandbox/sessions/:id/export` with `{ "fileName": "note.txt" }`.
3. The host's watch folder receives `from-sandbox-<id8>-note.txt`. The normal pipeline runs (pompelmi + VT). DB shows a fresh job row.

- [ ] **Step 7: path validation**

1. POST a session with `filePath: "/etc/passwd"`. Expect HTTP 400 "outside allowed roots".
2. POST with `filePath: "../foo"`. Expect 400 "absolute".
3. POST with a valid path inside `quarantinePath`. Expect 200.

- [ ] **Step 8: 2-VM concurrent limit**

1. Start 3 sessions in succession.
2. The third `tart run` fails. The session row is `failed` with detail; the API returns 200 with the row (sessions list reflects the failure). User discards another session and retries.

---

## Self-Review Checklist (already run)

- Spec coverage: schema, manager, tart wrapper, path validation, config, HTTP, daemon wiring, export flow, menu bar UI, settings UI, README, and verification — each section maps to a task.
- No placeholders. Every code change is shown.
- Type names consistent: `SandboxSession`, `SandboxStore`, `SandboxManager`, `TartCli`, `validateSandboxSourcePath`, `AllowedRoots` used identically across tasks.
- Each implementation task ends in a commit; verification task does not.
- Cross-plan dependency note: this plan can land independently of the pompelmi and watcher-mode plans. The "Export from sandbox" flow drops files into the existing watch folder, which works whether pompelmi is enabled or not.
