import Watcher from "./watcher.ts";
import { JobStore } from "./job-store.ts";
import { startUiServer } from "./ui-server.ts";
import { startLaunchAgentMonitor } from "./launch-agent-monitor.ts";
import { config, writeConfig } from "./config.ts";
import FileMover from "./file-mover.ts";
import { assertSafeHttpHost } from "./http-host-guard.ts";
import { startInconclusiveSweeper } from "./inconclusive-sweeper.ts";
import { LocalScanner } from "./local-scanner.ts";

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
  jobStore.close();
  process.exit(0);
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
