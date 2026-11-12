import { test } from "node:test";
import assert from "node:assert/strict";
import { createAiClientInspector } from "../src/client-integration/client.ts";
import type { ConfigurationPreview } from "../src/client-integration/types.ts";
test("client inspection transport exposes only fixed read-only commands and closed client IDs", async () => {
  const calls: unknown[] = [];
  const api = createAiClientInspector(async <T>(...args: unknown[]) => { calls.push(args); return {} as T; });
  await api.list(); await api.inspect("claude-code"); await api.preview("codex-cli");
  assert.deepEqual(calls, [["list_ai_clients"], ["inspect_ai_client", { client: "claude-code" }], ["preview_ai_client_configuration", { client: "codex-cli" }]]);
  assert.deepEqual(Object.keys(api).sort(), ["inspect", "list", "preview"]);
});
test("read-only preview preserves backend safety flags; typed errors propagate", async () => {
  const preview = { safeToApply: false, approvalRequired: true, existingRegistrationWouldBeAffected: false } as ConfigurationPreview;
  const api = createAiClientInspector(async <T>() => preview as T);
  assert.equal((await api.preview("claude-code")).safeToApply, false);
  const issue = { code: "inspection-unavailable", message: "Client configuration could not be inspected." };
  const failing = createAiClientInspector(async () => { throw issue; });
  await assert.rejects(failing.inspect("codex-cli"), error => error === issue);
});

test("consent preparation never approves; approval accepts only its opaque plan ID", async () => {
  const { createClientConsentApi } = await import("../src/client-integration/consent.ts");
  const calls: unknown[] = [];
  const api = createClientConsentApi(async <T>(...args: unknown[]) => { calls.push(args); return {} as T; });
  await api.prepare("claude-code");
  assert.deepEqual(calls, [["prepare_client_configuration", { client: "claude-code" }]]);
  await api.approve("opaque"); await api.reject("other"); await api.recovery();
  assert.deepEqual(calls.slice(1), [["approve_client_configuration", { planId: "opaque" }], ["reject_client_configuration_plan", { planId: "other" }], ["get_client_configuration_recovery"]]);
});
test("production gate errors propagate; no fallback or automatic retry is performed", async () => {
  const { createClientConsentApi } = await import("../src/client-integration/consent.ts");
  const issue = { code: "activation-disabled", message: "Production client configuration changes are disabled pending activation review." };
  let attempts = 0;
  const api = createClientConsentApi(async () => { attempts++; throw issue; });
  await assert.rejects(api.approve("opaque"), error => error === issue);
  assert.equal(attempts, 1);
});
