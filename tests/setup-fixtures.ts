import type { Component, InstallationEvent, ManagedInstallationReport, OperationState } from "../src/installer/types.ts";
export function fresh(): ManagedInstallationReport {
  return { revision: 0, supported: true, node: { component: "node", diskState: "not-installed", state: "not-installed", targetVersion: "24.22.2", installedVersion: null, lastOperation: null, error: null }, mcp: { component: "mcp", diskState: "not-installed", state: "not-installed", targetVersion: "1.46.0", installedVersion: null, lastOperation: null, error: null }, retention: { state: "none", attemptCount: 0, orphanedLedgerFiles: 0 } };
}
export function progress(component: Component, attemptId: string, revision: number, sequence = 1): InstallationEvent {
  return { schemaVersion: 1, component, attemptId, revision, sequence, stage: "downloading", status: "running", downloadedBytes: 10485760, totalBytes: 52428800, retainedStaging: true, activated: false, error: null };
}
export function update(report: ManagedInstallationReport, component: Component, state: OperationState, id: string): ManagedInstallationReport {
  const next = structuredClone(report); next.revision++;
  const previous=report[component].lastOperation;
  const event = progress(component, id, next.revision, previous?.attemptId===id ? (previous.lastEvent?.sequence ?? 0)+1 : 1);event.stage = state === "installed" ? "completed" : state === "failed" ? "failed" : state === "cancelled" ? "cancelled" : component === "node" ? "downloading" : "installing";
  event.status = state === "installed" ? "succeeded" : state === "failed" ? "failed" : state === "cancelled" ? "cancelled" : "running";
  const error = state === "failed" ? { code: "operation-failed" as const, message: "Synthetic failure" } : state === "cancelled" ? { code: "cancelled" as const, message: "Synthetic cancellation" } : null;
  next[component] = { ...next[component], state, diskState: state === "installed" ? "installed" : "not-installed", installedVersion: state === "installed" ? next[component].targetVersion : null, error, lastOperation: { attemptId: id, component, state, targetVersion: next[component].targetVersion, retainedStaging: true, activated: state === "installed", cancellationRequested: false, error, lastEvent: event } };
  next.retention = { state: "requires-review", attemptCount: (report.retention.attemptCount ?? 0) + (previous?.attemptId===id ? 0 : 1), orphanedLedgerFiles: 0 };return next;
}
