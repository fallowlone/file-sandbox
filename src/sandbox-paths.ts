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
