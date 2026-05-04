import Watcher from "./watcher.ts";
import { JobStore } from "./job-store.ts";
import { startUiServer } from "./ui-server.ts";
import { startLaunchAgentMonitor } from "./launch-agent-monitor.ts";
import { config, writeConfig } from "./config.ts";
import FileMover from "./file-mover.ts";
import { assertSafeHttpHost } from "./http-host-guard.ts";
import { startInconclusiveSweeper } from "./inconclusive-sweeper.ts";
import { LocalScanner } from "./local-scanner.ts";
import { TartCli } from "./sandbox-tart.ts";
import { SandboxStore } from "./sandbox-store.ts";
import { SandboxManager } from "./sandbox-manager.ts";

if (!config.vtApiKey)
  throw new Error("vtApiKey not set (config.json or VT_API_KEY)");
if (!config.watchPath)
  throw new Error("watchPath not set (config.json or WATCH_PATH)");
if (!config.quarantinePath)
  throw new Error("quarantinePath not set (config.json or QUARANTINE_PATH)");

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

const jobStore = new JobStore(config.databasePath);
const fileMover = new FileMover(config.quarantinePath);

let sandboxManager: SandboxManager | null = null;
let sandboxProbe = { tartInstalled: false, baseImagePresent: false };
if (config.sandboxEnabled) {
  const tart = new TartCli();
  sandboxProbe = await SandboxManager.probe(tart, config.sandboxBaseVm);
  if (!sandboxProbe.tartInstalled) {
    console.warn("[sandbox] enabled but `tart` not in PATH - sandbox endpoints will return 503.");
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
      probeInfo: sandboxProbe,
    });
    await sandboxManager.init();
    console.log("[sandbox] manager initialised");
  }
}

const watcher = new Watcher(
  config.watchPath,
  [".DS_Store"],
  config.quarantinePath,
  config.vtApiKey,
  jobStore,
  {
    watchRecursive: config.watchRecursive,
    maxScanBytes: config.maxScanBytes,
    maxConcurrentScans: config.maxConcurrentScans,
    useSeparateVtProcess: config.useSeparateVtProcess,
    localScanner,
    pompelmiFailureMode: config.pompelmiFailureMode,
    initialMode: config.watcherMode,
    vtEnabled: config.vtEnabled,
    onModeChange: (m) => {
      try {
        writeConfig({ watcherMode: m });
      } catch (e) {
        console.error(`[config] failed to persist mode: ${(e as Error).message}`);
      }
    },
  },
);
watcher.start();
startLaunchAgentMonitor();

async function deleteQuarantineJob(jobId: string, detail?: string) {
  const job = jobStore.getJob(jobId);
  if (!job) throw new Error(`Job ${jobId} not found`);
  if (job.status !== "quarantine_kept")
    throw new Error(`Job ${jobId} is not in quarantine_kept status`);
  if (!job.quarantine_path)
    throw new Error(`Job ${jobId} has no quarantine path`);
  await fileMover.deleteFile(job.quarantine_path);
  jobStore.setDeleted(jobId, detail ?? "Deleted by user");
}

async function restoreQuarantineJob(jobId: string) {
  const job = jobStore.getJob(jobId);
  if (!job) throw new Error(`Job ${jobId} not found`);
  if (job.status !== "quarantine_kept")
    throw new Error(`Job ${jobId} is not in quarantine_kept status`);
  if (!job.quarantine_path)
    throw new Error(`Job ${jobId} has no quarantine path`);
  const destPath = await fileMover.resolveRestoreDestination(
    config.watchPath,
    job.original_name,
  );
  watcher.markRestoring(destPath);
  const { restoredPath } = await fileMover.restoreToWatch(
    config.watchPath,
    job.quarantine_path,
    job.original_name,
  );
  jobStore.setRestored(jobId, restoredPath);
}

if (config.httpPort !== undefined) {
  const bindHost = process.env.HTTP_HOST ?? config.httpHost ?? "127.0.0.1";
  assertSafeHttpHost(bindHost);
  const watcherControl = {
    getMode: () => watcher.getMode(),
    setMode: (m) => watcher.setMode(m),
    pause: () => watcher.pause(),
    resume: () => watcher.resume(),
    isPaused: () => watcher.isPaused,
  };
  startUiServer(
    jobStore,
    config.httpPort,
    (id) => watcher.cancel(id),
    deleteQuarantineJob,
    restoreQuarantineJob,
    watcherControl,
    sandboxManager ?? undefined,
  );
}

if (config.inconclusiveRetentionDays > 0) {
  startInconclusiveSweeper(
    config.inconclusiveRetentionDays,
    jobStore,
    async (id) => {
      await deleteQuarantineJob(
        id,
        `Auto-deleted after ${config.inconclusiveRetentionDays} day(s) (inconclusive)`,
      );
    },
  );
}

async function shutdown() {
  try { await sandboxManager?.shutdownAll(); } catch { /* ignore */ }
  jobStore.close();
  process.exit(0);
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
