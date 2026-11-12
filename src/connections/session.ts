/** Presentation and explicit user intent only; backend reports remain authoritative. */
import type { createAiClientInspector } from "../client-integration/client.ts";
import type { createClientConsentApi } from "../client-integration/consent.ts";
import type { AiClient, ClientInspection, ConsentCode, ConsentPreview, RecoveryUsage } from "../client-integration/types.ts";
import type { SetupSnapshot } from "../setup/session.ts";
import { activeOperation } from "../setup/session.ts";
export type Issue = ConsentCode | "request-failed" | "unexpected-response" | "inspection-unavailable";
export interface Prerequisites { state: "checking" | "ready" | "missing" | "installing" | "invalid" | "unavailable"; missing: string[] }
export function prerequisitesFrom(setup: SetupSnapshot): Prerequisites {
  const r = setup.report;
  if (!r) return { state: "checking", missing: [] };
  if (setup.syncIssue || !r.supported) return { state: "unavailable", missing: [] };
  if (activeOperation(r) || setup.working) return { state: "installing", missing: [] };
  if (r.node.diskState === "invalid" || r.mcp.diskState === "invalid") return { state: "invalid", missing: [] };
  const missing = [r.node.diskState !== "installed" ? "Private runtime" : "", r.mcp.diskState !== "installed" ? "Figma Console" : ""].filter(Boolean);
  return { state: missing.length ? "missing" : "ready", missing };
}
export interface ConnectionSnapshot {
  clients: ClientInspection[]; loaded: boolean; action: "idle" | "refreshing" | "preparing" | "approving" | "rejecting";
  selected: AiClient | null; plan: ConsentPreview | null; planState: "review" | "expired" | "stale";
  confirmed: boolean; secondsLeft: number; issue: Issue | null; inspectionIssue: Issue | null;
  outcome: "blocked" | "rejected" | "expired" | "stale" | "error" | null;
  recovery: RecoveryUsage | null; recoveryIssue: boolean; prerequisites: Prerequisites;
}
export const initialConnectionSnapshot = (): ConnectionSnapshot => ({ clients: [], loaded: false, action: "idle", selected: null, plan: null, planState: "review", confirmed: false, secondsLeft: 0, issue: null, inspectionIssue: null, outcome: null, recovery: null, recoveryIssue: false, prerequisites: { state: "checking", missing: [] } });
const codes: readonly Issue[] = ["unsupported-platform", "prerequisites-missing", "client-unavailable", "unsupported-configuration", "conflict", "stale-plan", "expired-plan", "unknown-plan", "consumed-plan", "not-owner", "registry-capacity", "recovery-capacity", "activation-disabled", "rejected", "operation-failed", "uncertain", "inspection-unavailable"];
function safeIssue(error: unknown): Issue {
  if (error && typeof error === "object" && "code" in error && typeof error.code === "string" && codes.includes(error.code as Issue)) return error.code as Issue;
  return "request-failed"; // Never retain or render transport messages, stacks or configuration data.
}
export const knownWarnings = ["production-disabled", "external-writer-race", "backup-required-for-existing-file", "recovery-files-retained", "effective-configuration-unknown"];
export function canPreview(s: ConnectionSnapshot, client: AiClient): boolean {
  const c = s.clients.find(c => c.client === client);
  return s.action === "idle" && s.loaded && !s.inspectionIssue && s.prerequisites.state === "ready" && !!c && c.detection === "detected"
    && (c.configuration === "missing" || c.configuration === "readable") && c.registration === "not-registered"
    && !s.recoveryIssue && s.recovery?.state === "available";
}
export function canApprove(s: ConnectionSnapshot): boolean {
  return s.action === "idle" && !!s.plan && s.planState === "review" && s.secondsLeft > 0 && s.confirmed && s.prerequisites.state === "ready"
    && s.plan.productionMutationEnabled === false && s.plan.warnings.every(w => knownWarnings.includes(w));
}
interface Clock { wall(): number; monotonic(): number; schedule(tick: () => void): () => void }
const defaultClock: Clock = { wall: Date.now, monotonic: () => performance.now(), schedule: tick => { const timer = setInterval(tick, 1000); return () => clearInterval(timer); } };
export class ConnectionSession {
  private value = initialConnectionSnapshot();
  private listeners = new Set<() => void>();
  private disposed = false;
  private deadline = 0;
  private stopClock: () => void;
  private inspection: ReturnType<typeof createAiClientInspector>;
  private consent: ReturnType<typeof createClientConsentApi>;
  private clock: Clock;
  constructor(inspection: ReturnType<typeof createAiClientInspector>, consent: ReturnType<typeof createClientConsentApi>, clock: Clock = defaultClock) {
    this.inspection = inspection; this.consent = consent; this.clock = clock;
    this.stopClock = clock.schedule(() => this.tick());
  }
  getSnapshot = () => this.value;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private patch(values: Partial<ConnectionSnapshot>) { if (this.disposed) return; this.value = { ...this.value, ...values }; for (const listener of this.listeners) listener(); }
  setPrerequisites = (prerequisites: Prerequisites) => {
    if (this.disposed || JSON.stringify(prerequisites) === JSON.stringify(this.value.prerequisites)) return;
    const invalidates = prerequisites.state !== "ready" && this.value.plan && this.value.planState === "review";
    this.patch({ prerequisites, ...(invalidates ? { planState: "stale", confirmed: false, issue: "prerequisites-missing" } : {}) });
  };
  refresh = async () => {
    if (this.disposed || this.value.action !== "idle") return;
    this.patch({ action: "refreshing", plan: null, confirmed: false, issue: null, inspectionIssue: null, outcome: null });
    const [clients, recovery] = await Promise.allSettled([Promise.resolve().then(() => this.inspection.list()), Promise.resolve().then(() => this.consent.recovery())]);
    this.patch({ action: "idle", loaded: clients.status === "fulfilled" || this.value.loaded,
      ...(clients.status === "fulfilled" ? { clients: clients.value, inspectionIssue: null } : { inspectionIssue: safeIssue(clients.reason) }),
      recovery: recovery.status === "fulfilled" ? recovery.value : null, recoveryIssue: recovery.status === "rejected" });
  };
  prepare = async (client: AiClient) => {
    if (this.disposed || !canPreview(this.value, client)) return;
    this.patch({ action: "preparing", selected: client, plan: null, confirmed: false, outcome: null, issue: null });
    try {
      const plan = await this.consent.prepare(client);
      if (this.disposed) return;
      if (plan.client !== client || plan.scope !== "user" || plan.operation !== "add" || plan.registration !== "not-registered" || plan.productionMutationEnabled !== false || !Number.isFinite(plan.expiresAtUnixMs) || !Number.isFinite(new Date(plan.expiresAtUnixMs).getTime()) || !Number.isFinite(plan.expiresInSeconds) || plan.expiresInSeconds < 0 || plan.expiresInSeconds > 300) {
        this.patch({ issue: "unexpected-response", outcome: "error" }); return;
      }
      const remaining = Math.max(0, Math.min(plan.expiresInSeconds * 1000, plan.expiresAtUnixMs - this.clock.wall()));
      this.deadline = this.clock.monotonic() + remaining;
      const state = remaining <= 0 ? "expired" : this.value.prerequisites.state !== "ready" ? "stale" : "review";
      this.patch({ plan, secondsLeft: Math.ceil(remaining / 1000), planState: state, confirmed: false, ...(state === "expired" ? { outcome: "expired", issue: "expired-plan" } : {}) });
    } catch (error) { this.failure(error); }
    finally { this.patch({ action: "idle" }); }
  };
  private tick() {
    const p = this.value.plan;
    if (this.disposed || !p || this.value.planState !== "review") return;
    const secondsLeft = Math.max(0, Math.ceil(Math.min(this.deadline - this.clock.monotonic(), p.expiresAtUnixMs - this.clock.wall()) / 1000));
    if (secondsLeft <= 0) this.patch({ secondsLeft: 0, confirmed: false, planState: "expired", ...(this.value.action === "idle" ? { outcome: "expired", issue: "expired-plan" } : {}) });
    else if (secondsLeft !== this.value.secondsLeft) this.patch({ secondsLeft });
  }
  confirm = (confirmed: boolean) => {
    this.tick();
    if (this.value.action === "idle" && this.value.planState === "review") this.patch({ confirmed });
  };
  approve = async () => {
    this.tick();
    if (this.disposed || !canApprove(this.value)) return;
    const plan = this.value.plan!;
    this.patch({ action: "approving", confirmed: false, issue: null });
    try {
      // This method is called only by the explicit confirmed form submission.
      const receipt = await this.consent.approve(plan.planId);
      if (receipt.planId !== plan.planId || receipt.state !== "verified") throw { code: "operation-failed" };
      // This build cannot legitimately write. Do not turn an unexpected success
      // response into a connectivity claim or continue another operation.
      this.patch({ plan: null, issue: "unexpected-response", outcome: "error" });
    } catch (error) { this.failure(error, true); }
    finally { this.patch({ action: "idle" }); }
  };
  reject = async () => {
    if (this.disposed || !this.value.plan || this.value.action !== "idle") return;
    const plan = this.value.plan;
    this.patch({ action: "rejecting", confirmed: false, issue: null });
    try {
      const receipt = await this.consent.reject(plan.planId);
      if (receipt.planId !== plan.planId || receipt.state !== "rejected") throw { code: "operation-failed" };
      this.patch({ plan: null, outcome: "rejected" });
    } catch (error) { this.failure(error, true); }
    finally { this.patch({ action: "idle" }); }
  };
  private failure(error: unknown, consumed = false) {
    const issue = safeIssue(error);
    this.patch({ issue, confirmed: false, ...(consumed ? { plan: null } : {}), outcome: issue === "activation-disabled" ? "blocked" : ["expired-plan", "unknown-plan"].includes(issue) ? "expired" : issue === "stale-plan" ? "stale" : "error" });
  }
  dispose = () => { if (this.disposed) return; this.disposed = true; this.stopClock(); this.listeners.clear(); this.value = initialConnectionSnapshot(); };
}
