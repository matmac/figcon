# Managed installation API

The Tauri API wraps the existing production installers. The [desktop setup UI](installer-ui.md)
uses this API and observer. The API never reads or writes external AI client MCP
configuration. It accepts no filesystem path, executable, version, architecture,
registry URL, environment, or shell arguments. Production roots come only from
Tauri app-data resolution. macOS arm64 and x64 are supported; other platforms
reject installation before filesystem changes. Windows retains compilation gates.

## Commands and concurrency

- `install_managed_node()` starts pinned Node.js 24.22.2.
- `install_managed_mcp()` starts pinned figma-console-mcp 1.46.0; a structurally
  valid private runtime is required first and executable Node/npm probes still
  run in the installer before npm ci.
- `cancel_managed_installation({ attemptId })` requests cooperative cancellation.
- `get_managed_installation_state()` returns a read-only structural snapshot.

Start commands reserve one operation and return an `installing` receipt with a
random 128-bit attempt ID and target version. The same ID is used in the durable
receipt if staging initialization succeeds. Acceptance is not installation
success: a worker can subsequently fail preflight or cross-process admission.
Only one operation, across **both** components, may be active per app process.
Duplicate/overlapping requests return a typed `busy` error; they are not queued.
Install Node, wait for completion, then request MCP. The slot remains occupied
until the worker returns and terminal history is published, including cancellation.
Event delivery follows publication and does not hold admission or state locks.

A separate private `metadata/operation.lock` is held throughout each API worker
using macOS flock. Overlapping API operations in another cooperating Figcon
process fail with `blocked`/`busy`. Existing short-lived cross-process admission
locking still serializes attempt counting/allocation; all installer entry points
retain integrity, dependency validation, and atomic no-replace activation. Direct
internal installer tests do not acquire the API operation lock; they retain the
existing admission and activation safeguards. Lock files are never removed.

## Events and status

`figcon://installation-progress` is emitted to the originating webview window.
The production adapter explicitly subscribes with the matching `WebviewWindow`
target and current label. Targeting routes cooperating listeners; it is not an
authorization or confidentiality boundary (global Tauri listeners can observe
targeted events). Cancellation ownership is enforced separately in Rust.
Rust types are in `api_models.rs`; corresponding TypeScript types and the client
are in `src/installer/`. Schema version 1 uses camelCase fields and kebab-case
values:

```ts
{
  schemaVersion: 1,
  revision: number, // monotonic within this backend process; not a timestamp
  attemptId: string,
  component: "node" | "mcp",
  sequence: number, // monotonic per attempt, starting at 1
  stage: "preparing" | "downloading" | "verifying" | "extracting" |
         "installing" | "validating" | "activating" | "completed" |
         "failed" | "cancelled",
  status: "running" | "succeeded" | "failed" | "cancelled",
  downloadedBytes: number | null,
  totalBytes: number | null,
  retainedStaging: boolean,
  activated: boolean,
  error: { code: string, message: string } | null
}
```

Milestones are emitted on entering real backend operations; they are not proof
that that stage succeeded. Validation/preparation can occur repeatedly. Download
bytes count Node archive bytes successfully written; the optional total comes
from HTTP Content-Length and is informational until checksum verification.
Updates are throttled to approximately 100 ms, with initial/final byte events.
npm progress is stage-only: no dependency byte count or percentages are invented.
Exactly one terminal event closes each settled attempt stream; late callbacks
are ignored. Raw Rust errors, npm output, environment/config contents, and paths
are never returned through this API. A closed error-code mapping produces static
messages; some internal failures intentionally map to `operation-failed` rather
than exposing internal diagnostic text.

Status separates `diskState` (`not-installed`, `installed`, `invalid`) from
operational `state` and `lastOperation`. Failed/cancelled/blocked operations remain
observable even when no destination exists. If activation succeeded but a later
checkpoint failed, `lastOperation` is failed with `activated: true`, while valid
disk state is installed. `installedVersion` is the structurally validated pinned
target, not a fresh executable/version or cryptographic attestation.
Retention diagnostics are separate: `none`, `requires-review`, or `unknown`,
with attempt/orphan counts when inspection succeeds. They describe all existing
staging candidates conservatively, including live ones. Eight retained attempts
block further allocations; status exposes `blocked`/`attempt-limit` for components
not installed and not currently installing. Unknown inspection never authorizes
cleanup. Read snapshots can race ongoing installation and should be refreshed.

## Revision-checked snapshots and observation

One shared process-local revision clock serializes UI-relevant mutations:
admission, progress/last event, staging allocation reporting, activation reporting,
error hints, accepted cancellation, terminal history, and active-slot removal.
Events and successful state responses include the revision. Operation reports
also expose `cancellationRequested`, independently of their operational outcome.
The counter is process-local and resets on backend restart; it is not comparable
across app sessions or processes. Existing per-attempt sequences remain separate.

`get_managed_installation_state` captures the clock before disk inspection,
performs bounded-worker inspection off the async executor, then checks the clock
again while merging operation history. A changed revision discards the entire
inspection and retries, for at most **three** inspections. Exhaustion rejects with
the sanitized typed error `unstable-snapshot`; no unstable report is returned as
successful state. Clock comparison and history merge share the revision lock;
installation mutation cannot slip between those steps. A response is current
at that merge point, not necessarily when IPC delivers it.

This is **not an atomic filesystem snapshot**. Root/runtime/MCP/retention are
independent observations; other processes, accidental filesystem changes, and
the rename-to-activation-reporting interval are not transactions under this
clock. Existing filesystem validation and no-replace activation remain necessary.
A process doing continuous progress work may prevent a stable inspection; callers
must retain their last good observation and surface refresh errors.

Use the high-level client observer instead of separately querying then listening:

```ts
const observation = installer.observe({
  onState: report => { /* render a backend snapshot */ },
  onError: error => { /* show a sanitized refresh error */ },
});
await observation.ready;
// On unmount:
observation.dispose();
```

The observer registers a listener first and awaits registration before querying.
Events received before/during the initial query are coalesced into bounded
per-component attempt/sequence/revision cursors and a revision high-water mark.
They are notifications, never installed/failed state assertions. A query older
than any received notification or previously published response is discarded.
New attempts and sequence gaps invalidate state; the observer reconciles through
a fresh backend query. Old events cannot revive an earlier attempt. At most one
query is in flight per observer, with at most three inspections of stale responses
per refresh burst. Backend `unstable-snapshot` and transport errors are surfaced;
query failures are not interpreted as installation failures.

A **two-second polling interval**, also when idle, reconciles completely missed
terminal events and activity originating in other windows/processes. Polling can
only report those other processes' disk/retention observations, not their private
operation history. Timing is best effort and includes IPC and inspection latency.
Explicit `refresh()` is available. Remounting creates a new subscription and fresh
initial query; there is no persisted frontend state. Equal revisions can still
carry new disk observations from another process and are eligible for publication.
Listeners/timers are disposed exactly once; disposal while registration/querying
is pending suppresses callbacks and removes the listener once registered. It
never cancels installation or an already submitted backend inspection. `ready`
rejects if registration or initial reconciliation fails; dispose on unmount even
in that case. After an initial query failure, polling can still recover.

The observer also accepts an optional `onProgress` callback for filtered, actual
backend milestones. It is presentation-only; it must not drive installed state or
component sequencing. Newer snapshot milestones take precedence when notifications
were missed. The callback follows subscription lifetime and disposal.

The low-level `subscribe`/`status` methods remain available but do not implement
this reconciliation protocol on their own. Subscribe before start commands:
early notifications can precede their response. Detailed operation history is
bounded to the active operation and last operation per component in this app
process. After restart, durable receipts inform retention, not a replayable UI
event history. Retention counts are never evidence that an attempt is active.

Progress records are queued while updating state, then one drainer delivers them
in per-attempt sequence order **without holding service, revision, or progress
mutexes**. Reentrant listeners may synchronously inspect state or enqueue another
notification. Terminal history and active-slot removal precede terminal delivery.
Delivery across different attempts need not follow global revision order; the
observer's revision high-water mark handles delayed older notifications. Event
delivery failure never rolls back installation; queries and polling repair missed
notifications. Listener callbacks should return promptly; a blocked native
callback can delay delivery but cannot hold the installer state locks.

## Cancellation and security boundaries

Only the originating window can cancel its active attempt. Results are
`requested`, `already-requested`, or `too-late`; a request is not completion.
Cancellation propagates through the existing deadline token, blocking scans and
hashing, async download waits, and supervised private subprocess groups. Nested
installer calls execute within their existing worker rather than spawning a
worker that could outlive API admission. Filesystem work remains off the Tauri
async executor. Cancellation is cooperative; blocking kernel calls cannot be
preempted. Process supervision retains owned-group signalling protections and
reports inability to supervise through failure/retention policies.

The cancellation token and atomic no-replace rename share a mutex. A cancellation
accepted first prevents activation; once rename commits, cancellation is too late
and never relabels the activated installation as cancelled. The monotonic overall
600-second deadline still applies before activation; timeouts are failures, not
explicit user cancellation. A post-activation checkpoint error can report failed
with `activated: true`. App shutdown/crash may prevent terminal delivery; read-only
status and non-destructive recovery remain the source of restart diagnostics.

**Automatic staging deletion remains disabled in every outcome.** Allocated
staging is marked retained in progress and terminal reports, including successful
installation. Receipts, cache/temp/home, and partial downloads are retained.
Manual review and a future safe user-controlled reclamation flow are still
required; the eight-attempt cap includes successes. See
[recovery policy](installer-recovery.md) for resource limits, filesystem races,
process supervision, and the cooperative same-user threat boundary.
No privileged helper, new dependency, arbitrary frontend process execution, or
external MCP configuration integration is introduced. Pinned SHA-256, committed
lockfile integrity, private npm, disabled scripts, quotas, deadlines and exclusive
activation are unchanged. npm ci does not provide complete supply-chain security;
hostile same-user isolation, hard aggregate disk containment, power-loss atomicity
of payload plus journal, and reauthentication of installed payloads remain outside
the guarantees.

## Validation

Ordinary Rust tests cover lifecycle/ordering, duplicate and concurrent requests,
worker event isolation, typed errors, cancellation, retention, activation boundary,
production MCP execution with synthetic private npm, and independent-process
operation locking. macOS-only tests are gated. Frontend transport tests run with
`npm run test:installer` on Node.js 24 (also in macOS/Windows CI) and test command arguments,
explicit production adapter targets, two simulated windows, registration races,
delayed queries, sequence gaps, missed terminals, remounts, successive attempts,
bounded unstable retries, filtering, disposal, and observable errors. Rust tests
pause a real fixture installation during inspection, exhaust snapshot retries,
and synchronously query/reenter event dispatch without sleeps. The TypeScript build
checks the production client adapter. The opt-in isolated network E2E uses the
same API dispatcher for successful Node and MCP installation and asserts ordered
attempt-scoped real-byte progress; it does not launch a webview or run MCP against
Figma. See [E2E instructions](installation-e2e.md).
