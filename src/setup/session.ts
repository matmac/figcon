/** UI intent and command sequencing only. Installation outcomes come from the observer. */
import type { createInstallerClient } from "../installer/client.ts";
import type { Component, ComponentReport, InstallationError, InstallationEvent, ManagedInstallationReport, StartReceipt } from "../installer/types.ts";
import { isInstallationError } from "../installer/types.ts";
import type { InstallationObservation } from "../installer/observe.ts";

export type SetupClient = ReturnType<typeof createInstallerClient>;
export interface SetupSnapshot {
  report: ManagedInstallationReport | null;
  checking: boolean;
  progress: InstallationEvent | null;
  syncIssue: InstallationError | null;
  issue: InstallationError | null;
  working: boolean;
  requestedComponent: Component | null;
  cancelRequested: boolean;
  cancelInFlight: boolean;
  note: string | null;
  receipt: StartReceipt | null;
}
export const initialSetupSnapshot = (): SetupSnapshot => ({ report: null, checking:true, progress: null, syncIssue: null, issue: null, working: false, requestedComponent: null, cancelRequested: false, cancelInFlight:false, note: null, receipt: null });
const error = (code: InstallationError["code"]): InstallationError => ({ code, message: "Check setup status before continuing." });
const safe = (value: unknown) => isInstallationError(value) ? value : error("state-unavailable");
export const activeOperation = (report: ManagedInstallationReport | null) => report ? [report.node, report.mcp].find(c => c.state === "installing" && c.lastOperation?.state === "installing")?.lastOperation ?? null : null;
export const complete = (report: ManagedInstallationReport | null) => !!report && report.node.diskState === "installed" && report.mcp.diskState === "installed";
export function canStart(snapshot: SetupSnapshot): boolean {
  const { report } = snapshot;
  return !!report && !snapshot.checking && !snapshot.cancelInFlight && report.supported && !snapshot.working && !snapshot.syncIssue && !activeOperation(report) && !complete(report)
    && report.node.diskState !== "invalid" && report.mcp.diskState !== "invalid"
    && report.retention.state !== "unknown" && (report.retention.attemptCount ?? 0) < 8;
}
export class SetupSession {
  private value = initialSetupSnapshot();
  private listeners = new Set<() => void>();
  private observation?: InstallationObservation;
  private generation = 0;
  private disposed = false;
  private stop = false;
  private cancelAttempt: string | null = null;
  private waiters = new Set<{ receipt: StartReceipt; resolve: (report: ComponentReport) => void; reject: (error: InstallationError) => void }>();
  private client: SetupClient;
  constructor(client: SetupClient) { this.client = client; }
  getSnapshot = () => this.value;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private patch(patch: Partial<SetupSnapshot>) { if (this.disposed) return; this.value = { ...this.value, ...patch }; for (const listener of this.listeners) listener(); }
  connect = () => {
    if (this.disposed) return;
    this.observation?.dispose();
    const generation = ++this.generation;
    this.patch({ syncIssue: null, checking:true });
    try { this.observation = this.client.observe({
      onState: report => {
        if (this.disposed || this.generation !== generation) return;
        const cancelledTerminal = [report.node,report.mcp].some(c=>c.lastOperation?.attemptId===this.cancelAttempt && c.lastOperation.state!=="installing");
        this.patch({ report, checking:false, syncIssue: null, cancelRequested: cancelledTerminal ? false : this.value.cancelRequested });
        this.settle();
      },
      onProgress: progress => { if (this.generation === generation) this.patch({ progress }); },
      onError: issue => { if (this.generation === generation) this.patch({ syncIssue: issue }); },
    });
    void this.observation.ready.catch(issue => { if (this.generation === generation) this.patch({ syncIssue: safe(issue) }); });
    } catch(issue) {this.patch({syncIssue:safe(issue)});}
  };
  private settle() {
    const report = this.value.report;
    if (!report) return;
    for (const waiter of this.waiters) {
      const component = report[waiter.receipt.component];
      const operation = component.lastOperation;
      if (operation?.attemptId === waiter.receipt.attemptId && operation.state !== "installing") {
        this.waiters.delete(waiter);
        if (operation.state === "installed" && component.diskState === "installed") waiter.resolve(component);
        else waiter.reject(operation.error ?? error(operation.state === "cancelled" ? "cancelled" : "operation-failed"));
      } else if (!activeOperation(report) && component.diskState === "installed") {
        // Another successful installation may already have superseded history.
        // Only the observer's verified disk observation can satisfy this branch.
        this.waiters.delete(waiter); waiter.resolve(component);
      }
    }
  }
  private wait(receipt: StartReceipt): Promise<ComponentReport> {
    return new Promise((resolve, reject) => { this.waiters.add({ receipt, resolve, reject }); this.settle(); });
  }
  start = async (): Promise<void> => {
    if (!canStart(this.value)) return;
    this.stop = false;
    this.patch({ working: true, issue: null, note: null, cancelRequested: false, progress: null });
    try {
      await this.observation?.refresh();
      if (this.disposed || this.stop) return;
      const report = this.value.report;
      if (!report || !report.supported || activeOperation(report) || [report.node, report.mcp].some(c => c.diskState === "invalid") || report.retention.state === "unknown" || (report.retention.attemptCount ?? 0) >= 8) throw error("state-unavailable");
      for (const component of ["node", "mcp"] as const) {
        if (this.disposed || this.stop) return;
        const current = this.value.report;
        if (!current) throw error("state-unavailable");
        if (current[component].diskState === "installed") continue;
        if (activeOperation(current)) throw error("busy");
        if (current.retention.state === "unknown" || current[component].diskState === "invalid") throw error("state-unavailable");
        if ((current.retention.attemptCount ?? 0) >= 8) throw error("attempt-limit");
        if (component === "mcp" && current.node.diskState !== "installed") throw error("runtime-required");
        this.patch({ requestedComponent: component });
        const receipt = await (component === "node" ? this.client.installNode() : this.client.installMcp());
        if (this.disposed) return;
        this.patch({receipt});
        const terminal = this.wait(receipt);
        // A missed event or a command that completed before its response is
        // repaired by the existing observer. Errors stay in syncIssue; polling
        // continues, and no second installer is started on an uncertain result.
        void this.observation?.refresh().catch(() => {});
        await terminal;
        this.patch({ requestedComponent: null, progress: null, receipt: null });
      }
    } catch (issue) { if (!this.disposed) {const failure=safe(issue); this.patch(failure.code === "unstable-snapshot" ? {syncIssue:failure} : {issue:failure});} }
    finally { this.cancelAttempt=null;this.patch({ working: false, requestedComponent: null, receipt: null, cancelRequested:false }); }
  };
  cancel = async (): Promise<void> => {
    const operation = activeOperation(this.value.report);
    const attemptId=operation?.attemptId ?? this.value.receipt?.attemptId;
    if (!attemptId || operation?.activated || operation?.cancellationRequested || this.value.cancelRequested) return;
    this.cancelAttempt=attemptId;
    this.stop = true; // A request also prevents this window launching the next step.
    this.patch({ cancelRequested: true, cancelInFlight:true, issue: null, note: null });
    try {
      const result = await this.client.cancel(attemptId);
      if (result.disposition === "too-late") this.patch({ note: "This step is already finishing. Remaining steps will wait for you to continue." });
    } catch (issue) {
      if (isInstallationError(issue) && issue.code === "not-active") this.patch({cancelRequested:false,note:"This step has finished. Checking its final status."});
      else this.patch({ cancelRequested: false, issue: safe(issue) });
    }
    finally { this.patch({cancelInFlight:false});void this.observation?.refresh().catch(() => {}); }
  };
  dispose = () => {
    this.disposed = true; this.stop = true; this.generation++;
    this.observation?.dispose();
    for (const waiter of this.waiters) waiter.reject(error("cancelled"));
    this.waiters.clear(); this.listeners.clear();
  };
}
