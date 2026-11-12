import type { AiClient, ClientInspection, ConfigurationPreview } from "./types.ts";
export type ClientInvoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
export function createAiClientInspector(invoke: ClientInvoke) {
  return {
    list: () => invoke<ClientInspection[]>("list_ai_clients"),
    inspect: (client: AiClient) => invoke<ClientInspection>("inspect_ai_client", { client }),
    preview: (client: AiClient) => invoke<ConfigurationPreview>("preview_ai_client_configuration", { client }),
  };
}
