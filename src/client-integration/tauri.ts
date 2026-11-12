import { invoke } from "@tauri-apps/api/core";
import { createAiClientInspector } from "./client.ts";
export const aiClientInspector = createAiClientInspector(invoke);
export { createClientConsentApi } from "./consent.ts";
import { createClientConsentApi } from "./consent.ts";
export const clientConsent = createClientConsentApi(invoke);
