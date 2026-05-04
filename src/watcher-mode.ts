export type WatcherMode = "active" | "scan_paused" | "monitoring_disabled";

export const MODES: readonly WatcherMode[] = [
  "active",
  "scan_paused",
  "monitoring_disabled",
] as const;

export function parseMode(input: unknown): WatcherMode {
  if (typeof input !== "string") return "active";
  return (MODES as readonly string[]).includes(input)
    ? (input as WatcherMode)
    : "active";
}
