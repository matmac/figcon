import { test } from "node:test";
import assert from "node:assert/strict";
import { createInstallerClient } from "../src/installer/client.ts";
import type { InstallationEvent, ManagedInstallationReport, OperationReport } from "../src/installer/types.ts";
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; }
const event = (revision: number, attemptId = "a", sequence = revision): InstallationEvent => ({ schemaVersion: 1, revision, attemptId, sequence, component: "node", stage: "validating", status: "running", downloadedBytes: null, totalBytes: null, retainedStaging: true, activated: false, error: null });
function snapshot(revision: number, e = event(revision), state: OperationReport["state"] = "installing"): ManagedInstallationReport {
  const op: OperationReport = { attemptId: e.attemptId, component: "node", state, targetVersion: "24.22.2", retainedStaging: true, activated: state === "installed", error: null, lastEvent: e, cancellationRequested: false };
  return { revision, supported: true, node: { component: "node", state, diskState: state === "installed" ? "installed" : "not-installed", targetVersion: "24.22.2", installedVersion: state === "installed" ? "24.22.2" : null, lastOperation: op, error: null }, mcp: { component: "mcp", state: "not-installed", diskState: "not-installed", targetVersion: "1.46.0", installedVersion: null, lastOperation: null, error: null }, retention: { state: "requires-review", attemptCount: 1, orphanedLedgerFiles: 0 } };
}
function harness(onPublish?: (state: ManagedInstallationReport) => void) {
  const registration = deferred<() => void>(); const queries: ReturnType<typeof deferred<ManagedInstallationReport>>[] = [];
  let handler!: (e: InstallationEvent) => void, tick!: () => void;
  let removed = 0, stopped = 0;
  const client = createInstallerClient({ invoke: <T>() => { const query = deferred<ManagedInstallationReport>(); queries.push(query); return query.promise as Promise<T>; }, listen: (_, callback) => { handler = callback; return registration.promise; } });
  const states: ManagedInstallationReport[] = [], errors: unknown[] = [];
  const observation = client.observe({ onState: s => { states.push(s); onPublish?.(s); }, onError: e => errors.push(e), schedule: callback => { tick = callback; return () => stopped++; } });
  return { registration, queries, observation, states, errors, emit: (e: InstallationEvent) => handler(e), tick: () => tick(), removed: () => removed, stopped: () => stopped, register: () => registration.resolve(() => removed++) };
}
// Drain promise continuations only; no wall-clock timing assumptions.
async function flush() { for (let i = 0; i < 12; i++) await Promise.resolve(); }

test("registration race buffers notifications before the initial query", async () => {
  const h = harness(); h.emit(event(2)); assert.equal(h.queries.length, 0);
  h.register(); await flush(); assert.equal(h.queries.length, 1);
  h.queries[0].resolve(snapshot(1)); await flush(); assert.equal(h.states.length, 0); assert.equal(h.queries.length, 2);
  h.queries[1].resolve(snapshot(2)); await h.observation.ready; assert.deepEqual(h.states.map(s => s.revision), [2]); h.observation.dispose();
});
test("delayed query cannot overwrite newer events; gaps cause reconciliation", async () => {
  const h = harness(); h.register(); await flush(); h.queries[0].resolve(snapshot(1)); await h.observation.ready;
  h.emit(event(2)); await flush(); h.emit(event(5)); h.queries[1].resolve(snapshot(2)); await flush();
  assert.deepEqual(h.states.map(s => s.revision), [1]); assert.equal(h.queries.length, 3);
  h.queries[2].resolve(snapshot(5)); await flush(); assert.deepEqual(h.states.map(s => s.revision), [1, 5]);
  h.emit(event(3)); assert.equal(h.queries.length, 3); h.observation.dispose();
});
test("polling repairs a completely missed terminal event and remount uses fresh state", async () => {
  const h = harness(); h.register(); await flush(); h.queries[0].resolve(snapshot(1)); await h.observation.ready;
  h.tick(); await flush(); h.queries[1].resolve(snapshot(4, { ...event(4), stage: "completed", status: "succeeded", activated: true }, "installed")); await flush();
  assert.equal(h.states.at(-1)?.node.diskState, "installed"); h.observation.dispose(); assert.equal(h.removed(), 1); assert.equal(h.stopped(), 1);
  const remount = harness(); remount.register(); await flush(); remount.queries[0].resolve(snapshot(4, event(4), "installed")); await remount.observation.ready;
  assert.equal(remount.states[0].node.state, "installed"); remount.observation.dispose();
});
test("successive attempts and old queued events cannot revive previous history", async () => {
  const h = harness(); h.register(); await flush(); h.queries[0].resolve(snapshot(5, event(5, "a", 5), "failed")); await h.observation.ready;
  h.emit(event(6, "b", 1)); await flush(); h.queries[1].resolve(snapshot(6, event(6, "b", 1))); await flush();
  h.emit(event(5, "a", 5)); assert.equal(h.queries.length, 2); assert.equal(h.states.at(-1)?.node.lastOperation?.attemptId, "b");
  h.emit(event(8, "b", 3)); await flush(); assert.equal(h.queries.length, 3); h.queries[2].resolve(snapshot(8, event(8, "b", 3), "cancelled")); await flush();
  assert.equal(h.states.at(-1)?.node.state, "cancelled"); h.observation.dispose();
});
test("dispose during registration and pending queries suppresses all callbacks", async () => {
  const before = harness(); before.observation.dispose(); before.register(); await before.observation.ready; assert.equal(before.removed(), 1); assert.equal(before.queries.length, 0);
  const h = harness(); h.register(); await flush(); h.observation.dispose(); h.observation.dispose(); h.queries[0].resolve(snapshot(1)); await h.observation.ready;
  h.emit(event(2)); assert.equal(h.states.length, 0); assert.equal(h.removed(), 1); assert.equal(h.stopped(), 1);
});
test("unstable snapshots and transport failures are observable and polling can recover", async () => {
  const h = harness(); h.register(); await flush(); h.queries[0].reject({ code: "unstable-snapshot", message: "Refresh installation state." });
  await assert.rejects(h.observation.ready, e => (e as { code: string }).code === "unstable-snapshot"); assert.equal(h.errors.length, 1);
  h.tick(); await flush(); h.queries[1].resolve(snapshot(3)); await flush(); assert.equal(h.states[0].revision, 3); h.observation.dispose();
});
test("repeated invalidation has bounded retries and never publishes stale disk state", async () => {
  const h = harness(); h.register(); await flush();
  const failure = assert.rejects(h.observation.ready, e => (e as { code: string }).code === "unstable-snapshot");
  for (let i = 0; i < 3; i++) { h.emit(event(i + 2)); h.queries[i].resolve(snapshot(i + 1)); await flush(); }
  await failure; assert.equal(h.queries.length, 3); assert.equal(h.states.length, 0); h.observation.dispose();
});

test("an invalidation queued at query completion is not stranded behind the settled query", async () => {
  const h = harness(state => { if (state.revision === 1) queueMicrotask(() => h.emit(event(2))); });
  h.register(); await flush(); h.queries[0].resolve(snapshot(1)); await h.observation.ready; await flush();
  assert.equal(h.queries.length, 2);
  h.queries[1].resolve(snapshot(2)); await flush(); assert.deepEqual(h.states.map(s => s.revision), [1, 2]); h.observation.dispose();
});
