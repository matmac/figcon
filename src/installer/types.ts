/** Wire contract version 1; mirrors installer/api_models.rs. No private paths. */
export type Component = "node" | "mcp";
export type OperationState = "not-installed" | "installing" | "installed" | "failed" | "cancelled" | "blocked";
export type DiskState = "not-installed" | "installed" | "invalid";
export type Stage = "preparing" | "downloading" | "verifying" | "extracting" | "installing" | "validating" | "activating" | "completed" | "failed" | "cancelled";
export type ErrorCode = "busy" | "already-installed" | "runtime-required" | "attempt-limit" | "unsupported-platform" | "cancelled" | "deadline-exceeded" | "integrity-failed" | "resource-limit" | "validation-failed" | "operation-failed" | "not-active" | "not-owner" | "state-unavailable" | "unstable-snapshot";
export interface InstallationError { code: ErrorCode; message: string }
export interface InstallationEvent {
  revision: number;
  schemaVersion: 1;
  attemptId: string;
  component: Component;
  sequence: number;
  stage: Stage;
  status: "running" | "succeeded" | "failed" | "cancelled";
  downloadedBytes: number | null;
  totalBytes: number | null;
  retainedStaging: boolean;
  activated: boolean;
  error: InstallationError | null;
}
export interface OperationReport {
  cancellationRequested: boolean;
  attemptId: string;
  component: Component;
  state: OperationState;
  targetVersion: string;
  retainedStaging: boolean;
  activated: boolean;
  error: InstallationError | null;
  lastEvent: InstallationEvent | null;
}
export interface StartReceipt { attemptId: string; component: Component; state: OperationState; targetVersion: string }
export interface CancellationReceipt { attemptId: string; disposition: "requested" | "already-requested" | "too-late" }
export interface RetentionReport {
  state: "none" | "requires-review" | "unknown";
  attemptCount: number | null;
  orphanedLedgerFiles: number | null;
}
export interface ComponentReport {
  component: Component;
  state: OperationState;
  diskState: DiskState;
  targetVersion: string;
  installedVersion: string | null;
  lastOperation: OperationReport | null;
  error: InstallationError | null;
}
export interface ManagedInstallationReport {
  revision: number;
  supported: boolean;
  node: ComponentReport;
  mcp: ComponentReport;
  retention: RetentionReport;
}
/** Tauri rejects typed operational errors; transport/runtime failures remain unknown. */
export function isInstallationError(value: unknown): value is InstallationError {
  if (typeof value !== "object" || value === null) return false;
  const error = value as Record<string, unknown>;
  return typeof error.message === "string" && typeof error.code === "string" && errorCodes.includes(error.code as ErrorCode);
}
const errorCodes: readonly ErrorCode[] = ["busy", "already-installed", "runtime-required", "attempt-limit", "unsupported-platform", "cancelled", "deadline-exceeded", "integrity-failed", "resource-limit", "validation-failed", "operation-failed", "not-active", "not-owner", "state-unavailable", "unstable-snapshot"];
