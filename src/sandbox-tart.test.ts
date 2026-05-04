import { test } from "node:test";
import assert from "node:assert/strict";
import { TartCli } from "./sandbox-tart.ts";

test("clone passes correct args", async () => {
  const calls: string[][] = [];
  const cli = new TartCli({
    runCommand: async (cmd, args) => {
      calls.push([cmd, ...args]);
      return { stdout: "", stderr: "" };
    },
  });
  await cli.clone("base-vm", "fsbx-1");
  assert.deepEqual(calls[0], ["tart", "clone", "base-vm", "fsbx-1"]);
});

test("delete passes correct args", async () => {
  const calls: string[][] = [];
  const cli = new TartCli({
    runCommand: async (cmd, args) => {
      calls.push([cmd, ...args]);
      return { stdout: "", stderr: "" };
    },
  });
  await cli.delete("fsbx-1");
  assert.deepEqual(calls[0], ["tart", "delete", "fsbx-1"]);
});

test("listVms parses JSON output", async () => {
  const cli = new TartCli({
    runCommand: async () => ({
      stdout: JSON.stringify([
        { Name: "base-vm", State: "stopped" },
        { Name: "fsbx-1", State: "running" },
      ]),
      stderr: "",
    }),
  });
  const vms = await cli.listVms();
  assert.equal(vms.length, 2);
  assert.equal(vms[1].name, "fsbx-1");
});
