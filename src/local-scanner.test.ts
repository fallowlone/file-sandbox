import { test } from "node:test";
import assert from "node:assert/strict";
import { LocalScanner, type LocalScanResult } from "./local-scanner.ts";

test("probe rejects when socket missing", async () => {
  await assert.rejects(
    () => LocalScanner.probe("/tmp/definitely-not-a-real-socket-987654.sock"),
    /unreachable|ENOENT|connect/i,
  );
});

test("verdict translation maps pompelmi result to LocalScanResult", async () => {
  // Build a scanner that bypasses pompelmi via a fake check function for testing.
  const fakeRunner = async (_path: string) => "MALICIOUS";
  const scanner = LocalScanner.fromFakeRunner(fakeRunner);
  const r: LocalScanResult = await scanner.check("/tmp/x");
  assert.equal(r.verdict, "malicious");
});
