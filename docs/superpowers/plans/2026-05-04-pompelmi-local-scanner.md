# pompelmi local scanner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a local-first ClamAV-based scanner upstream of the existing VirusTotal pipeline using the `pompelmi` npm package, so malicious files are caught locally before any cloud upload.

**Architecture:** The watcher pipeline gains a new local-scan stage between `setScanning` and the existing VT call. A new `LocalScanner` class wraps `pompelmi.createScanner` and connects to a host-managed `clamd` daemon over a UNIX socket. Verdicts are stored in a new `pompelmi_verdict` column on `jobs`. The existing `vt-cache` and semaphore are unchanged.

**Tech Stack:** Node 22 with `--experimental-strip-types`, TypeScript via type-stripping, `pompelmi` npm package, ClamAV/`clamd` (host-installed), yarn, `better-sqlite3`, `node:test` for tests.

---

## Reference

- Spec: `docs/superpowers/specs/2026-05-04-pompelmi-local-scanner-design.md`
- Related: pompelmi GitHub https://github.com/pompelmi/pompelmi
- Affected files: `src/config.ts`, `src/job-store.ts`, `src/watcher.ts`, `src/ui-server.ts`, `src/index.ts`, `package.json`, `README.md`, plus a new `src/local-scanner.ts`.

## File Structure

| File | Responsibility | Status |
|---|---|---|
| `package.json` | Switch from npm to yarn, add `pompelmi`, add `test` script. | Modify |
| `yarn.lock` | Lockfile after yarn install. | Create |
| `src/local-scanner.ts` | `LocalScanner` class, `LocalVerdict` type, `probe` static method. | Create |
| `src/local-scanner.test.ts` | Unit tests for verdict translation, probe error path. | Create |
| `src/config.ts` | Add `pompelmiEnabled`, `pompelmiSocketPath`, `pompelmiFailureMode`. | Modify |
| `src/job-store.ts` | Add `pompelmi_verdict` column + setter, expose in `Job`. | Modify |
| `src/watcher.ts` | Insert local-scan stage in `handleFile`, branch on engine flags + failure mode. | Modify |
| `src/ui-server.ts` | Expose `pompelmi_verdict` (via `Job` row), augment `/api/health`. | Modify |
| `src/index.ts` | Construct and probe `LocalScanner`; refuse start if reachable=false and enabled=true; pass to `Watcher`. | Modify |
| `README.md` | Document ClamAV install, `clamd.conf` edits, troubleshooting. | Modify |

---

## Task 1: Switch to yarn

**Files:**

- Delete: `package-lock.json`
- Modify: `package.json`
- Create: `yarn.lock`

- [ ] **Step 1: Verify yarn installed**

```bash
yarn --version
```

Expected: Yarn version output. If missing: `corepack enable && corepack prepare yarn@stable --activate`.

- [ ] **Step 2: Delete npm lockfile, run yarn**

```bash
rm package-lock.json
yarn install
```

Expected: `yarn.lock` is created. `node_modules` populated.

- [ ] **Step 3: Add `test` script to `package.json`**

Modify the `scripts` block of `package.json` to:

```json
"scripts": {
  "start": "node --experimental-strip-types src/index.ts",
  "start:local": "node --experimental-strip-types --env-file=.env src/index.ts",
  "test": "node --experimental-strip-types --test 'src/**/*.test.ts'"
}
```

- [ ] **Step 4: Verify yarn test runs (no tests yet, exit 0)**

```bash
yarn test
```

Expected: `# tests 0` and exit 0.

- [ ] **Step 5: Commit**

```bash
git add package.json yarn.lock
git rm package-lock.json
git commit -m "chore: switch to yarn, add node:test runner"
```

---

## Task 2: Pin pompelmi dependency

**Files:**

- Modify: `package.json`

- [ ] **Step 1: Install pompelmi**

```bash
yarn add pompelmi
```

Expected: `package.json` `dependencies` gains `"pompelmi": "^x.y.z"`. `yarn.lock` updates.

- [ ] **Step 2: Sanity-check the import works under type-stripping**

Create temporary `scripts/check-pompelmi.ts`:

```ts
import { Verdict } from "pompelmi";
console.log(Object.keys(Verdict ?? {}));
```

Run:

```bash
node --experimental-strip-types scripts/check-pompelmi.ts
```

Expected: prints something like `[ 'Clean', 'Malicious', 'ScanError' ]` (exact symbol names from pompelmi). Delete the temp file.

- [ ] **Step 3: Commit**

```bash
git add package.json yarn.lock
git commit -m "feat: add pompelmi dependency"
```

---

## Task 3: Schema column for pompelmi verdict

**Files:**

- Modify: `src/job-store.ts`
- Test: `src/job-store.test.ts`

- [ ] **Step 1: Write failing test for `pompelmi_verdict` round-trip**

Create `src/job-store.test.ts`:

```ts
import { test } from "node:test";
import assert from "node:assert/strict";
import { JobStore } from "./job-store.ts";

function freshStore() {
  return new JobStore(":memory:");
}

test("pompelmi_verdict starts null and persists when set", () => {
  const store = freshStore();
  store.insertReceived("job-1", "/a.bin", "a.bin");
  const before = store.get("job-1");
  assert.equal(before?.pompelmi_verdict, null);

  store.setPompelmiVerdict("job-1", "clean", "ok");
  const after = store.get("job-1");
  assert.equal(after?.pompelmi_verdict, "clean");
});
```

- [ ] **Step 2: Run test, see it fail**

```bash
yarn test
```

Expected: FAIL — `setPompelmiVerdict` is not a function or `pompelmi_verdict` is undefined.

- [ ] **Step 3: Add column, setter, and update Job select**

In `src/job-store.ts`:

(a) After `CREATE TABLE IF NOT EXISTS jobs (...)`, add an idempotent migration block right where the table is created (in the constructor, after the table create statement):

```ts
// Idempotent column add for pompelmi_verdict
try {
  this.db.exec("ALTER TABLE jobs ADD COLUMN pompelmi_verdict TEXT");
} catch (e) {
  // Already exists — sqlite throws "duplicate column name"
  if (!String(e).includes("duplicate column name")) throw e;
}
```

(b) In the `Job` interface (currently exports type `Job` near top), add:

```ts
pompelmi_verdict: "clean" | "malicious" | "error" | null;
```

(c) Update every `SELECT` that returns a `Job` to include `pompelmi_verdict`:

```sql
SELECT id, source_path, original_name, quarantine_path, final_path, status, vt_verdict, pompelmi_verdict, detail, created_at, updated_at
FROM jobs ...
```

(There are three: `get`, `listRecent`, the inconclusive sweep. Update all three.)

(d) Update `INSERT INTO jobs (...)` column list and bind sites to include `pompelmi_verdict` with default `NULL`:

```ts
this.db
  .prepare(
    `INSERT INTO jobs (id, source_path, original_name, quarantine_path, final_path, status, vt_verdict, pompelmi_verdict, detail, created_at, updated_at)
     VALUES (?, ?, ?, ?, ?, 'received', NULL, NULL, NULL, ?, ?)`
  )
  .run(jobId, sourcePath, originalName, null, null, now, now);
```

(e) Add the setter:

```ts
setPompelmiVerdict(
  jobId: string,
  verdict: "clean" | "malicious" | "error",
  detail?: string,
): void {
  const now = new Date().toISOString();
  this.db
    .prepare(
      `UPDATE jobs SET pompelmi_verdict = ?, detail = COALESCE(?, detail), updated_at = ? WHERE id = ?`,
    )
    .run(verdict, detail ?? null, now, jobId);
}
```

- [ ] **Step 4: Run tests, see them pass**

```bash
yarn test
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/job-store.ts src/job-store.test.ts
git commit -m "feat(job-store): add pompelmi_verdict column and setter"
```

---

## Task 4: Config knobs for pompelmi

**Files:**

- Modify: `src/config.ts`

- [ ] **Step 1: Add fields to `RawConfig` interface**

In `src/config.ts`, extend `RawConfig`:

```ts
/** Run pompelmi/ClamAV before VirusTotal. Defaults to true. */
pompelmiEnabled?: boolean;
/** UNIX socket path for clamd. Defaults to /tmp/clamd.sock. */
pompelmiSocketPath?: string;
/** What to do when pompelmi returns ScanError. Defaults to "bypass". */
pompelmiFailureMode?: "bypass" | "inconclusive";
```

- [ ] **Step 2: Add to exported `config` object**

In the same file, append to the `export const config = { ... }` literal:

```ts
pompelmiEnabled: file.pompelmiEnabled ?? envBool("POMPELMI_ENABLED", true),
pompelmiSocketPath:
  file.pompelmiSocketPath ?? process.env.POMPELMI_SOCKET ?? "/tmp/clamd.sock",
pompelmiFailureMode: ((): "bypass" | "inconclusive" => {
  const v = (file.pompelmiFailureMode ?? process.env.POMPELMI_FAILURE_MODE ?? "bypass").trim().toLowerCase();
  return v === "inconclusive" ? "inconclusive" : "bypass";
})(),
```

- [ ] **Step 3: Run existing tests to ensure nothing broke**

```bash
yarn test
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/config.ts
git commit -m "feat(config): add pompelmi knobs (enabled, socket, failure mode)"
```

---

## Task 5: `LocalScanner` module

**Files:**

- Create: `src/local-scanner.ts`
- Create: `src/local-scanner.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/local-scanner.test.ts`:

```ts
import { test } from "node:test";
import assert from "node:assert/strict";
import { LocalScanner, type LocalScanResult } from "./local-scanner.ts";

test("probe rejects when socket missing", async () => {
  await assert.rejects(
    () => LocalScanner.probe("/tmp/definitely-not-a-real-socket-987654.sock"),
    /unreachable|ENOENT|connect/i,
  );
});

test("verdict translation maps pompelmi result to LocalScanResult", async () => {
  // Build a scanner that bypasses pompelmi via a fake check function for testing.
  const fakeRunner = async (_path: string) => "MALICIOUS";
  const scanner = LocalScanner.fromFakeRunner(fakeRunner);
  const r: LocalScanResult = await scanner.check("/tmp/x");
  assert.equal(r.verdict, "malicious");
});
```

- [ ] **Step 2: Run test to see it fail**

```bash
yarn test
```

Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement `src/local-scanner.ts`**

```ts
import { promises as fs } from "fs";
import { createScanner, Verdict } from "pompelmi";

export type LocalVerdict = "clean" | "malicious" | "error";

export interface LocalScanResult {
  verdict: LocalVerdict;
  message: string;
}

export interface LocalScannerOptions {
  socketPath: string;
}

type RawRunner = (filePath: string, signal?: AbortSignal) => Promise<string>;

function classify(raw: unknown): LocalVerdict {
  if (raw === Verdict.Clean) return "clean";
  if (raw === Verdict.Malicious) return "malicious";
  if (raw === Verdict.ScanError) return "error";
  // Symbols don't compare as strings; also accept stringified for testing seam.
  if (typeof raw === "string") {
    const v = raw.toLowerCase();
    if (v === "clean") return "clean";
    if (v === "malicious") return "malicious";
  }
  return "error";
}

export class LocalScanner {
  private readonly scanner: { scan: RawRunner };

  constructor(opts: LocalScannerOptions) {
    this.scanner = createScanner({
      clamd: { socket: opts.socketPath },
    }) as unknown as { scan: RawRunner };
  }

  /** Test seam — bypass real pompelmi for unit tests. */
  static fromFakeRunner(run: RawRunner): LocalScanner {
    const inst = Object.create(LocalScanner.prototype) as LocalScanner;
    (inst as unknown as { scanner: { scan: RawRunner } }).scanner = { scan: run };
    return inst;
  }

  static async probe(socketPath: string): Promise<void> {
    try {
      await fs.access(socketPath);
    } catch (e) {
      throw new Error(
        `clamd socket unreachable at ${socketPath}: ${(e as Error).message}`,
      );
    }
  }

  async check(filePath: string, signal?: AbortSignal): Promise<LocalScanResult> {
    try {
      const raw = await this.scanner.scan(filePath, signal);
      const verdict = classify(raw);
      return {
        verdict,
        message:
          verdict === "error"
            ? `pompelmi ScanError on ${filePath}`
            : `pompelmi ${verdict}`,
      };
    } catch (e) {
      return {
        verdict: "error",
        message: `pompelmi exception: ${(e as Error).message}`,
      };
    }
  }
}
```

- [ ] **Step 4: Run tests, see them pass**

```bash
yarn test
```

Expected: PASS for both `local-scanner.test.ts` cases.

- [ ] **Step 5: Commit**

```bash
git add src/local-scanner.ts src/local-scanner.test.ts
git commit -m "feat(local-scanner): pompelmi-backed LocalScanner with probe + test seam"
```

---

## Task 6: Wire `LocalScanner` into the watcher pipeline

**Files:**

- Modify: `src/watcher.ts`
- Modify: `src/index.ts`

- [ ] **Step 1: Extend `WatcherOptions` with the local scanner and failure mode**

In `src/watcher.ts`, near the existing `WatcherOptions` interface:

```ts
import type { LocalScanner } from "./local-scanner.ts";

export interface WatcherOptions {
  watchRecursive?: boolean;
  maxScanBytes?: number;
  maxConcurrentScans?: number;
  useSeparateVtProcess?: boolean;
  /** When provided AND pompelmiEnabled, runs upstream of VT. */
  localScanner?: LocalScanner | null;
  /** Behavior on pompelmi ScanError. */
  pompelmiFailureMode?: "bypass" | "inconclusive";
}
```

Add private fields to `Watcher` and read them from opts in the constructor:

```ts
private readonly localScanner: LocalScanner | null;
private readonly pompelmiFailureMode: "bypass" | "inconclusive";
// in constructor:
this.localScanner = opts?.localScanner ?? null;
this.pompelmiFailureMode = opts?.pompelmiFailureMode ?? "bypass";
```

- [ ] **Step 2: Insert the local-scan stage in `handleFile`**

In `handleFile`, find the section right after `this.jobStore?.setScanning(jobId);` and before the `oversized` check.

Insert:

```ts
// Local pompelmi stage (defense-in-depth)
if (this.localScanner) {
  const localController = new AbortController();
  this.scanControllers.set(jobId, localController);
  let local;
  try {
    local = await this.localScanner.check(quarantineFilePath, localController.signal);
  } finally {
    this.scanControllers.delete(jobId);
  }
  this.jobStore?.setPompelmiVerdict(jobId, local.verdict, local.message);

  if (local.verdict === "malicious") {
    this.jobStore?.setScanResult(jobId, {
      verdict: "infected",
      message: `Local scanner: ${local.message}`,
    });
    console.log(`pompelmi infected — kept in quarantine: ${quarantineFilePath}`);
    return;
  }

  if (local.verdict === "error") {
    if (this.pompelmiFailureMode === "inconclusive") {
      this.jobStore?.setScanResult(jobId, {
        verdict: "inconclusive",
        message: `Local scanner failed: ${local.message}`,
      });
      console.warn(`pompelmi error (inconclusive mode): ${local.message}`);
      return;
    }
    console.warn(`pompelmi error (bypass): ${local.message}`);
    // Fall through to VT.
  }
  // verdict === 'clean' or bypassed error → fall through to VT
}
```

- [ ] **Step 3: Construct `LocalScanner` in `src/index.ts`**

In `src/index.ts`, near the top after `config` is loaded and before `Watcher` is constructed:

```ts
import { LocalScanner } from "./local-scanner.ts";

let localScanner: LocalScanner | null = null;
if (config.pompelmiEnabled) {
  try {
    await LocalScanner.probe(config.pompelmiSocketPath);
    localScanner = new LocalScanner({ socketPath: config.pompelmiSocketPath });
    console.log(`[pompelmi] enabled, socket=${config.pompelmiSocketPath}`);
  } catch (e) {
    console.error(
      `[pompelmi] enabled but probe failed (${(e as Error).message}). Refusing to start. Disable with pompelmiEnabled=false or fix clamd.`,
    );
    process.exit(1);
  }
} else {
  console.log("[pompelmi] disabled by config");
}
```

Pass it into the watcher options:

```ts
new Watcher(watchPath, ignored, quarantinePath, vtApiKey, jobStore, {
  // ... existing options ...
  localScanner,
  pompelmiFailureMode: config.pompelmiFailureMode,
});
```

- [ ] **Step 4: Smoke test (manual)**

This step requires ClamAV installed (see README task). Skip if not on a dev box with clamd running.

1. Drop the EICAR test string into the watch folder:

```bash
echo 'X5O!P%@AP[4\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*' > "$WATCH_PATH/eicar.com"
```

2. Watch logs: expect `pompelmi infected — kept in quarantine`. The file is in the quarantine folder; no VT request was sent.
3. Inspect DB:

```bash
sqlite3 ./data/jobs.sqlite "select id, status, pompelmi_verdict, vt_verdict from jobs order by created_at desc limit 1"
```

Expected: `pompelmi_verdict='malicious'`, `vt_verdict=NULL`, `status='quarantine_kept'`.

- [ ] **Step 5: Commit**

```bash
git add src/watcher.ts src/index.ts
git commit -m "feat(watcher): pompelmi local-scan stage upstream of VT"
```

---

## Task 7: Surface `pompelmi_verdict` and clamd reachability in HTTP responses

**Files:**

- Modify: `src/ui-server.ts`

- [ ] **Step 1: Confirm `/api/jobs` already returns full Job rows**

Open `src/ui-server.ts`. The handler returns `store.listRecent(200)`. Since `Job` now contains `pompelmi_verdict`, the field is already in the response. Verify by reading the handler.

No code change needed for `/api/jobs`.

- [ ] **Step 2: Add `localScanner` block to `/api/health`**

In the `/api/health` handler, add:

```ts
const localScannerInfo = await (async () => {
  if (!config.pompelmiEnabled) return { enabled: false, socketReachable: false };
  try {
    const { LocalScanner } = await import("./local-scanner.ts");
    await LocalScanner.probe(config.pompelmiSocketPath);
    return { enabled: true, socketReachable: true };
  } catch {
    return { enabled: true, socketReachable: false };
  }
})();
```

Then include `localScanner: localScannerInfo` in the health response object.

- [ ] **Step 3: Manual test**

```bash
curl -s http://127.0.0.1:3847/api/health | jq .
```

Expected: response includes `"localScanner": { "enabled": true, "socketReachable": true }` (or false in either field).

- [ ] **Step 4: Commit**

```bash
git add src/ui-server.ts
git commit -m "feat(api): expose localScanner status in /api/health"
```

---

## Task 8: Document ClamAV setup in README

**Files:**

- Modify: `README.md`

- [ ] **Step 1: Append a "Local scanner (pompelmi/ClamAV)" section**

Append to `README.md`:

```markdown
## Local scanner (pompelmi / ClamAV)

By default the daemon runs a local ClamAV scan on every quarantined file before sending it to VirusTotal. Install ClamAV on the host:

    brew install clamav
    freshclam
    # Edit /opt/homebrew/etc/clamav/clamd.conf:
    #   uncomment LocalSocket /tmp/clamd.sock
    #   set MaxFileSize 4000M
    #   set MaxScanSize 4000M
    #   set StreamMaxLength 4000M
    brew services start clamav

To disable the local scanner, set `pompelmiEnabled: false` in `config.json`. To require strict failure handling instead of falling back to VT on local-scan errors, set `pompelmiFailureMode: "inconclusive"`.

The daemon refuses to start if `pompelmiEnabled=true` and the configured socket is unreachable. Check `/api/health` for `localScanner.socketReachable`.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: ClamAV setup for the local scanner"
```

---

## Task 9: End-to-end manual verification

**No file changes. This is a verification gate.**

- [ ] **Step 1: Drop EICAR — expect `infected`, no VT call**

```bash
echo 'X5O!P%@AP[4\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*' > "$WATCH_PATH/eicar.com"
sqlite3 ./data/jobs.sqlite "select pompelmi_verdict, vt_verdict, status from jobs order by created_at desc limit 1"
```

Expected: `malicious|<null>|quarantine_kept`.

- [ ] **Step 2: Drop a known-clean file — expect both verdicts populated, restored**

```bash
echo "harmless content $RANDOM" > "$WATCH_PATH/clean-$(date +%s).txt"
sleep 30  # allow VT to settle
sqlite3 ./data/jobs.sqlite "select pompelmi_verdict, vt_verdict, status from jobs order by created_at desc limit 1"
```

Expected: `clean|clean|restored` (or `clean|<null>|restored` if `vtEnabled=false`).

- [ ] **Step 3: Stop clamd, set failure mode `inconclusive`, drop a file — expect `inconclusive`**

```bash
brew services stop clamav
# in config.json set pompelmiFailureMode: "inconclusive", restart daemon
echo "test" > "$WATCH_PATH/no-scanner.txt"
sqlite3 ./data/jobs.sqlite "select pompelmi_verdict, vt_verdict, status from jobs order by created_at desc limit 1"
```

Expected: `error|<null>|quarantine_kept`. The daemon should have refused to start; if it started it means `pompelmiEnabled` was off — flip back on and confirm refusal.

- [ ] **Step 4: Restart clamd, set failure mode `bypass`, drop a file — expect VT runs**

```bash
brew services start clamav
# config.json pompelmiFailureMode: "bypass"
echo "test bypass" > "$WATCH_PATH/bypass.txt"
```

Expected: pompelmi runs, returns clean (file is benign), VT runs, file restored.

---

## Self-Review Checklist (already run)

- Spec coverage: every config field, schema column, pipeline branch, API change, prereq, and failure mode in the spec is mapped to a task above.
- No placeholders: every code step shows actual code.
- Type consistency: `LocalVerdict`, `LocalScanResult`, `setPompelmiVerdict` named consistently across tasks.
- Tasks 1-8 each end in a commit; Task 9 is a verification gate that does not change code.
