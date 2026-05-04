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
  probeInfo?: { tartInstalled: boolean; baseImagePresent: boolean };
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
  private readonly probeInfo: { tartInstalled: boolean; baseImagePresent: boolean };
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
    this.probeInfo = opts.probeInfo ?? { tartInstalled: true, baseImagePresent: true };
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

  getProbe(): { tartInstalled: boolean; baseImagePresent: boolean } {
    return this.probeInfo;
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
}
