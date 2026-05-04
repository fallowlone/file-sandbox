import { readFileSync, writeFileSync, existsSync } from "fs";
import { join } from "path";
import {
  decryptConfigJson,
  encryptConfigJson,
  isEncryptedConfigPayload,
} from "./config-crypto.ts";
import { parseMode, type WatcherMode } from "./watcher-mode.ts";

export interface RawConfig {
  vtApiKey?: string;
  watchPath?: string;
  quarantinePath?: string;
  databasePath?: string;
  httpPort?: number;
  httpHost?: string;
  apiToken?: string;
  /** When false, only direct children of watchPath are observed (chokidar depth 0). */
  watchRecursive?: boolean;
  /** Skip VT upload when file size exceeds this (bytes). Default 400 MiB. */
  maxScanBytes?: number;
  /** Max parallel VirusTotal pipelines. Default 2. */
  maxConcurrentScans?: number;
  /** Run VT upload/analysis in a child Node process (same machine user). */
  useSeparateVtProcess?: boolean;
  /** Delete inconclusive quarantine_kept jobs older than this many days (0 = off). */
  inconclusiveRetentionDays?: number;
  /** Run pompelmi/ClamAV before VirusTotal. Defaults to true. */
  pompelmiEnabled?: boolean;
  /** UNIX socket path for clamd. Defaults to /tmp/clamd.sock. */
  pompelmiSocketPath?: string;
  /** What to do when pompelmi returns ScanError. Defaults to "bypass". */
  pompelmiFailureMode?: "bypass" | "inconclusive";
  watcherMode?: WatcherMode | string;
  /** Run VirusTotal scan stage. Defaults to true. */
  vtEnabled?: boolean;
  sandboxEnabled?: boolean;
  sandboxBaseVm?: string;
  sandboxIdleTimeoutMinutes?: number;
  sandboxNetworkDefault?: boolean;
  sandboxSessionsDir?: string;
  sandboxOutRetentionDays?: number;
}

const configPath = join(process.cwd(), "config.json");

function masterKeyFromEnv(): string | undefined {
  const k = process.env.FILESANDBOX_MASTER_KEY?.trim();
  return k || undefined;
}

function readConfigFileRaw(): string {
  if (!existsSync(configPath)) return "{}";
  return readFileSync(configPath, "utf8");
}

function parseConfigJson(json: string): RawConfig {
  try {
    return JSON.parse(json) as RawConfig;
  } catch (err) {
    console.error(
      `[config] config.json is malformed — using defaults. Error: ${err instanceof Error ? err.message : String(err)}`,
    );
    return {};
  }
}

function loadFile(): RawConfig {
  let raw = readConfigFileRaw();
  const mk = masterKeyFromEnv();
  if (mk && isEncryptedConfigPayload(raw)) {
    try {
      raw = decryptConfigJson(raw.trim(), mk);
    } catch (e) {
      throw new Error(
        `Failed to decrypt config.json (check FILESANDBOX_MASTER_KEY): ${e}`,
      );
    }
  } else if (isEncryptedConfigPayload(raw) && !mk) {
    throw new Error(
      "config.json is encrypted; set FILESANDBOX_MASTER_KEY to decrypt.",
    );
  }
  return parseConfigJson(raw);
}

const file = loadFile();

function get(fileVal: string | undefined, envVal: string | undefined): string {
  return fileVal ?? envVal ?? "";
}

function envInt(name: string, fallback: number): number {
  const v = process.env[name];
  if (v === undefined || v === "") return fallback;
  const n = Number(v);
  return Number.isFinite(n) ? n : fallback;
}

function envBool(name: string, fallback: boolean): boolean {
  const v = process.env[name]?.trim().toLowerCase();
  if (v === undefined || v === "") return fallback;
  if (["1", "true", "yes", "on"].includes(v)) return true;
  if (["0", "false", "no", "off"].includes(v)) return false;
  return fallback;
}

const defaultMaxScan = 400 * 1024 * 1024;

export function writeConfig(updates: Partial<RawConfig>): void {
  let existing: RawConfig = {};
  const mk = masterKeyFromEnv();
  let rawOnDisk = readConfigFileRaw();
  let jsonBody: string;
  if (mk && isEncryptedConfigPayload(rawOnDisk)) {
    try {
      jsonBody = decryptConfigJson(rawOnDisk.trim(), mk);
      existing = parseConfigJson(jsonBody);
    } catch {
      existing = {};
      jsonBody = "{}";
    }
  } else if (isEncryptedConfigPayload(rawOnDisk) && !mk) {
    throw new Error(
      "Cannot write config: file encrypted but FILESANDBOX_MASTER_KEY unset",
    );
  } else {
    try {
      existing = JSON.parse(rawOnDisk || "{}") as RawConfig;
    } catch {
      existing = {};
    }
    jsonBody = JSON.stringify(existing, null, 2);
  }
  const merged = { ...existing, ...updates };
  const out = JSON.stringify(merged, null, 2);
  if (mk) {
    writeFileSync(configPath, encryptConfigJson(out, mk), "utf8");
  } else {
    writeFileSync(configPath, out, "utf8");
  }
}

/** Mask for API responses; do not send full key to clients. */
export function maskSecret(value: string, visibleTail = 4): string {
  if (!value) return "";
  if (value.length <= visibleTail) return "****";
  return `****${value.slice(-visibleTail)}`;
}

export const config = {
  vtApiKey: get(file.vtApiKey, process.env.VT_API_KEY),
  apiToken: get(file.apiToken, process.env.FILESANDBOX_API_TOKEN),
  watchPath: get(file.watchPath, process.env.WATCH_PATH),
  quarantinePath: get(file.quarantinePath, process.env.QUARANTINE_PATH),
  databasePath:
    file.databasePath ?? process.env.DATABASE_PATH ?? "./data/jobs.sqlite",
  httpPort: (() => {
    const raw = file.httpPort ?? process.env.HTTP_PORT;
    if (raw === undefined || raw === "") return undefined;
    const n = Number(raw);
    if (!Number.isFinite(n) || n < 1 || n > 65535)
      throw new Error("httpPort must be 1–65535");
    return n;
  })(),
  httpHost: file.httpHost ?? process.env.HTTP_HOST ?? "127.0.0.1",
  watchRecursive: file.watchRecursive ?? envBool("WATCH_RECURSIVE", true),
  maxScanBytes: file.maxScanBytes ?? envInt("MAX_SCAN_BYTES", defaultMaxScan),
  maxConcurrentScans: Math.max(
    1,
    file.maxConcurrentScans ?? envInt("MAX_CONCURRENT_SCANS", 2),
  ),
  useSeparateVtProcess:
    file.useSeparateVtProcess ?? envBool("USE_SEPARATE_VT_PROCESS", false),
  inconclusiveRetentionDays:
    file.inconclusiveRetentionDays ?? envInt("INCONCLUSIVE_RETENTION_DAYS", 0),
  pompelmiEnabled: file.pompelmiEnabled ?? envBool("POMPELMI_ENABLED", true),
  pompelmiSocketPath:
    file.pompelmiSocketPath ?? process.env.POMPELMI_SOCKET ?? "/tmp/clamd.sock",
  pompelmiFailureMode: ((): "bypass" | "inconclusive" => {
    const v = (file.pompelmiFailureMode ?? process.env.POMPELMI_FAILURE_MODE ?? "bypass").trim().toLowerCase();
    return v === "inconclusive" ? "inconclusive" : "bypass";
  })(),
  watcherMode: parseMode(file.watcherMode ?? process.env.WATCHER_MODE),
  vtEnabled: file.vtEnabled ?? envBool("VT_ENABLED", true),
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
  configEncryptedAtRest: Boolean(masterKeyFromEnv()),
};
