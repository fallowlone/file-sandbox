import fs, { type PathOrFileDescriptor } from "fs";
import { spawn } from "child_process";
import { dirname, join } from "path";
import { fileURLToPath } from "url";
import type { AnalysisResponse } from "./types/analysis.ts";

export type VirusVerdict = "clean" | "infected" | "inconclusive" | "oversized";

export interface VirusCheckResult {
  verdict: VirusVerdict;
  message: string;
  malicious?: number;
  suspicious?: number;
}

interface IVTUploadResponse {
  data?: {
    type: string;
    id: string;
  };
  error?: { code?: string; message?: string };
}

const apiUrl = "https://www.virustotal.com/api/v3";

const MAX_UPLOAD_ATTEMPTS = 4;

export interface UploadRetryContext {
  attempt: number;
  lastError: "network-error" | "http-error";
  httpStatus?: number;
}

/**
 * Decide whether to retry a failed upload.
 *
 * TODO(learning): implement real policy. Current default = no retry (legacy behavior).
 *
 * Considerations:
 *   - attempt is 1-based; returning false on attempt === maxAttempts stops retries
 *   - HTTP 4xx (except 429) = bad request, auth, invalid file → retry won't help
 *   - HTTP 429 = rate limit → worth retrying with longer backoff
 *   - HTTP 5xx = server error → transient, retry
 *   - network-error = connection reset / DNS / TLS → transient, retry
 */
function shouldRetryUpload(
  _ctx: UploadRetryContext,
  _maxAttempts: number,
): boolean {
  return false;
}

/**
 * Delay (ms) before next upload attempt.
 *
 * TODO(learning): implement real backoff. Current default = 0 (irrelevant until retries enabled).
 *
 * Considerations:
 *   - VT public API: 4 requests/min → base ≥ 15000ms on 429
 *   - Exponential (2^attempt * base) prevents hammering the service
 *   - Small jitter (±20%) avoids thundering-herd if many files scanned concurrently
 *   - Cap at e.g. 120_000 so worst-case wait stays bounded
 */
function backoffMsForAttempt(_attempt: number): number {
  return 0;
}

function sleepAbortable(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      return reject(new DOMException("Aborted", "AbortError"));
    }
    const onAbort = () => {
      clearTimeout(timer);
      reject(new DOMException("Aborted", "AbortError"));
    };
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", onAbort);
      resolve();
    }, ms);
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

export interface VirusCheckOptions {
  maxBytes: number;
}

/**
 * Core VT scan (same process). Respects maxBytes before reading whole file into RAM.
 */
export async function virusCheckFile(
  apiKey: string,
  path: PathOrFileDescriptor,
  signal: AbortSignal | undefined,
  opts: VirusCheckOptions,
): Promise<VirusCheckResult> {
  try {
    const st = fs.statSync(path);
    if (st.isFile() && st.size > opts.maxBytes) {
      return {
        verdict: "oversized",
        message: `File exceeds scan limit (${opts.maxBytes} bytes); not uploaded to VirusTotal. You can restore or delete from the UI.`,
      };
    }
  } catch {
    return {
      verdict: "inconclusive",
      message: "Failed to stat file before scan",
    };
  }

  let file: Buffer;
  try {
    file = fs.readFileSync(path);
  } catch {
    return {
      verdict: "inconclusive",
      message: "Failed to read file for upload",
    };
  }

  const formData = new FormData();
  formData.append("file", new Blob([file]));

  let request: Response | null = null;
  let lastFailure: VirusCheckResult | null = null;

  for (let attempt = 1; attempt <= MAX_UPLOAD_ATTEMPTS; attempt++) {
    let resp: Response;
    try {
      resp = await fetch(apiUrl + "/files", {
        method: "POST",
        headers: { "x-apikey": apiKey },
        body: formData,
        signal,
      });
    } catch (e) {
      if (e instanceof Error && e.name === "AbortError") {
        return { verdict: "inconclusive", message: "Cancelled by user" };
      }
      lastFailure = {
        verdict: "inconclusive",
        message: `Upload network error (attempt ${attempt}/${MAX_UPLOAD_ATTEMPTS}): ${e}`,
      };
      const retry = shouldRetryUpload(
        { attempt, lastError: "network-error" },
        MAX_UPLOAD_ATTEMPTS,
      );
      if (!retry) break;
      try {
        await sleepAbortable(backoffMsForAttempt(attempt), signal);
      } catch {
        return { verdict: "inconclusive", message: "Cancelled by user" };
      }
      continue;
    }

    if (resp.ok) {
      request = resp;
      break;
    }

    const body = await resp.text();
    lastFailure = {
      verdict: "inconclusive",
      message: `Upload failed HTTP ${resp.status} (attempt ${attempt}/${MAX_UPLOAD_ATTEMPTS}): ${body.slice(0, 500)}`,
    };
    const retry = shouldRetryUpload(
      { attempt, lastError: "http-error", httpStatus: resp.status },
      MAX_UPLOAD_ATTEMPTS,
    );
    if (!retry) break;
    try {
      await sleepAbortable(backoffMsForAttempt(attempt), signal);
    } catch {
      return { verdict: "inconclusive", message: "Cancelled by user" };
    }
  }

  if (!request) {
    return (
      lastFailure ?? {
        verdict: "inconclusive",
        message: "Upload failed with no details",
      }
    );
  }

  let uploadJson: IVTUploadResponse;
  try {
    uploadJson = (await request.json()) as IVTUploadResponse;
  } catch {
    return {
      verdict: "inconclusive",
      message: "Invalid JSON in upload response",
    };
  }

  if (uploadJson.error) {
    return {
      verdict: "inconclusive",
      message: `Upload API error: ${uploadJson.error.message ?? JSON.stringify(uploadJson.error)}`,
    };
  }

  const analysisId = uploadJson.data?.id;
  if (!analysisId) {
    return {
      verdict: "inconclusive",
      message: "No analysis id in upload response",
    };
  }

  const maxPolls = Number(process.env.VT_MAX_POLLS) || 20;
  const pollMs = Number(process.env.VT_POLL_INTERVAL_MS) || 15000;

  for (let i = 0; i < maxPolls; i++) {
    try {
      await sleepAbortable(pollMs, signal);
    } catch {
      return { verdict: "inconclusive", message: "Cancelled by user" };
    }

    let status: Response;
    try {
      status = await fetch(apiUrl + `/analyses/${analysisId}`, {
        method: "GET",
        headers: {
          "x-apikey": apiKey,
        },
        signal,
      });
    } catch (e) {
      if (e instanceof Error && e.name === "AbortError") {
        return { verdict: "inconclusive", message: "Cancelled by user" };
      }
      return {
        verdict: "inconclusive",
        message: `Analysis poll network error: ${e}`,
      };
    }

    if (!status.ok) {
      const body = await status.text();
      return {
        verdict: "inconclusive",
        message: `Analysis poll HTTP ${status.status}: ${body.slice(0, 500)}`,
      };
    }

    let parsed: AnalysisResponse;
    try {
      parsed = (await status.json()) as AnalysisResponse;
    } catch {
      return {
        verdict: "inconclusive",
        message: "Invalid JSON in analysis response",
      };
    }

    const { data } = parsed;
    const state = data.attributes.status;

    if (state === "queued" || state === "in-progress") {
      continue;
    }

    if (state === "completed") {
      const stats = data.attributes.stats;
      const malicious = stats.malicious ?? 0;
      const suspicious = stats.suspicious ?? 0;
      const harmless = stats.harmless ?? 0;
      const undetected = stats.undetected ?? 0;
      const total = malicious + harmless + undetected + suspicious;

      if (malicious > 0 || suspicious > 0) {
        return {
          verdict: "infected",
          message: `Threats: malicious=${malicious}, suspicious=${suspicious} (engines reporting: ${total})`,
          malicious,
          suspicious,
        };
      }

      return {
        verdict: "clean",
        message: `No malicious or suspicious flags (${total} engines with verdicts)`,
        malicious: 0,
        suspicious: 0,
      };
    }

    return {
      verdict: "inconclusive",
      message: `Unexpected analysis status: ${state}`,
    };
  }

  return {
    verdict: "inconclusive",
    message: `Polling timeout after ${maxPolls} attempts (${pollMs}ms interval)`,
  };
}

/**
 * Run VT scan in a fresh Node process (bytes read + network only in child).
 */
export function virusCheckInChildProcess(
  apiKey: string,
  filePath: string,
  signal: AbortSignal | undefined,
  maxBytes: number,
): Promise<VirusCheckResult> {
  return new Promise((resolve) => {
    const worker = join(
      dirname(fileURLToPath(import.meta.url)),
      "vt-worker.ts",
    );
    const child = spawn(process.execPath, [worker, filePath], {
      env: {
        ...process.env,
        VT_API_KEY: apiKey,
        MAX_SCAN_BYTES: String(maxBytes),
      },
      execArgv: [...process.execArgv],
      stdio: ["ignore", "pipe", "pipe"],
    });

    // Calculate timeout: max polling duration + 60s buffer
    const maxPolls = Number(process.env.VT_MAX_POLLS) || 20;
    const pollMs = Number(process.env.VT_POLL_INTERVAL_MS) || 15000;
    const timeoutMs = maxPolls * pollMs + 60_000;

    let settled = false;

    const timeoutId = setTimeout(() => {
      if (settled) return;
      settled = true;
      child.kill();
      resolve({
        verdict: "inconclusive",
        message: `VT child process timed out after ${timeoutMs}ms`,
      });
    }, timeoutMs);

    const onAbort = () => {
      if (settled) return;
      settled = true;
      child.kill("SIGTERM");
      clearTimeout(timeoutId);
      signal?.removeEventListener("abort", onAbort);
      resolve({
        verdict: "inconclusive",
        message: "Cancelled by user",
      });
    };
    signal?.addEventListener("abort", onAbort);

    let out = "";
    let err = "";
    child.stdout?.on("data", (d: Buffer) => {
      out += d.toString();
    });
    child.stderr?.on("data", (d: Buffer) => {
      err += d.toString();
    });
    child.on("error", (err) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeoutId);
      signal?.removeEventListener("abort", onAbort);
      resolve({
        verdict: "inconclusive",
        message: `VT child spawn failed: ${err.message}`,
      });
    });
    child.on("close", (code) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeoutId);
      signal?.removeEventListener("abort", onAbort);
      try {
        const line = out.trim().split("\n").pop() ?? "";
        const r = JSON.parse(line) as VirusCheckResult;
        resolve(r);
      } catch {
        resolve({
          verdict: "inconclusive",
          message: `VT child exit ${code}: ${err.slice(0, 300)} ${out.slice(0, 300)}`,
        });
      }
    });
  });
}

export interface VirusCheckerOptions {
  maxScanBytes: number;
  useSeparateVtProcess: boolean;
}

class VirusChecker {
  private readonly apiKey: string;
  private readonly maxScanBytes: number;
  private readonly useSeparateVtProcess: boolean;

  constructor(apiKey: string, options?: Partial<VirusCheckerOptions>) {
    this.apiKey = apiKey;
    this.maxScanBytes = options?.maxScanBytes ?? 400 * 1024 * 1024;
    this.useSeparateVtProcess = options?.useSeparateVtProcess ?? false;
  }

  async check(
    path: PathOrFileDescriptor,
    signal?: AbortSignal,
  ): Promise<VirusCheckResult> {
    const opts = { maxBytes: this.maxScanBytes };
    if (this.useSeparateVtProcess && typeof path === "string") {
      return virusCheckInChildProcess(
        this.apiKey,
        path,
        signal,
        this.maxScanBytes,
      );
    }
    return virusCheckFile(this.apiKey, path, signal, opts);
  }
}

export default VirusChecker;
