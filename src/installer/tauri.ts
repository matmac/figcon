import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { createInstallerClient } from "./client.ts";
import type { InstallerTransport } from "./client.ts";
import type { InstallationEvent } from "./types.ts";

interface TauriAdapter {
  invoke: InstallerTransport["invoke"];
  currentWindow(): { label: string };
  listen<T>(event: string, handler: (event: { payload: T }) => void, options: { target: { kind: "WebviewWindow"; label: string } }): Promise<() => void>;
}
/** This is the production adapter, injectable to test the actual target arguments.
 * Targeting is routing for cooperating windows, not an authorization boundary. */
export function createTauriTransport(api: TauriAdapter = { invoke, listen, currentWindow: getCurrentWebviewWindow }): InstallerTransport {
  return {
    invoke: api.invoke,
    listen: (event, handler) => api.listen<InstallationEvent>(event, ({ payload }) => handler(payload), {
      target: { kind: "WebviewWindow", label: api.currentWindow().label },
    }),
  };
}
export const installer = createInstallerClient(createTauriTransport());
