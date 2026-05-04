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

  /** Test seam - bypass real pompelmi for unit tests. */
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
