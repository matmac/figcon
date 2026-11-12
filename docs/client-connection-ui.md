# AI client connection review UI

The desktop app has two views: **Setup** and **AI clients**. Setup retains its
existing installation observer and controller at the application root while views
change. Navigation does not cancel installation, start another installer, approve
plans, or modify client settings. An in-progress indicator links back to setup.

## Discovery and prerequisites

Opening AI clients or choosing Refresh invokes passive client inspection and
recovery-capacity accounting. Separate Claude Code and Codex cards show detection,
registration, unsupported settings and conflicts. Matching external registrations
remain external: Figcon does not adopt them or claim live connectivity. Inspection
never starts either client or an MCP server. Detection limitations in
[client-inspection.md](client-inspection.md) still apply.

The existing installation observer supplies managed runtime/package prerequisites.
Missing, incomplete, installing or unavailable prerequisites prevent plan requests.
The backend independently checks prerequisites; frontend readiness is not authority.
Recovery storage full/unknown states also block plan requests. No recovery cleanup
is offered or enabled.

## Deliberate review and approval

1. Choose Review on an eligible client card to request a backend-owned plan.
2. Review the server name, user-wide scope, addition-only behavior, warnings and
   expiration. Expand Executable and argument details to see the exact backend
   executable and ordered arguments. No paths or commands are accepted as input.
3. Select the confirmation checkbox, then separately choose Confirm approval.
   This calls the approval contract with only the opaque plan ID.
4. The production backend gate remains closed. The UI displays **Changes are
   blocked in this version**, never a connection-success screen. Unexpected success
   responses are reported for review instead of being interpreted as connectivity.

Decline plan calls the rejection contract. Escape within the review section also
explicitly declines. Busy controls prevent duplicate submissions. Unknown plan
warnings or an unexpected enabled-plan response fail closed in the UI as well;
the Rust gates remain independent of frontend behavior.

The countdown uses the earlier of the backend wall-clock expiry and a monotonic
lifetime capped by the backend plan lifetime. Backend expiry/revalidation remains
authoritative. Expiration clears confirmation. Prerequisite changes invalidate
review. Changed configuration, consumed/expired plans and other approval errors
require another explicit review, with no automatic approval or retry.

Refresh, returning to this view, reload and restart obtain fresh inspection.
Locally abandoned plans are not approved or reused; their bounded backend records
expire normally. Unmount disposes the timer and subscriptions and ignores delayed
responses. There is no localStorage/sessionStorage persistence. Raw transport
messages, stacks and configuration contents are neither displayed nor logged.
Only allowlisted error/warning codes produce user-facing copy. Proposed managed
paths are intentionally shown inside technical details, never unrelated settings.

## Accessibility and validation boundaries

Native navigation buttons, labelled checkbox, form and details elements support
standard keyboard interaction. Review headings and result notices receive focus;
visible focus styling, wrapping paths and a narrow-window card layout share the
existing light/dark palette. Expiration does not announce every timer tick.

Deterministic frontend tests cover discovery, external/conflicting/malformed
settings, prerequisites, exact preview rendering, consent, duplicate requests,
expiry, stale configuration, rejection, capacity, sanitized errors, recovery,
disposal and native keyboard-control semantics. Existing installer and observer
tests remain in the same CI command (`npm run test:installer`). Native desktop qualification is recorded in
[the acceptance report](client-connection-ui-acceptance.md), including focused
control-painting and environment-loss fixes. Native keyboard navigation has since been exercised with macOS keyboard navigation
enabled. Full accessibility and VoiceOver certification remain outstanding.

Production configuration mutation is deliberately unavailable. This UI does not
resolve the architecture's activation gates, uncoordinated external-writer races,
receipt adoption or safe user-controlled recovery-file reclamation. See
[client-integration-architecture.md](client-integration-architecture.md).
