import express, {
  type Request,
  type Response,
  type NextFunction,
} from "express";
import { resolve } from "path";
import type { JobStore } from "./job-store.ts";
import { JobConflictError } from "./job-store.ts";
import { config, writeConfig, maskSecret, type RawConfig } from "./config.ts";
import { metrics } from "./metrics.ts";

function shouldUpdateSecretField(
  incoming: string | undefined,
  currentReal: string,
): boolean {
  if (incoming === undefined || incoming === "") return false;
  if (currentReal && incoming === maskSecret(currentReal)) return false;
  if (/^\*+$/.test(incoming.trim())) return false;
  if (incoming.startsWith("****") && incoming.length <= 12) return false;
  return true;
}

function validatePaths(
  updates: Record<string, string | number | boolean>,
): string | null {
  const systemDirs = ["/", "/etc", "/bin", "/usr", "/System", "/Library"];

  // Validate watchPath and quarantinePath
  for (const pathKey of ["watchPath", "quarantinePath"]) {
    if (pathKey in updates) {
      const rawPath = String(updates[pathKey]);
      const resolvedPath = resolve(rawPath);

      // Must be absolute (after normalization)
      if (!resolvedPath.startsWith("/")) {
        return `${pathKey} must be an absolute path`;
      }

      // Must not be / or under system directories
      if (
        resolvedPath === "/" ||
        systemDirs.some(
          (dir) => resolvedPath === dir || resolvedPath.startsWith(dir + "/"),
        )
      ) {
        return `${pathKey} cannot be / or under system directories (${systemDirs.join(", ")})`;
      }

      // Update with resolved path
      updates[pathKey] = resolvedPath;
    }
  }

  // Validate databasePath
  if ("databasePath" in updates) {
    const rawPath = String(updates.databasePath);
    const resolvedPath = resolve(rawPath);

    // Must end with .sqlite or .db
    if (!resolvedPath.endsWith(".sqlite") && !resolvedPath.endsWith(".db")) {
      return "databasePath must end with .sqlite or .db";
    }

    // Update with resolved path
    updates.databasePath = resolvedPath;
  }

  return null;
}

const PUBLIC_PATHS = new Set(["/api/health", "/health"]);

function gateHttpAuth(req: Request, res: Response, next: NextFunction): void {
  if (PUBLIC_PATHS.has(req.path)) {
    next();
    return;
  }
  const token = config.apiToken?.trim();
  if (!token) {
    next();
    return;
  }
  const auth = req.headers.authorization;
  const bearer =
    typeof auth === "string" && auth.toLowerCase().startsWith("bearer ")
      ? auth.slice(7).trim()
      : undefined;
  const header =
    typeof req.headers["x-filesandbox-token"] === "string"
      ? req.headers["x-filesandbox-token"]
      : undefined;
  if (bearer === token || header === token) {
    next();
    return;
  }
  res.status(401).json({ error: "Unauthorized" });
}

export interface WatcherControl {
  pause: () => void;
  resume: () => void;
  isPaused: () => boolean;
}

export function startUiServer(
  store: JobStore,
  port: number,
  cancelJob?: (id: string) => void,
  deleteQuarantinedFile?: (id: string, detail?: string) => Promise<void>,
  restoreQuarantinedFile?: (id: string) => Promise<void>,
  watcherControl?: WatcherControl,
) {
  const host = process.env.HTTP_HOST ?? config.httpHost ?? "127.0.0.1";
  const app = express();
  app.use(express.json({ limit: "512kb" }));

  app.get("/api/health", (_req, res) => {
    const scanning = store
      .listRecent(500)
      .filter(
        (j) => j.status === "scanning" || j.status === "in_quarantine",
      ).length;
    res.json({
      ok: true,
      uptimeSec: Math.floor((Date.now() - metrics.startedAt) / 1000),
      activeScans: metrics.activeScans,
      scanningOrQueuedJobs: scanning,
      lastError: metrics.lastError,
      apiAuthEnabled: Boolean(config.apiToken?.trim()),
      configEncryptedAtRest: config.configEncryptedAtRest,
    });
  });

  app.get("/health", (_req, res) => {
    res.redirect(301, "/api/health");
  });

  app.use(gateHttpAuth);

  app.get("/api/jobs", (_req, res) => {
    try {
      res.json({
        jobs: store.listRecent(200),
        paused: watcherControl?.isPaused() ?? false,
      });
    } catch (e) {
      res.status(500).json({ error: String(e) });
    }
  });

  app.post("/api/watcher/pause", (_req, res) => {
    if (!watcherControl) {
      res.status(501).json({ error: "watcher control not configured" });
      return;
    }
    watcherControl.pause();
    res.json({ ok: true, paused: true });
  });

  app.post("/api/watcher/resume", (_req, res) => {
    if (!watcherControl) {
      res.status(501).json({ error: "watcher control not configured" });
      return;
    }
    watcherControl.resume();
    res.json({ ok: true, paused: false });
  });

  app.delete("/api/jobs", (_req, res) => {
    try {
      const { deleted, skipped } = store.clearAll();
      res.json({ ok: true, deleted, skipped });
    } catch (e) {
      res.status(500).json({ error: String(e) });
    }
  });

  app.post("/api/jobs/:id/cancel", (req, res) => {
    try {
      const { id } = req.params;
      cancelJob?.(id);
      res.json({ ok: true });
    } catch (e) {
      res.status(500).json({ error: String(e) });
    }
  });

  app.post("/api/jobs/:id/restore", async (req, res) => {
    try {
      const { id } = req.params;
      if (!restoreQuarantinedFile) {
        res.status(501).json({ error: "restore not configured" });
        return;
      }
      await restoreQuarantinedFile(id);
      res.json({ ok: true });
    } catch (e) {
      res.status(400).json({ error: String(e) });
    }
  });

  app.get("/api/config", (_req, res) => {
    res.json({
      vtApiKey: config.vtApiKey ? maskSecret(config.vtApiKey) : "",
      apiToken: config.apiToken ? maskSecret(config.apiToken) : "",
      watchPath: config.watchPath,
      quarantinePath: config.quarantinePath,
      databasePath: config.databasePath,
      httpPort: config.httpPort !== undefined ? String(config.httpPort) : "",
      httpHost: config.httpHost,
      watchRecursive: config.watchRecursive,
      maxScanBytes: config.maxScanBytes,
      maxConcurrentScans: config.maxConcurrentScans,
      useSeparateVtProcess: config.useSeparateVtProcess,
      inconclusiveRetentionDays: config.inconclusiveRetentionDays,
      configEncryptedAtRest: config.configEncryptedAtRest,
    });
  });

  app.post("/api/config", (req, res) => {
    try {
      const body = req.body as Record<string, unknown>;
      const updates: Record<string, string | number | boolean> = {};
      if (shouldUpdateSecretField(body.vtApiKey as string, config.vtApiKey)) {
        updates.vtApiKey = body.vtApiKey as string;
      }
      if (body.apiToken !== undefined) {
        if (body.apiToken === "") {
          updates.apiToken = "";
        } else if (
          shouldUpdateSecretField(body.apiToken as string, config.apiToken)
        ) {
          updates.apiToken = body.apiToken as string;
        }
      }
      if (typeof body.watchPath === "string" && body.watchPath)
        updates.watchPath = body.watchPath;
      if (typeof body.quarantinePath === "string" && body.quarantinePath)
        updates.quarantinePath = body.quarantinePath;
      if (typeof body.databasePath === "string" && body.databasePath)
        updates.databasePath = body.databasePath;

      // Validate paths before writing config
      const pathError = validatePaths(updates);
      if (pathError) {
        res.status(400).json({ error: pathError });
        return;
      }
      if (typeof body.httpHost === "string" && body.httpHost)
        updates.httpHost = body.httpHost;
      if (typeof body.httpPort === "string" && body.httpPort) {
        const n = Number(body.httpPort);
        if (Number.isFinite(n) && n >= 1 && n <= 65535) updates.httpPort = n;
      }
      const asBool = (v: unknown): boolean | undefined => {
        if (typeof v === "boolean") return v;
        if (v === "true") return true;
        if (v === "false") return false;
        return undefined;
      };
      const asInt = (v: unknown): number | undefined => {
        const n = typeof v === "number" ? v : Number(v);
        return Number.isFinite(n) ? Math.floor(n) : undefined;
      };
      const br = asBool(body.watchRecursive);
      if (br !== undefined) updates.watchRecursive = br;
      const msb = asInt(body.maxScanBytes);
      if (msb !== undefined && msb >= 1) updates.maxScanBytes = msb;
      const mcs = asInt(body.maxConcurrentScans);
      if (mcs !== undefined && mcs >= 1) updates.maxConcurrentScans = mcs;
      const uvp = asBool(body.useSeparateVtProcess);
      if (uvp !== undefined) updates.useSeparateVtProcess = uvp;
      const ird = asInt(body.inconclusiveRetentionDays);
      if (ird !== undefined && ird >= 0)
        updates.inconclusiveRetentionDays = ird;
      writeConfig(updates as Partial<RawConfig>);
      res.json({ ok: true, restartRequired: true });
    } catch (e) {
      res.status(500).json({ error: String(e) });
    }
  });

  app.delete("/api/jobs/:id/quarantine", async (req, res) => {
    try {
      const { id } = req.params;
      if (!deleteQuarantinedFile) {
        res.status(501).json({ error: "delete not configured" });
        return;
      }
      await deleteQuarantinedFile(id);
      res.json({ ok: true });
    } catch (e) {
      const isConflict = e instanceof JobConflictError;
      res.status(isConflict ? 409 : 400).json({ error: String(e) });
    }
  });

  app.get("/", (_req, res) => {
    res.type("html").send(`<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width, initial-scale=1"/>
  <title>file-sandbox jobs</title>
  <style>
    body { font-family: system-ui, sans-serif; margin: 1rem; background: #111; color: #e6e6e6; }
    table { border-collapse: collapse; width: 100%; font-size: 14px; }
    th, td { border: 1px solid #333; padding: 6px 8px; text-align: left; vertical-align: top; }
    th { background: #1a1a1a; }
    tr:nth-child(even) { background: #161616; }
    h1 { font-size: 1.1rem; }
    a { color: #8cb4ff; }
    .oversized { color: #ffb347; font-weight: 600; }
    .refresh-status { color: #666; font-size: 12px; margin-left: 0.5rem; }
  </style>
</head>
<body>
  <h1>Quarantine / VirusTotal job queue</h1>
  <p><a href="/api/jobs">JSON</a> · <a href="/api/health">health</a> · refreshes every 15s<span id="status" class="refresh-status"></span></p>
  <table>
    <thead><tr><th>id</th><th>file</th><th>status</th><th>VT</th><th>detail</th><th>final path</th><th>actions</th></tr></thead>
    <tbody id="jobs"><tr><td colspan="7">loading…</td></tr></tbody>
  </table>
  <script>
    const tbody = document.getElementById('jobs');
    const status = document.getElementById('status');

    function cell(text) {
      const td = document.createElement('td');
      td.textContent = text;
      return td;
    }

    function rowFor(j) {
      const tr = document.createElement('tr');
      tr.appendChild(cell(j.id.slice(0, 8) + '…'));
      tr.appendChild(cell(j.original_name));
      tr.appendChild(cell(j.status));

      const vt = document.createElement('td');
      if (!j.vt_verdict) {
        vt.textContent = '—';
      } else if (j.vt_verdict === 'oversized') {
        const span = document.createElement('span');
        span.className = 'oversized';
        span.textContent = j.vt_verdict;
        vt.appendChild(span);
      } else {
        vt.textContent = j.vt_verdict;
      }
      tr.appendChild(vt);

      const det = document.createElement('td');
      const full = j.detail ?? '';
      det.title = full;
      det.textContent = full.length > 80 ? full.slice(0, 80) + '…' : full;
      tr.appendChild(det);

      tr.appendChild(cell(j.final_path ?? '—'));

      const acts = document.createElement('td');
      if (j.status === 'quarantine_kept') {
        const r = document.createElement('button');
        r.type = 'button';
        r.textContent = 'Restore';
        r.addEventListener('click', () => restoreFile(j.id));
        acts.appendChild(r);
        acts.appendChild(document.createTextNode(' '));
        const d = document.createElement('button');
        d.type = 'button';
        d.textContent = 'Delete';
        d.addEventListener('click', () => deleteFile(j.id));
        acts.appendChild(d);
      }
      tr.appendChild(acts);
      return tr;
    }

    function render(jobs) {
      tbody.replaceChildren();
      if (!jobs.length) {
        const tr = document.createElement('tr');
        const td = document.createElement('td');
        td.colSpan = 7;
        td.textContent = 'no jobs yet';
        tr.appendChild(td);
        tbody.appendChild(tr);
        return;
      }
      for (const j of jobs) tbody.appendChild(rowFor(j));
    }

    async function refresh() {
      try {
        const res = await fetch('/api/jobs');
        if (!res.ok) {
          status.textContent = 'error ' + res.status;
          return;
        }
        const data = await res.json();
        render(data.jobs ?? []);
        status.textContent = '';
      } catch (e) {
        status.textContent = 'offline';
      }
    }

    async function deleteFile(id) {
      if (!confirm('Permanently delete quarantined file?')) return;
      const res = await fetch('/api/jobs/' + id + '/quarantine', { method: 'DELETE' });
      const data = await res.json();
      if (data.ok) refresh();
      else alert('Error: ' + data.error);
    }

    async function restoreFile(id) {
      if (!confirm('Restore this file to the watch folder?')) return;
      const res = await fetch('/api/jobs/' + id + '/restore', { method: 'POST' });
      const data = await res.json();
      if (data.ok) refresh();
      else alert('Error: ' + data.error);
    }

    refresh();
    setInterval(refresh, 15000);
  </script>
</body>
</html>`);
  });

  app.listen(port, host, () => {
    console.log(`UI http://${host}:${port}/`);
  });
}
