// src/sandbox-manager.test.ts
import { test } from "node:test";
import assert from "node:assert/strict";
import path from "path";
import os from "os";
import fs from "fs";
import { SandboxStore } from "./sandbox-store.ts";
import { SandboxManager } from "./sandbox-manager.ts";

class FakeTart {
  cloned: Array<{ base: string; name: string }> = [];
  deleted: string[] = [];
  vms = [{ name: "base-vm", state: "stopped" }];
  async version() { return "tart 0.0.0-fake"; }
  async clone(base: string, name: string) { this.cloned.push({ base, name }); this.vms.push({ name, state: "stopped" }); }
  async delete(name: string) { this.deleted.push(name); this.vms = this.vms.filter(v => v.name !== name); }
  async listVms() { return [...this.vms]; }
}

class FakeSpawner {
  pids: number[] = [];
  exitListeners = new Map<number, (code: number) => void>();
  spawnRun(args: string[]) {
    const pid = 1000 + this.pids.length + 1;
    this.pids.push(pid);
    return {
      pid,
      kill: (_sig: string) => { /* invoked by manager */ },
      onExit: (cb: (code: number) => void) => this.exitListeners.set(pid, cb),
    };
  }
  triggerExit(pid: number, code: number) {
    this.exitListeners.get(pid)?.(code);
  }
}

function tmpDir() {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "fsbx-test-"));
  return fs.realpathSync(tmp);
}

test("createSession copies file, clones VM, runs, sets running", async () => {
  const dir = tmpDir();
  const src = path.join(dir, "input.bin");
  fs.writeFileSync(src, "hello");

  const store = new SandboxStore(":memory:");
  const tart = new FakeTart();
  const spawner = new FakeSpawner();
  const mgr = new SandboxManager({
    store,
    tart: tart as any,
    spawner: spawner as any,
    sessionsDir: path.join(dir, "sessions"),
    baseVm: "base-vm",
    idleTimeoutMinutes: 240,
    networkDefault: false,
    allowedRoots: { watchPath: dir, quarantinePath: dir, homeDir: dir },
  });
  await mgr.init();

  const session = await mgr.createSession({ filePath: src });
  assert.equal(tart.cloned.length, 1);
  assert.equal(session.status, "running");
  assert.equal(session.networkEnabled, false);

  const inDir = path.join(session.sessionDir, "in");
  assert.ok(fs.existsSync(path.join(inDir, "input.bin")));
});

test("discardSession kills child, deletes clone, marks discarded", async () => {
  const dir = tmpDir();
  const src = path.join(dir, "f.bin");
  fs.writeFileSync(src, "x");

  const store = new SandboxStore(":memory:");
  const tart = new FakeTart();
  const spawner = new FakeSpawner();
  const mgr = new SandboxManager({
    store,
    tart: tart as any,
    spawner: spawner as any,
    sessionsDir: path.join(dir, "sessions"),
    baseVm: "base-vm",
    idleTimeoutMinutes: 240,
    networkDefault: false,
    allowedRoots: { watchPath: dir, quarantinePath: dir, homeDir: dir },
  });
  await mgr.init();
  const s = await mgr.createSession({ filePath: src });
  await mgr.discardSession(s.id);
  const after = store.get(s.id);
  assert.equal(after?.status, "discarded");
  assert.ok(tart.deleted.includes(s.vmName));
});

test("reconcile marks dead VMs discarded on init", async () => {
  const dir = tmpDir();
  const store = new SandboxStore(":memory:");
  store.insert({
    id: "stale-1", vmName: "fsbx-stale", sourceJobId: null, sourceFilePath: "/x", sessionDir: path.join(dir, "stale"), networkEnabled: false,
  });
  store.setRunning("stale-1", 9999);
  const tart = new FakeTart(); // no fsbx-stale in vms
  const spawner = new FakeSpawner();
  const mgr = new SandboxManager({
    store,
    tart: tart as any,
    spawner: spawner as any,
    sessionsDir: path.join(dir, "sessions"),
    baseVm: "base-vm",
    idleTimeoutMinutes: 240,
    networkDefault: false,
    allowedRoots: { watchPath: dir, quarantinePath: dir, homeDir: dir },
  });
  await mgr.init();
  const r = store.get("stale-1");
  assert.equal(r?.status, "discarded");
});
