# Desktop setup interface

Figcon opens directly into one setup screen. The initial observer subscription
and revision-checked snapshot must succeed before setup is enabled. The primary
“Set up Figma Console” button confirms a download into Figcon-managed storage.
There are no shell instructions, path inputs, or client-configuration writes.

The React screen uses the existing typed API and `installer.observe`. Its small
`SetupSession` adapter stores only the current backend report, presentation-only
progress, local command receipts, errors, and the user's current setup/cancel
intent. It does not infer installed state from progress events or maintain a
second installation lifecycle. Node runs first. Its terminal operation and valid
disk state must be observed before MCP is submitted. Every step checks the latest
available report and retained-attempt cap; backend admission and validation remain
the final authority. Already valid components are skipped. Invalid active payloads
block setup and require manual review; repair/replacement is not implemented.

The observer's optional `onProgress` callback supplies real, filtered milestones
for display while snapshots are changing. Those events never advance the setup
sequence or assert success. Newer snapshot milestones supersede older presentation
notifications. Download bars represent only actual bytes against the backend's
optional HTTP length, with a byte count and no invented overall percentage.

Cancellation is available beside progress. An accepted receipt permits a request
even before the first stable active snapshot arrives. “Cancellation requested”
remains distinct from a terminal cancelled report. A request prevents this window
from automatically launching the next component, even if activation is already
too late to cancel. Backend window ownership and activation protections still
apply. Finishing/activated operations do not offer cancellation. Duplicate setup
requests are guarded synchronously before any await.

Unmount/reload disposes the observer and UI waiters, not the production installer.
A newly mounted screen follows any active backend operation using authoritative
snapshots and polling. After that operation finishes, the user chooses “Continue
setup” to launch remaining components. The UI does not persist an automatic
installation plan across reloads or backend restarts. Status refresh failures,
including unstable snapshots, preserve the last observation and offer “Check
again”/“Refresh status”; they are not installation failures.

Retained setup files are shown separately, including after success. Eight retained
attempts block further setup, and the cap is rechecked between Node and MCP.
No reclamation command exists and no files are automatically deleted. A safe,
user-controlled review/reclamation workflow remains follow-up work.

Success means both managed components passed structural status inspection. It
explicitly says AI client configuration and Figma plugin onboarding are separate
upcoming steps. It does not run MCP against Figma or claim client connectivity.

The interface uses native buttons, ordered steps, visible keyboard focus, polite
stage announcements, numeric download progress, OS light/dark appearance, and a
reduced-motion variant. Small windows scroll; key cancellation controls are
inside the progress card. Error messages use a closed designer-facing mapping;
raw transport strings and sensitive filesystem paths are not rendered.

`npm run test:installer` includes the existing API/reconciliation tests plus setup
sequencing, cancellation, retry, reload, missed events, attempt caps, incomplete
payloads, and server-rendered UI assertions. The test-only JSX loader uses the
already installed TypeScript compiler and writes no generated modules. These are
controller and rendered-markup tests, not a complete DOM/browser interaction or
assistive-technology certification. Live downloads remain covered by the opt-in
isolated backend E2E. See [the API contract](installation-api.md).

## Real desktop acceptance

A real macOS arm64 Tauri desktop acceptance run installed Node.js 24.22.2,
private npm 11.19.1, and Figma Console MCP 1.46.0 in an isolated temporary
application-data root. Real progress, Node-before-MCP sequencing, active-window
reload, process restart, checksum failure, retry, cancellation during preparation,
and existing-installation preservation were exercised. Light/dark appearance
and keyboard focus were inspected. No simulated success or client configuration
action was used. Cancellation during npm execution remains untested in the native
desktop acceptance run; release packaging and full accessibility certification
also remain untested.

The external configuration investigation recovered exact before/after versions
using the recorded SHA-256 hashes. Only Claude Code's `promptQueueUseCount`
changed from 25 to 26. MCP registrations and Figma Console configuration were
byte-identical; the entire file differed only in that counter. Existing Claude
processes are a plausible explanation, but writer attribution was not conclusively
established. No sensitive configuration contents or machine-specific evidence
paths are included here. A subprocess regression uses a synthetic temporary home
and checks five external client configuration files byte-for-byte after successful,
failed, and cooperatively cancelled production MCP installer calls with synthetic
private runtime fixtures. It never reads or writes the real user configuration.
