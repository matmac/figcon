import type { InstallationEvent, InstallationError, ManagedInstallationReport } from "./types.ts";
import { isInstallationError } from "./types.ts";
import type { InstallationSubscription } from "./client.ts";

export interface ObservationSource {
  status(): Promise<ManagedInstallationReport>;
  subscribe(handler: (event: InstallationEvent) => void): InstallationSubscription;
}
export interface ObservationOptions {
  /** Presentation-only milestones; never use these to assert installed state. */
  onProgress?(event: InstallationEvent): void;
  onState(state: ManagedInstallationReport): void;
  onError?(error: InstallationError): void;
  /** Injectable scheduler for deterministic lost-event/polling tests. */
  schedule?: (refresh: () => void) => () => void;
}
export interface InstallationObservation extends InstallationSubscription { refresh(): Promise<void> }

/** Events invalidate state; only revision-checked backend snapshots are published.
 * Each observer owns its subscription, query lane, and two bounded stream cursors. */
export function observeInstallation(source: ObservationSource, options: ObservationOptions): InstallationObservation {
  let disposed = false, registered = false, dirty = false;
  let floor = 0, published = -1;
  let flight: Promise<void> | undefined;
  let stopPolling: (() => void) | undefined;
  const streams = new Map<string, { attemptId: string; sequence: number; revision: number }>();
  const unavailable: InstallationError = { code: "state-unavailable", message: "Managed installation state could not be inspected." };
  const unstable: InstallationError = { code: "unstable-snapshot", message: "Installation changed during inspection. Refresh installation state." };

  async function drain() {
    // Bound retries when notifications repeatedly invalidate slow inspections.
    for (let retry = 0; dirty && !disposed && retry < 3; retry++) {
      dirty = false;
      const state = await source.status();
      if (disposed) return;
      if (state.revision < floor || state.revision < published) { dirty = true; continue; }
      floor = Math.max(floor, state.revision);
      published = state.revision;
      // Seed cursors from the authoritative snapshot so an old queued event
      // cannot revive a previous attempt after mounting or switching attempts.
      for (const component of [state.node, state.mcp]) {
        const event = component.lastOperation?.lastEvent;
        if (event) streams.set(component.component, { attemptId: event.attemptId, sequence: event.sequence, revision: event.revision });
      }
      dirty = false; // This snapshot includes all notifications up to its revision.
      options.onState(state);
    }
    if (dirty && !disposed) throw unstable;
  }
  function refresh(): Promise<void> {
    if (disposed) return Promise.resolve();
    dirty = true;
    if (!registered) return Promise.resolve();
    if (!flight) {
      let succeeded = false;
      flight = drain().then(() => { succeeded = true; }).catch(error => {
        const safe = isInstallationError(error) ? error : unavailable;
        if (!disposed) options.onError?.(safe);
        throw safe;
      }).finally(() => {
        flight = undefined;
        // An event can arrive between the final drain iteration and this
        // continuation. Do not strand its invalidation until the next poll.
        if (succeeded && dirty && !disposed) void refresh().catch(() => {});
      });
    }
    return flight;
  }
  function notify(event: InstallationEvent) {
    if (disposed || event.schemaVersion !== 1 || event.revision <= floor) return;
    const previous = streams.get(event.component);
    if (previous && event.attemptId === previous.attemptId && event.sequence <= previous.sequence) return;
    if (previous && event.revision <= previous.revision) return;
    const gap = previous?.attemptId === event.attemptId ? event.sequence !== previous.sequence + 1 : event.sequence !== 1;
    streams.set(event.component, { attemptId: event.attemptId, sequence: event.sequence, revision: event.revision });
    floor = Math.max(floor, event.revision);
    options.onProgress?.(event);
    // Coalesced cursors are the initial event buffer. A gap/new attempt always
    // invalidates; contiguous notifications also refresh because they aren't state.
    if (gap) dirty = true;
    void refresh().catch(() => {});
  }
  const subscription = source.subscribe(notify);
  const ready = subscription.ready.then(async () => {
    if (disposed) return;
    registered = true;
    const schedule = options.schedule ?? (callback => {
      const timer = setInterval(callback, 2000);
      return () => clearInterval(timer);
    });
    // Poll even when idle: terminal events and other windows' notifications may
    // never arrive here. Never infer activity from retained staging counts.
    stopPolling = schedule(() => { void refresh().catch(() => {}); });
    await refresh();
  });
  return {
    ready, refresh,
    dispose() { if (disposed) return; disposed = true; stopPolling?.(); subscription.dispose(); streams.clear(); },
  };
}
