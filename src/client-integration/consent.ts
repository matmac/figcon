import type { ClientInvoke } from "./client.ts";
import type { AiClient, ConsentPreview, ApprovalReceipt, RecoveryUsage } from "./types.ts";
/** Contract only. Calling approve is the explicit approval action; never call it
 * from detection, refresh, installation, startup or plan preparation. The Rust
 * production gate remains closed independently of any frontend setting. */
export function createClientConsentApi(invoke: ClientInvoke) {
  return {
    prepare: (client: AiClient) => invoke<ConsentPreview>("prepare_client_configuration", { client }),
    approve: (planId: string) => invoke<ApprovalReceipt>("approve_client_configuration", { planId }),
    reject: (planId: string) => invoke<ApprovalReceipt>("reject_client_configuration_plan", { planId }),
    recovery: () => invoke<RecoveryUsage>("get_client_configuration_recovery"),
  };
}
