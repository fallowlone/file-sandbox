import { test } from "node:test";
import assert from "node:assert/strict";
import { JobStore } from "./job-store.ts";

function freshStore() {
  return new JobStore(":memory:");
}

test("pompelmi_verdict starts null and persists when set", () => {
  const store = freshStore();
  store.insertReceived("job-1", "/a.bin", "a.bin");
  const before = store.getJob("job-1");
  assert.equal(before?.pompelmi_verdict, null);

  store.setPompelmiVerdict("job-1", "clean", "ok");
  const after = store.getJob("job-1");
  assert.equal(after?.pompelmi_verdict, "clean");
});
