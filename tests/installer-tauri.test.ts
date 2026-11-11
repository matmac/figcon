import { test } from "node:test";
import assert from "node:assert/strict";
import { createTauriTransport } from "../src/installer/tauri.ts";
import type { InstallationEvent } from "../src/installer/types.ts";

test("production adapter supplies explicit WebviewWindow targets and isolates two simulated windows", async () => {
  const listeners: { target: { kind: string; label: string }; handler: (e: { payload: unknown }) => void }[] = [];
  const make = (label: string) => createTauriTransport({
    invoke: async <T>() => ({} as T), currentWindow: () => ({ label }),
    listen: async <T>(name: string, callback: (event: { payload: T }) => void, options: { target: { kind: "WebviewWindow"; label: string } }) => {
      assert.equal(name, "figcon://installation-progress");
      const listener = { target: options.target, handler: callback as (event: { payload: unknown }) => void }; listeners.push(listener);
      return () => { listeners.splice(listeners.indexOf(listener), 1); };
    },
  });
  const a: string[] = [], b: string[] = [];
  const removeA = await make("main").listen("figcon://installation-progress", e => a.push(e.attemptId));
  const removeB = await make("other").listen("figcon://installation-progress", e => b.push(e.attemptId));
  assert.deepEqual(listeners.map(l => l.target), [{ kind: "WebviewWindow", label: "main" }, { kind: "WebviewWindow", label: "other" }]);
  const emit = (label: string, attemptId: string) => {
    for (const listener of listeners) if (listener.target.kind === "WebviewWindow" && listener.target.label === label) listener.handler({ payload: { attemptId } as InstallationEvent });
  };
  emit("main", "a"); emit("other", "b"); assert.deepEqual(a, ["a"]); assert.deepEqual(b, ["b"]);
  removeA(); emit("main", "late"); removeB(); assert.equal(listeners.length, 0); assert.deepEqual(a, ["a"]);
});
