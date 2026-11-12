import { test } from "node:test";
import assert from "node:assert/strict";
import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { ConnectionSession, canApprove, canPreview, prerequisitesFrom } from "../src/connections/session.ts";
import { ConnectionsScreen } from "../src/connections/ConnectionsScreen.tsx";
import { Navigation } from "../src/connections/Navigation.tsx";
import type { ClientInspection, ConsentPreview, RecoveryUsage, ApprovalReceipt } from "../src/client-integration/types.ts";
import type { ConnectionSnapshot } from "../src/connections/session.ts";
import { initialSetupSnapshot } from "../src/setup/session.ts";
import { fresh, update } from "./setup-fixtures.ts";
const recovery: RecoveryUsage = { state: "available", artifactCount: 0, storageBytes: 0, fileCount: 0, availableDiskBytes: 1000000000, maxArtifacts: 32, maxStorageBytes: 67108864, maxFiles: 4096, reservedTransactionBytes: 12582912, automaticCleanupEnabled: false };
const inspection = (client: ClientInspection["client"] = "claude-code"): ClientInspection => ({ client, clientName: client, detection: "detected", configuration: "readable", configurationLocation: "user-level", registration: "not-registered", ownership: "absent", warnings: ["read-only"] });
const preview = (): ConsentPreview => ({ planId: "opaque-plan", client: "claude-code", clientName: "Claude Code", proposedServerName: "figma-console", proposedExecutable: "/synthetic/figcon/runtime/bin/node", proposedArguments: ["/synthetic/figcon/mcp/node_modules/figma-console-mcp/dist/local.js"], scope: "user", operation: "add", registration: "not-registered", summary: "Synthetic addition", warnings: ["production-disabled", "external-writer-race", "recovery-files-retained"], expiresAtUnixMs: 301000, expiresInSeconds: 300, productionMutationEnabled: false, recovery });
function deferred<T>() { let resolve!: (value: T) => void; let reject!: (error: unknown) => void; const promise = new Promise<T>((a, b) => { resolve = a; reject = b; }); return { promise, resolve, reject }; }
function harness() {
  let now = 1000, tick!: () => void, stopped = 0;
  const calls: string[] = [];
  const sources = {
    list: async () => [inspection(), inspection("codex-cli")],
    recovery: async () => recovery,
    prepare: async () => preview(),
    approve: async (): Promise<ApprovalReceipt> => { throw { code: "activation-disabled", message: "Never render this backend message" }; },
    reject: async (planId: string): Promise<ApprovalReceipt> => ({ planId, state: "rejected" }),
  };
  const session = new ConnectionSession({ list: () => { calls.push("list"); return sources.list(); }, inspect: async () => inspection(), preview: async () => { throw Error("Unused informational endpoint"); } }, {
    recovery: () => { calls.push("recovery"); return sources.recovery(); },
    prepare: async client => { calls.push(`prepare:${client}`); return sources.prepare(); },
    approve: async id => { calls.push(`approve:${id}`); return sources.approve(); },
    reject: async id => { calls.push(`reject:${id}`); return sources.reject(id); },
  }, { wall: () => now, monotonic: () => now, schedule: callback => { tick = callback; return () => stopped++; } });
  session.setPrerequisites({ state: "ready", missing: [] });
  return { session, sources, calls, advance: (ms: number) => { now += ms; tick(); }, stopped: () => stopped };
}
function screen(snapshot: ConnectionSnapshot) {
  return renderToStaticMarkup(React.createElement(ConnectionsScreen, { snapshot, onRefresh() {}, onPrepare() {}, onConfirm() {}, onApprove() {}, onReject() {}, onSetup() {} }));
}
test("client cards show detected and missing clients without installing or approving", async () => {
  const h = harness(); h.sources.list = async () => [inspection(), { ...inspection("codex-cli"), detection: "missing" }];
  await h.session.refresh(); const s = h.session.getSnapshot(); const html = screen(s);
  assert.match(html, /Claude Code/); assert.match(html, /Codex/); assert.match(html, /Installed/); assert.match(html, /Not detected/);
  assert.equal(canPreview(s, "claude-code"), true); assert.equal(canPreview(s, "codex-cli"), false);
  assert.deepEqual(h.calls, ["list", "recovery"]); h.session.dispose();
});
test("external, conflicting and unsupported registrations are explained and never offered replacement", async () => {
  for (const registration of ["matching-external", "external", "conflict", "unsupported", "unknown"] as const) {
    const h = harness(); h.sources.list = async () => [{ ...inspection(), registration }]; await h.session.refresh();
    assert.equal(canPreview(h.session.getSnapshot(), "claude-code"), false);
    assert.match(screen(h.session.getSnapshot()), registration.includes("external") ? /Configured outside Figcon/ : /review|checked/);
    await h.session.prepare("claude-code"); assert.equal(h.calls.length, 2); h.session.dispose();
  }
});
test("malformed and inaccessible settings block previews without revealing configuration", async () => {
  for (const configuration of ["malformed", "inaccessible", "unsafe", "unsupported-location"] as const) {
    const h = harness(); h.sources.list = async () => [{ ...inspection(), configuration }]; await h.session.refresh();
    assert.equal(canPreview(h.session.getSnapshot(), "claude-code"), false); assert.match(screen(h.session.getSnapshot()), /Settings need review/); h.session.dispose();
  }
});
test("prerequisites use the existing installer observer and keep active work separate", async () => {
  const initial = initialSetupSnapshot(); assert.equal(prerequisitesFrom(initial).state, "checking");
  initial.report = fresh(); assert.equal(prerequisitesFrom(initial).state, "missing");
  initial.report = update(initial.report, "node", "installing", "node-1"); assert.equal(prerequisitesFrom(initial).state, "installing");
  initial.report = update(initial.report, "node", "installed", "node-1"); initial.report = update(initial.report, "mcp", "installed", "mcp-1");
  assert.equal(prerequisitesFrom(initial).state, "ready");
  const h = harness(); await h.session.refresh(); h.session.setPrerequisites({ state: "missing", missing: ["Private runtime"] });
  await h.session.prepare("claude-code"); assert.equal(h.calls.length, 2); assert.match(screen(h.session.getSnapshot()), /Private runtime must be installed/); h.session.dispose();
});
test("backend plan details are shown exactly and preparation never implies approval", async () => {
  const h = harness(); await h.session.refresh(); await h.session.prepare("claude-code");
  const html = screen(h.session.getSnapshot()); assert.match(html, /figma-console/); assert.match(html, /\/synthetic\/figcon\/runtime\/bin\/node/);
  assert.match(html, /dist\/local.js/); assert.match(html, /None · Addition only/); assert.match(html, /Expires in 5:00/);
  assert.match(html, /<details/); assert.match(html, /Configuration scope/); assert.equal(canApprove(h.session.getSnapshot()), false);
  await h.session.approve(); assert.equal(h.calls.some(c => c.startsWith("approve")), false); h.session.dispose();
});
test("approval requires explicit confirmation and passes only the plan ID; backend gate is clearly blocked", async () => {
  const h = harness(); await h.session.refresh(); await h.session.prepare("claude-code"); h.session.confirm(true);
  assert.equal(canApprove(h.session.getSnapshot()), true); await h.session.approve();
  assert.equal(h.calls.at(-1), "approve:opaque-plan"); assert.equal(h.session.getSnapshot().outcome, "blocked"); assert.equal(h.session.getSnapshot().plan, null);
  const html = screen(h.session.getSnapshot()); assert.match(html, /Changes are blocked/); assert.match(html, /no client connection was confirmed/); assert.doesNotMatch(html, /Never render/); h.session.dispose();
});
test("duplicate prepare and approval submissions are ignored while requests are pending", async () => {
  const h = harness(); await h.session.refresh(); const preparation = deferred<ConsentPreview>(); h.sources.prepare = () => preparation.promise;
  const pending = h.session.prepare("claude-code"); await h.session.prepare("claude-code"); assert.equal(h.calls.filter(c => c.startsWith("prepare")).length, 1);
  preparation.resolve(preview()); await pending; h.session.confirm(true);
  const approval = deferred<ApprovalReceipt>(); h.sources.approve = () => approval.promise;
  const approving = h.session.approve(); await h.session.approve(); await h.session.refresh();
  assert.equal(h.calls.filter(c => c.startsWith("approve")).length, 1); assert.equal(h.calls.filter(c => c === "list").length, 1);
  approval.reject({ code: "activation-disabled" }); await approving; h.session.dispose();
});
test("declining uses the rejection contract and cannot approve the discarded plan", async () => {
  const h = harness(); await h.session.refresh(); await h.session.prepare("claude-code"); await h.session.reject();
  assert.equal(h.calls.at(-1), "reject:opaque-plan"); assert.equal(h.session.getSnapshot().outcome, "rejected");
  h.session.confirm(true); await h.session.approve(); assert.equal(h.calls.some(c => c.startsWith("approve")), false); assert.match(screen(h.session.getSnapshot()), /Plan declined/); h.session.dispose();
});
test("expiration deterministically removes consent and requires a fresh preview", async () => {
  const h = harness(); await h.session.refresh(); await h.session.prepare("claude-code"); h.session.confirm(true); h.advance(300001);
  assert.equal(h.session.getSnapshot().planState, "expired"); assert.equal(h.session.getSnapshot().confirmed, false);
  await h.session.approve(); assert.equal(h.calls.some(c => c.startsWith("approve")), false); assert.match(screen(h.session.getSnapshot()), /This plan has expired/); h.session.dispose();
});
test("changed configuration requires a new review rather than reusing approval", async () => {
  const h = harness(); await h.session.refresh(); await h.session.prepare("claude-code"); h.session.confirm(true);
  h.sources.approve = async () => { throw { code: "stale-plan", message: "secret raw payload" }; }; await h.session.approve();
  assert.equal(h.session.getSnapshot().outcome, "stale"); assert.equal(h.session.getSnapshot().plan, null); assert.match(screen(h.session.getSnapshot()), /Settings changed/);
  await h.session.refresh(); await h.session.prepare("claude-code"); assert.equal(h.session.getSnapshot().confirmed, false); h.session.dispose();
});
test("capacity limits fail closed and recovery remains non-destructive", async () => {
  const h = harness(); h.sources.recovery = async () => ({ ...recovery, state: "full", artifactCount: 32 }); await h.session.refresh();
  assert.equal(canPreview(h.session.getSnapshot(), "claude-code"), false); assert.match(screen(h.session.getSnapshot()), /Nothing is deleted automatically/);
  h.sources.recovery = async () => recovery; await h.session.refresh(); h.sources.prepare = async () => { throw { code: "registry-capacity" }; };
  await h.session.prepare("claude-code"); assert.match(screen(h.session.getSnapshot()), /Too many pending plans/); h.session.dispose();
});
test("inspection errors are sanitized and explicit refresh recovers", async () => {
  const h = harness(); h.sources.list = async () => { throw new Error("TOKEN=synthetic-secret /private/home/user"); }; await h.session.refresh();
  const html = screen(h.session.getSnapshot()); assert.doesNotMatch(html, /synthetic-secret|private\/home/); assert.match(html, /Refresh and try again/);
  assert.equal(canPreview(h.session.getSnapshot(), "claude-code"), false); h.sources.list = async () => [inspection()]; await h.session.refresh();
  assert.equal(canPreview(h.session.getSnapshot(), "claude-code"), true); h.session.dispose();
});
test("prerequisite changes during preview invalidate confirmation without starting other work", async () => {
  const h = harness(); await h.session.refresh(); const gate = deferred<ConsentPreview>(); h.sources.prepare = () => gate.promise;
  const pending = h.session.prepare("claude-code"); h.session.setPrerequisites({ state: "installing", missing: [] }); gate.resolve(preview()); await pending;
  assert.equal(h.session.getSnapshot().planState, "stale"); h.session.confirm(true); await h.session.approve();
  assert.equal(h.calls.length, 3); assert.match(screen(h.session.getSnapshot()), /Installation continues/); h.session.dispose();
});
test("unexpected or enabled plans fail closed, including invalid expiration timestamps", async () => {
  for (const change of [{ productionMutationEnabled: true }, { client: "codex-cli" as const }, { expiresAtUnixMs: 1e100 }, { expiresInSeconds: 301 }]) {
    const h = harness(); await h.session.refresh(); h.sources.prepare = async () => ({ ...preview(), ...change }); await h.session.prepare("claude-code");
    h.session.confirm(true); await h.session.approve(); assert.equal(h.session.getSnapshot().plan, null); assert.equal(h.session.getSnapshot().issue, "unexpected-response"); assert.equal(h.calls.length, 3); h.session.dispose();
  }
});
test("unknown plan warnings disable approval without rendering raw warning data", async () => {
  const h = harness(); await h.session.refresh(); h.sources.prepare = async () => ({ ...preview(), warnings: ["synthetic-secret-warning"] }); await h.session.prepare("claude-code");
  h.session.confirm(true); assert.equal(canApprove(h.session.getSnapshot()), false); assert.doesNotMatch(screen(h.session.getSnapshot()), /synthetic-secret-warning/); h.session.dispose();
});
test("disposal stops expiration timer and suppresses delayed replies and subscribers", async () => {
  const h = harness(); let notifications = 0; h.session.subscribe(() => notifications++);
  const gate = deferred<ClientInspection[]>(); h.sources.list = () => gate.promise; const pending = h.session.refresh(); h.session.dispose(); const count = notifications;
  gate.resolve([inspection()]); await pending; h.advance(400000); assert.equal(notifications, count); assert.equal(h.stopped(), 1); assert.equal(h.session.getSnapshot().loaded, false);
});
test("native navigation and confirmation controls expose keyboard and focus semantics", async () => {
  const nav = renderToStaticMarkup(React.createElement(Navigation, { page: "clients", onNavigate() {}, installing: true }));
  assert.match(nav, /<nav[^>]*aria-label=/); assert.match(nav, /aria-current="page"/); assert.match(nav, /In progress/);
  const h = harness(); await h.session.refresh(); await h.session.prepare("claude-code"); const html = screen(h.session.getSnapshot());
  assert.match(html, /<form/); assert.match(html, /type="checkbox" required=""/); assert.match(html, /type="submit" disabled=""/); assert.match(html, /tabindex="-1"/); assert.match(html, /Escape declines/); h.session.dispose();
});
