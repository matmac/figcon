import { observeInstallation } from "./observe.ts";
import type { ObservationOptions } from "./observe.ts";
import type { CancellationReceipt, Component, InstallationEvent, ManagedInstallationReport, StartReceipt } from "./types.ts";
export const INSTALLATION_EVENT = "figcon://installation-progress";
/** Injectable for transport tests; production transport is fixed in tauri.ts. */
export interface InstallerTransport {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen(event: string, handler: (event: InstallationEvent) => void): Promise<() => void>;
}
export interface InstallationSubscription { ready: Promise<void>; dispose(): void }
export function createInstallerClient(transport: InstallerTransport) {
  const client = {

    installNode: () => transport.invoke<StartReceipt>("install_managed_node"),
    installMcp: () => transport.invoke<StartReceipt>("install_managed_mcp"),
    cancel: (attemptId: string) => transport.invoke<CancellationReceipt>("cancel_managed_installation", { attemptId }),
    status: () => transport.invoke<ManagedInstallationReport>("get_managed_installation_state"),
    /** Subscribe before invoking install; disposal never implicitly cancels work.
     * Unfiltered streams need no attempt history; filtered streams reject stale sequences. */
    subscribe(handler: (event: InstallationEvent) => void, filter?: { attemptId: string; component?: Component }): InstallationSubscription {
      let disposed = false;
      let unlisten: (() => void) | undefined;
      let sequence = 0;
      const ready = transport.listen(INSTALLATION_EVENT, event => {
        if (disposed || event.schemaVersion !== 1) return;
        if (filter && (event.attemptId !== filter.attemptId || (filter.component && event.component !== filter.component))) return;
        if (filter) {
          if (event.sequence <= sequence) return;
          sequence = event.sequence;
        }
        handler(event);
      }).then(remove => { if (disposed) remove(); else unlisten = remove; });
      return { ready, dispose() { disposed = true; unlisten?.(); unlisten = undefined; } };
    },
  };
  return { ...client, observe: (options: ObservationOptions) => observeInstallation(client, options) };
}
