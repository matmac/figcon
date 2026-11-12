/** Read-only wire contract; deliberately no apply/remove/approval endpoint. */
export type AiClient = "claude-code" | "codex-cli";
export type DetectionStatus = "missing" | "detected" | "ambiguous" | "unsupported-platform";
export type ConfigurationStatus = "missing" | "readable" | "malformed" | "inaccessible" | "unsafe" | "unsupported-location" | "unsupported-platform";
export type RegistrationStatus = "not-registered" | "matching-external" | "external" | "conflict" | "unsupported" | "unknown";
export type OwnershipStatus = "absent" | "mismatched" | "unverified";
export type InspectionWarning = "registration-conflict" | "read-only" | "client-missing" | "client-ambiguous" | "unsupported-platform" | "unsupported-location" | "malformed-configuration" | "inaccessible-configuration" | "unsafe-configuration" | "unsupported-registration" | "additional-registration" | "possible-shadowing" | "effective-configuration-unknown" | "ownership-unverified" | "managed-installation-unavailable";
export interface ClientInspection {
  client: AiClient;
  clientName: string;
  detection: DetectionStatus;
  configuration: ConfigurationStatus;
  configurationLocation: string;
  registration: RegistrationStatus;
  ownership: OwnershipStatus;
  warnings: InspectionWarning[];
}
export interface ConfigurationPreview {
  inspection: ClientInspection;
  proposedServerName: string;
  proposedExecutable: string;
  proposedArguments: string[];
  scope: "user";
  existingRegistrationWouldBeAffected: boolean;
  approvalRequired: boolean;
  safeToApply: boolean;
  managedInstallationValid: boolean;
}
export interface ClientInspectionError { code: "inspection-unavailable"; message: string }

export type ConsentCode = "unsupported-platform" | "prerequisites-missing" | "client-unavailable" | "unsupported-configuration" | "conflict" | "stale-plan" | "expired-plan" | "unknown-plan" | "consumed-plan" | "not-owner" | "registry-capacity" | "recovery-capacity" | "activation-disabled" | "rejected" | "operation-failed" | "uncertain";
export interface ConsentError { code: ConsentCode; message: string }
export interface RecoveryUsage {
  state: "available" | "full" | "unknown";
  artifactCount: number | null;
  storageBytes: number | null;
  fileCount: number | null;
  availableDiskBytes: number | null;
  maxArtifacts: number;
  maxStorageBytes: number;
  maxFiles: number;
  reservedTransactionBytes: number;
  automaticCleanupEnabled: boolean;
}
export interface ConsentPreview {
  planId: string;
  client: AiClient;
  clientName: string;
  proposedServerName: string;
  proposedExecutable: string;
  proposedArguments: string[];
  scope: "user";
  operation: "add";
  registration: RegistrationStatus;
  summary: string;
  warnings: string[];
  expiresAtUnixMs: number;
  expiresInSeconds: number;
  productionMutationEnabled: boolean;
  recovery: RecoveryUsage;
}
export interface ApprovalReceipt { planId: string; state: "verified" | "rejected" }
