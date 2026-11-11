import { test } from "node:test";
import assert from "node:assert/strict";
import { createInstallerClient, INSTALLATION_EVENT } from "../src/installer/client.ts";
import { isInstallationError } from "../src/installer/types.ts";
import type { InstallationEvent } from "../src/installer/types.ts";
function event(attemptId: string, sequence: number): InstallationEvent {
  return { revision: sequence, schemaVersion: 1, attemptId, sequence, component: "node", stage: "preparing", status: "running", downloadedBytes: null, totalBytes: null, retainedStaging: false, activated: false, error: null };
}
test("commands send only fixed names and cancellation ID", async () => {
  const calls: unknown[] = [];
  const api = createInstallerClient({ invoke: async <T>(...args: unknown[]) => { calls.push(args); return {} as T; }, listen: async () => () => {} });
  await api.installNode(); await api.installMcp(); await api.cancel("attempt"); await api.status();
  assert.deepEqual(calls, [["install_managed_node"], ["install_managed_mcp"], ["cancel_managed_installation", { attemptId: "attempt" }], ["get_managed_installation_state"]]);
});
test("dispose before listener registration resolves removes it exactly once", async () => {
  let resolve!: (value: () => void) => void;
  let callback!: (event: InstallationEvent) => void;
  let removed = 0, delivered = 0;
  const api = createInstallerClient({ invoke: async <T>() => ({} as T), listen: (name, handler) => { assert.equal(name, INSTALLATION_EVENT); callback = handler; return new Promise(r => { resolve = r; }); } });
  const subscription = api.subscribe(() => delivered++);
  subscription.dispose(); callback(event("old", 1)); resolve(() => removed++);
  await subscription.ready; subscription.dispose();
  assert.equal(removed, 1); assert.equal(delivered, 0);
});
test("isolates attempts, rejects stale events, and disposes live listeners", async () => {
  let callback!: (event: InstallationEvent) => void;
  let removed = 0;
  const api = createInstallerClient({ invoke: async <T>() => ({} as T), listen: async (_, handler) => { callback = handler; return () => removed++; } });
  const received: number[] = [];
  const subscription = api.subscribe(e => received.push(e.sequence), { attemptId: "current", component: "node" });
  await subscription.ready;
  callback(event("other", 99)); callback(event("current", 1)); callback(event("current", 1)); callback(event("current", 3)); callback(event("current", 2));
  callback({ ...event("current", 4), component: "mcp" });
  subscription.dispose(); subscription.dispose(); callback(event("current", 4));
  assert.deepEqual(received, [1, 3]); assert.equal(removed, 1);
});
test("listener failures and typed command errors are observable", async () => {
  const error = { code: "attempt-limit", message: "Review retained staging" };
  const api = createInstallerClient({ invoke: async () => { throw error; }, listen: async () => { throw new Error("transport unavailable"); } });
  await assert.rejects(api.installNode(), e => isInstallationError(e) && e.code === "attempt-limit");
  const subscription = api.subscribe(() => {});
  await assert.rejects(subscription.ready, /transport unavailable/); subscription.dispose();
  assert.equal(isInstallationError({ code: "arbitrary", message: "secret" }), false);
});
