import { watch } from "chokidar";
import { watch as fsWatch, chmod as fsChmod } from "fs";
import { platform } from "node:os";
import { cacheCheck, cacheStore } from "./vt-cache.ts";
import { chmod as chmodAsync, stat as statAsync } from "fs/promises";
import { execFile } from "child_process";
import { basename, join } from "path";
import { randomUUID } from "crypto";
import FileMover from "./file-mover.ts";
import FilePermissions from "./file-permissions.ts";
import VirusChecker from "./virus-checker.ts";
import type { JobStore } from "./job-store.ts";
import type { LocalScanner } from "./local-scanner.ts";
import { Semaphore } from "./semaphore.ts";
import { metrics } from "./metrics.ts";

function setQuarantineXattr(filePath: string): Promise<void> {
  return new Promise((resolve) => {
    execFile(
      "xattr",
      ["-w", "com.apple.quarantine", "0083;00000000;FileSandbox;", filePath],
      (err) => {
        if (err) {
          console.warn(
            `[xattr] Failed to set quarantine xattr on ${filePath}: ${err.message}`,
          );
        }
        resolve();
      },
    );
  });
}

function clearAllXattrs(filePath: string): Promise<void> {
  return new Promise((resolve) => {
    execFile("xattr", ["-c", filePath], (err) => {
      if (err) {
        console.warn(
          `[xattr] Failed to clear xattrs on ${filePath}: ${err.message}`,
        );
      }
      resolve();
    });
  });
}

const BROWSER_TEMP_EXTENSIONS = [
  ".crdownload",
  ".download",
  ".part",
  ".opdownload",
  ".tmp",
];

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

class Watcher {
  private readonly watchPath: string;
  private readonly ignored: string[];
  private filePermissions: FilePermissions;
  private fileMover: FileMover;
  private quarantinePath: string;
  private virusChecker: VirusChecker;
  private readonly jobStore?: JobStore;
  private readonly restoringPaths = new Set<string>();
  private readonly processingPaths = new Set<string>();
  private readonly scanControllers = new Map<string, AbortController>();
  private readonly watchRecursive: boolean;
  private readonly maxScanBytes: number;
  private readonly scanSemaphore: Semaphore;
  private paused = false;
  private readonly localScanner: LocalScanner | null;
  private readonly pompelmiFailureMode: "bypass" | "inconclusive";

  constructor(
    watchPath: string,
    ignored: string[],
    quarantinePath: string,
    apiKey: string,
    jobStore?: JobStore,
    opts?: WatcherOptions,
  ) {
    this.quarantinePath = quarantinePath;
    this.watchPath = watchPath;
    this.ignored = ignored;
    this.watchRecursive = opts?.watchRecursive ?? true;
    this.maxScanBytes = opts?.maxScanBytes ?? 400 * 1024 * 1024;
    const concurrent = Math.max(1, opts?.maxConcurrentScans ?? 2);
    this.scanSemaphore = new Semaphore(concurrent);

    this.fileMover = new FileMover(this.quarantinePath);
    this.virusChecker = new VirusChecker(apiKey, {
      maxScanBytes: this.maxScanBytes,
      useSeparateVtProcess: opts?.useSeparateVtProcess ?? false,
    });
    this.filePermissions = new FilePermissions();
    this.jobStore = jobStore;
    this.localScanner = opts?.localScanner ?? null;
    this.pompelmiFailureMode = opts?.pompelmiFailureMode ?? "bypass";
  }

  /** Skip re-scan when restoring from API or clean pipeline. */
  markRestoring(destPath: string) {
    this.restoringPaths.add(destPath);
  }

  pause() {
    this.paused = true;
  }

  resume() {
    this.paused = false;
  }

  get isPaused(): boolean {
    return this.paused;
  }

  cancel(jobId: string) {
    const controller = this.scanControllers.get(jobId);
    if (controller) {
      controller.abort();
      this.scanControllers.delete(jobId);
    }
  }

  start() {
    const stabilityThreshold = Number(process.env.WATCH_STABILITY_MS) || 2000;
    const pollInterval = Number(process.env.WATCH_POLL_MS) || 100;

    const isBrowserTemp = (filename: string) =>
      BROWSER_TEMP_EXTENSIONS.some((ext) => filename.endsWith(ext));

    const pl = platform();
    const rawRecursive =
      this.watchRecursive && (pl === "darwin" || pl === "win32");

    const rawFsWatcher = fsWatch(
      this.watchPath,
      { recursive: rawRecursive },
      (event, filename) => {
        if (this.paused) return;
        if (!filename || event !== "rename") return;
        if ((this.ignored as string[]).some((ign) => filename.endsWith(ign)))
          return;
        if (isBrowserTemp(filename)) return;
        const fullPath = join(this.watchPath, filename);
        if (this.restoringPaths.has(fullPath)) return;
        if (this.processingPaths.has(fullPath)) return;
        // Skip directories — only lock actual files
        try {
          const st = require("fs").statSync(fullPath);
          if (st.isDirectory()) return;
        } catch {
          return; // file doesn't exist yet
        }
        fsChmod(fullPath, 0o000, () => {});
        setQuarantineXattr(fullPath).catch(() => {});
      },
    );

    const watcher = watch(this.watchPath, {
      ignoreInitial: true,
      ignorePermissionErrors: true,
      ...(this.watchRecursive ? {} : { depth: 0 }),
      ignored: (f: string) => this.ignored.some((ign) => f.endsWith(ign)),
      awaitWriteFinish: {
        stabilityThreshold,
        pollInterval,
      },
    });

    const handleFile = async (filepath: string) => {
      if (this.paused) return;
      if (isBrowserTemp(filepath)) return;

      const fname = basename(filepath);
      if (this.ignored.some((ign) => fname === ign)) return;

      // Skip directories
      try {
        const st = await statAsync(filepath);
        if (st.isDirectory()) return;
      } catch {
        return;
      }

      if (this.restoringPaths.has(filepath)) {
        this.restoringPaths.delete(filepath);
        return;
      }

      // Dedupe "add" + "change" events: chokidar fires both when write completes.
      if (this.processingPaths.has(filepath)) {
        return;
      }

      this.processingPaths.add(filepath);
      try {
        await setQuarantineXattr(filepath);

        const jobId = randomUUID();
        this.jobStore?.insertReceived(jobId, filepath, basename(filepath));

        try {
          await chmodAsync(filepath, 0o444).catch(() => {});

          const { quarantineFilePath, originalBaseName } =
            await this.fileMover.move(filepath);

          this.jobStore?.setInQuarantine(jobId, quarantineFilePath);

          await this.filePermissions.changePermissions(
            quarantineFilePath,
            0o444,
          );

          console.log(
            `Watching: path=${filepath} quarantine=${quarantineFilePath} originalName=${originalBaseName}`,
          );

          this.jobStore?.setScanning(jobId);

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

          const st = await statAsync(quarantineFilePath);
          if (st.size > this.maxScanBytes) {
            const result = {
              verdict: "oversized" as const,
              message: `File exceeds scan limit (${this.maxScanBytes} bytes); not sent to VirusTotal. Restore or delete from the UI.`,
            };
            this.jobStore?.setScanResult(jobId, result);
            console.log(
              `Oversized — kept in quarantine: ${quarantineFilePath}`,
            );
            return;
          }

          const cached = await cacheCheck(quarantineFilePath);
          if (cached) {
            console.log(`vt-cache hit: ${cached} (skipped upload)`);
            const result = {
              verdict: cached as import("./virus-checker.ts").VirusVerdict,
              message: `From local cache (SHA-256 match)`,
            };
            this.jobStore?.setScanResult(jobId, result);
            if (result.verdict === "clean") {
              const destPath = await this.fileMover.resolveRestoreDestination(
                this.watchPath,
                originalBaseName,
              );
              this.restoringPaths.add(destPath);
              const { restoredPath } = await this.fileMover.restoreToWatch(
                this.watchPath,
                quarantineFilePath,
                originalBaseName,
              );
              await chmodAsync(restoredPath, 0o644).catch((err) =>
                console.warn(
                  `[chmod] Failed to set 0o644 on ${restoredPath}: ${err.message}`,
                ),
              );
              await clearAllXattrs(restoredPath);
              this.jobStore?.setRestored(jobId, restoredPath);
            }
            return;
          }

          const controller = new AbortController();
          this.scanControllers.set(jobId, controller);

          await this.scanSemaphore.acquire();
          metrics.incScan();
          let result: import("./virus-checker.ts").VirusCheckResult;
          try {
            result = await this.virusChecker.check(
              quarantineFilePath,
              controller.signal,
            );
          } finally {
            metrics.decScan();
            this.scanSemaphore.release();
          }
          this.scanControllers.delete(jobId);

          if (
            result.verdict !== "inconclusive" &&
            result.verdict !== "oversized"
          ) {
            await cacheStore(quarantineFilePath, result.verdict);
          }
          console.log(`VirusTotal: ${result.verdict} — ${result.message}`);

          if (result.message === "Cancelled by user") {
            this.jobStore?.cancelJob(jobId);
            console.log(
              `Scan cancelled — keeping in quarantine: ${quarantineFilePath}`,
            );
            return;
          }

          this.jobStore?.setScanResult(jobId, result);

          if (result.verdict === "clean") {
            const destPath = await this.fileMover.resolveRestoreDestination(
              this.watchPath,
              originalBaseName,
            );
            this.restoringPaths.add(destPath);

            const { restoredPath } = await this.fileMover.restoreToWatch(
              this.watchPath,
              quarantineFilePath,
              originalBaseName,
            );
            await chmodAsync(restoredPath, 0o644).catch((err) =>
              console.warn(
                `[chmod] Failed to set 0o644 on ${restoredPath}: ${err.message}`,
              ),
            );
            await clearAllXattrs(restoredPath);
            this.jobStore?.setRestored(jobId, restoredPath);
          } else {
            console.log(
              `Keeping in quarantine (${result.verdict}): ${quarantineFilePath}`,
            );
          }
        } catch (error) {
          metrics.setLastError(String(error));
          console.log(`Failed processing ${filepath}: ${error}`);
          this.jobStore?.fail(jobId, String(error));
        }
      } finally {
        this.processingPaths.delete(filepath);
      }
    };

    watcher.on("add", handleFile);
    watcher.on("change", handleFile);
    watcher.on("error", (error) => {
      console.log(`Watcher error: ${error}`);
    });

    return { watcher, rawFsWatcher };
  }
}

export default Watcher;
