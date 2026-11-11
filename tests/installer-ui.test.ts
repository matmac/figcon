import { test } from "node:test";
import assert from "node:assert/strict";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { SetupScreen } from "../src/setup/SetupScreen.tsx";
import { SetupSession, initialSetupSnapshot } from "../src/setup/session.ts";
import { setupView } from "../src/setup/presentation.ts";
import { createInstallerClient } from "../src/installer/client.ts";
import type { CancellationReceipt, Component, InstallationEvent, ManagedInstallationReport, StartReceipt } from "../src/installer/types.ts";
import { fresh, progress, update } from "./setup-fixtures.ts";
async function flush() { for (let i = 0; i < 30; i++) await Promise.resolve(); }
function harness(initial = fresh(), failAfterStart = false, cancelGate?: Promise<CancellationReceipt>) {
  let report = initial, callback!: (event: InstallationEvent) => void, poll!: () => void;
  const calls: string[] = [];let count = 0, removed = 0, stopped = 0;let readsFail=false;
  const receipts: StartReceipt[] = [];
  const client = createInstallerClient({
    invoke: async <T>(command: string, args?: Record<string, unknown>) => {
      calls.push(command);
      if (command === "get_managed_installation_state") {if(readsFail) throw {code:"unstable-snapshot",message:"Synthetic refresh"}; return structuredClone(report) as T;}
      if (command === "cancel_managed_installation") return (cancelGate ? await cancelGate : { attemptId: args?.attemptId, disposition: "requested" }) as T;
      const component: Component = command === "install_managed_node" ? "node" : "mcp";
      const receipt: StartReceipt = { attemptId: `${component}-${++count}`, component, state: "installing", targetVersion: report[component].targetVersion };
      receipts.push(receipt); if(failAfterStart) readsFail=true; report = update(report, component, "installing", receipt.attemptId);return receipt as T;
    },
    listen: async (_, handler) => { callback = handler; return () => removed++; },
  });
  const connected = { ...client, observe: (options: Parameters<typeof client.observe>[0]) => client.observe({ ...options, schedule: tick => { poll = tick; return () => stopped++; } }) };
  const session = new SetupSession(connected);session.connect();
  return { session, calls, receipts, restoreReads:()=>{readsFail=false;}, get: () => report, set: (next: ManagedInstallationReport, emit = true) => { report = next; if (emit) { const op = [next.node, next.mcp].find(c => c.lastOperation?.lastEvent?.revision === next.revision)?.lastOperation; if (op?.lastEvent) callback(op.lastEvent); } }, emit: (event: InstallationEvent) => callback(event), poll: () => poll(), removed: () => removed, stopped: () => stopped };
}
const markup = (report: ManagedInstallationReport | null) => renderToStaticMarkup(React.createElement(SetupScreen, { snapshot: { ...initialSetupSnapshot(), report, checking:!report }, onStart() {}, onCancel() {}, onRefresh() {} }));

test("fresh setup shows one clear action; initialized and loading states are distinct", async () => {
  assert.match(markup(null), /Checking your setup/);assert.doesNotMatch(markup(null), /Set up Figma Console<\/button>/);
  const h = harness();await flush();assert.equal(setupView(h.session.getSnapshot()).canStart, true);
  const html = markup(fresh());assert.match(html, /Set up Figma Console/);assert.match(html, /private Node.js runtime/);assert.match(html, /<ol/);h.session.dispose();
});
test("Node success starts MCP only after observer verification; duplicate actions are prevented", async () => {
  const h = harness();await flush();const run = h.session.start();const duplicate = h.session.start();await flush();await duplicate;
  assert.equal(h.receipts.length, 1);assert.equal(h.receipts[0].component, "node");assert.equal(setupView(h.session.getSnapshot()).canStart, false);
  // A terminal event is only a notification. The backend snapshot still says installing.
  h.emit({ ...progress("node", h.receipts[0].attemptId, h.get().revision + 1, 2), stage: "completed", status: "succeeded" });await flush();assert.equal(h.receipts.length, 1);
  h.set(update(h.get(), "node", "installed", h.receipts[0].attemptId));h.poll();await flush();
  assert.equal(h.receipts.length, 2);assert.equal(h.receipts[1].component, "mcp");
  h.set(update(h.get(), "mcp", "installed", h.receipts[1].attemptId));await flush();await run;
  const view = setupView(h.session.getSnapshot());assert.equal(view.installed, true);assert.equal(view.inProgress, false);h.session.dispose();
});
test("already installed components are skipped and success does not claim connectivity", async () => {
  const both = update(update(fresh(), "node", "installed", "old-node"), "mcp", "installed", "old-mcp");
  const h = harness(both);await flush();await h.session.start();assert.equal(h.receipts.length, 0);
  const html = markup(both);assert.match(html, /Both components are installed/);assert.match(html, /isn’t configured yet/);assert.match(html, /Figma plugin/);assert.doesNotMatch(html, /Set up Figma Console/);h.session.dispose();
  const partial = harness(update(fresh(), "node", "installed", "old"));await flush();const run = partial.session.start();await flush();assert.equal(partial.receipts[0].component, "mcp");partial.set(update(partial.get(), "mcp", "installed", partial.receipts[0].attemptId));await flush();await run;partial.session.dispose();
});
test("failure can be retried safely and only missing components are submitted", async () => {
  const h = harness();await flush();const first = h.session.start();await flush();h.set(update(h.get(), "node", "failed", h.receipts[0].attemptId));await flush();await first;
  assert.equal(setupView(h.session.getSnapshot()).canStart, true);assert.equal(setupView(h.session.getSnapshot()).action, "Retry setup");
  const retry = h.session.start();await flush();assert.equal(h.receipts.length, 2);h.set(update(h.get(), "node", "cancelled", h.receipts[1].attemptId));await flush();await retry;h.session.dispose();
});
test("cancellation requested is not cancellation completed and prevents the next step", async () => {
  const h = harness();await flush();const run = h.session.start();await flush();await h.session.cancel();
  let view = setupView(h.session.getSnapshot());assert.equal(view.stopping, true);assert.equal(view.canStart, false);assert.equal(view.canCancel, false);assert.equal(h.session.getSnapshot().report?.node.state, "installing");
  h.set(update(h.get(), "node", "cancelled", h.receipts[0].attemptId));await flush();await run;view = setupView(h.session.getSnapshot());assert.equal(view.installed, false);assert.equal(view.canStart, true);assert.equal(h.receipts.length, 1);h.session.dispose();
});
test("missed terminal events reconcile via the observer's polling", async () => {
  const h = harness(update(fresh(), "node", "installed", "old"));await flush();const run = h.session.start();await flush();h.set(update(h.get(), "mcp", "installed", h.receipts[0].attemptId), false);await flush();assert.equal(h.session.getSnapshot().working, true);
  h.poll();await flush();await run;assert.equal(setupView(h.session.getSnapshot()).installed, true);h.session.dispose();
});
test("retained staging is separate from failure, and the eight-attempt cap blocks action", async () => {
  const report = update(fresh(), "node", "failed", "failed");let html = markup(report);assert.match(html, /Temporary setup files kept/);assert.match(html, /We couldn’t finish/);assert.match(html, /Retry setup/);
  report.retention.attemptCount = 8;const h = harness(report);await flush();await h.session.start();assert.equal(h.receipts.length, 0);html = markup(report);assert.match(html, /Eight-attempt limit reached/);assert.doesNotMatch(html, /Retry setup/);h.session.dispose();
});
test("window reload follows an active attempt, preserves cancellation, and waits for explicit continuation", async () => {
  const active = update(fresh(), "node", "installing", "existing");const h = harness(active);await flush();assert.equal(setupView(h.session.getSnapshot()).inProgress, true);assert.equal(setupView(h.session.getSnapshot()).canCancel, true);await h.session.start();assert.equal(h.receipts.length, 0);
  h.set(update(h.get(), "node", "installed", "existing"));await flush();assert.equal(setupView(h.session.getSnapshot()).action, "Continue setup");assert.equal(h.receipts.length, 0);h.session.dispose();assert.equal(h.removed(), 1);assert.equal(h.stopped(), 1);
});
test("incomplete existing installations are never submitted for replacement", async () => {
  const report = fresh();report.node.diskState = "invalid";report.node.state = "failed";const h = harness(report);await flush();await h.session.start();assert.equal(h.receipts.length, 0);assert.match(markup(report), /Manual review is needed/);assert.match(markup(report), /won’t overwrite/);h.session.dispose();
});
test("unstable snapshots keep setup recoverable instead of claiming installation failure", () => {
  const snapshot = { ...initialSetupSnapshot(), syncIssue: { code: "unstable-snapshot" as const, message: "synthetic" } };
  const html = renderToStaticMarkup(React.createElement(SetupScreen, { snapshot, onStart() {}, onCancel() {}, onRefresh() {} }));assert.match(html, /Refreshing setup/);assert.match(html, /Check again/);assert.doesNotMatch(html, /The essentials are ready/);
});

test("a known command receipt permits cancellation while snapshots are unstable", async () => {
  const h = harness(fresh(), true);await flush();const run = h.session.start();await flush();
  assert.equal(h.session.getSnapshot().report?.node.state, "not-installed");
  assert.equal(setupView(h.session.getSnapshot()).canCancel, true);
  await h.session.cancel();assert.equal(h.session.getSnapshot().cancelRequested, true);
  h.restoreReads();h.set(update(h.get(), "node", "cancelled", h.receipts[0].attemptId));await flush();h.poll();await flush();await run;
  assert.equal(h.receipts.length, 1);h.session.dispose();
});
test("the attempt cap is checked again between Node and Figma Console", async () => {
  const report = fresh();report.retention = { state:"requires-review",attemptCount:7,orphanedLedgerFiles:0 };
  const h = harness(report);await flush();const run = h.session.start();await flush();h.set(update(h.get(), "node", "installed", h.receipts[0].attemptId));await flush();await run;
  assert.equal(h.receipts.length, 1);assert.equal(setupView(h.session.getSnapshot()).limit, true);h.session.dispose();
});
test("newer observer snapshots replace older presentation-only milestone events", () => {
  const report=update(fresh(),"node","installing","a");report.node.lastOperation!.lastEvent!.stage="validating";report.node.lastOperation!.lastEvent!.revision=5;report.revision=5;
  const snapshot={...initialSetupSnapshot(),report,checking:false,progress:progress("node","a",2)};
  assert.equal(setupView(snapshot).event?.stage,"validating");
});

test("success keeps a refresh action available after a status synchronization error", () => {
  const report=update(update(fresh(),"node","installed","n"),"mcp","installed","m");
  const snapshot={...initialSetupSnapshot(),report,checking:false,syncIssue:{code:"unstable-snapshot" as const,message:"synthetic"}};
  const html=renderToStaticMarkup(React.createElement(SetupScreen,{snapshot,onStart(){},onCancel(){},onRefresh(){}}));
  assert.match(html,/Refresh status/);assert.match(html,/Both components are installed/);
});

test("a delayed cancellation response cannot overlap a new setup action", async () => {
  let release!: (value:CancellationReceipt)=>void;
  const gate = new Promise<CancellationReceipt>(resolve=>{release=resolve;});
  const h=harness(fresh(),false,gate);await flush();const run=h.session.start();await flush();const cancelling=h.session.cancel();
  const id=h.receipts[0].attemptId;h.set(update(h.get(),"node","installed",id));await flush();await run;
  assert.equal(setupView(h.session.getSnapshot()).canStart,false);await h.session.start();assert.equal(h.receipts.length,1);
  release({attemptId:id,disposition:"too-late"});await cancelling;await flush();
  assert.equal(setupView(h.session.getSnapshot()).canStart,true);h.session.dispose();
});
