# Native AI client integration acceptance

## Fresh isolated acceptance — 2026-11-12

**PASS for the exercised macOS arm64 flows; ready for PR #6 checkpoint review.**
Production configuration mutation remains disabled. This is not qualification for
production writes, release packaging or full accessibility certification. The
previous report and its earlier isolation incident are preserved below; this fresh
run had no observed unguarded/default-production launch.

### Environment and evidence

The documented fixture script and `FIGCON_NATIVE_ACCEPTANCE=1` /
`FIGCON_NATIVE_ACCEPTANCE_ROOT` / `npm run tauri dev` launch succeeded. Native
control required a temporary app wrapper containing the identical debug executable
and explicit acceptance variables in its launch metadata, with standalone Vite
serving the real frontend. No simulated backend or installer was substituted.
The debug missing-environment startup guard remained active. Tests ran serially
because native automation could not reliably select two debug copies simultaneously.

Main and cancellation runs used separate private temporary acceptance roots on this Mac; exact paths are intentionally omitted.
Each root contains private `home`, `bin`, `data` and `evidence` directories. Client
candidates/configurations and managed paths came only from the selected environment;
PATH and fixed production candidates were not added. Webview storage was ephemeral.
Evidence includes **51 main and six cancellation screenshots**, AX observations,
fixture snapshots/hashes, process samples and test logs. Each root's
`evidence/screenshots.json` lists its images. Screenshots remain outside Git.

### Actual native scenarios

| Scenario | Result |
| --- | --- |
| Fresh Setup and missing prerequisites | PASS: no existing runtime/MCP; both missing synthetic clients remained missing; previews unavailable. |
| Real installation | PASS: production downloader, checksum, extraction, private npm and lockfile installation produced Node **24.22.2**, npm **11.19.1**, MCP **1.46.0**. No MCP server was launched. |
| Navigation during installation | PASS: Setup → AI clients → Setup retained real milestones, byte progress and Cancel. |
| Reload during installation | PASS: native context-menu Reload reconstructed Node installed and MCP “Verifying setup” / “In progress”, with Cancel still available. This was observed in the native AX response; screenshot 16 was captured shortly afterward, when installation had completed, and alone does not prove active reload. |
| Restart and existing installation | PASS: explicit isolated restart reconstructed installed components and retained attempts, with no duplicate installation or resumed approval. |
| npm cancellation and retry | PASS in the second root: byte-verified copy of the real private runtime, no MCP; process sampling observed actual `npm ci` before native Cancel. UI reached “Setup stopped”, retained staging, preserved runtime, and did not activate MCP. Retry installed MCP 1.46.0. See secondary `result.json` and screenshot 04. This is sampled process evidence, not exact syscall timing. |
| Client detection | PASS: both missing, Claude alone detected, then both detected using passive synthetic candidates. |
| Existing external/conflicting registration | PASS: matching Claude registration remained external; conflicting Codex registration blocked review; neither was adopted or overwritten. |
| Malformed/unsupported configuration | PASS: malformed JSON and unknown TOML registration fields blocked review safely. |
| Missing/inaccessible configuration | PASS: missing Claude file supported planning but remained absent after blocked approval; mode-000 Codex file blocked review. |
| Backend-owned plans | PASS for both clients: exact root-local Node and MCP `dist/local.js`, `figcon-figma-console`, user scope, addition-only behavior and warnings. Expanded native details verified paths. |
| Unrelated data | PASS in inspected UI/AX: synthetic unrelated sentinel absent; no unrelated settings or credentials displayed. |
| Consent and duplicates | PASS: approval disabled until deliberate confirmation; rapid double-click reached one terminal blocked outcome, removed the plan and never claimed connection. Exact IPC count was not established. An attempted diagnostic wrapper could not replace the immutable Tauri bridge, so its zero counter is invalid evidence; automated tests cover deduplication. |
| Decline | PASS: keyboard Escape rejected the plan, removed review and focused the result. |
| Expiration | PASS: real five-minute expiry cleared confirmation and disabled approval. Native UI prevented expired submission; backend expired-ID rejection is automated coverage. |
| Stale plan | PASS: intentional synthetic config change after preview yielded “Settings changed since this preview”; changed bytes remained intact. |
| Recovery capacity | PASS: 32 private synthetic retained directories blocked previews with a no-automatic-deletion warning. |
| Backend error and retry | PASS: intentionally invalid marker failed closed with sanitized errors; restoring exact marker bytes and Refresh recovered without production fallback. |
| Keyboard and focus | PASS for keyboard navigation between Setup/AI clients and review controls, Tab/Shift-Tab, Return, Space confirmation and Escape decline. macOS keyboard navigation was temporarily enabled and restored to its original off setting. Native labels/headings/result focus were inspected. |
| Appearance | PASS: light and dark native screens readable; original Auto preference restored. |
| Resizing | Native window zoom passed. Fresh narrow drag was blocked by automation errors, so no fresh narrow-layout pass is claimed. Earlier genuine narrow-layout/path-wrapping screenshots remain separate evidence for unchanged CSS. |
| Accessibility | Inline review is not a modal; no focus trap expected. Visible focus and AX labels inspected. Full VoiceOver audio/error-announcement behavior remains untested. |

Representative main images include `01-fresh.jpg`,
`21-real-plan-expired.jpg`, plan/blocked/stale
states, light/dark screens, and restarted Setup/clients. The manifests are the
canonical filenames. Native Web Inspector showed no application exception at
inspection points; this was not continuous console monitoring.

### Integrity, isolation and retention

Main `evidence/final-integrity.json` confirms both synthetic configurations are
byte-identical to originals and runtime/MCP trees retain baseline content hashes,
inodes, modes, sizes and symlink targets. Phase-specific before/after checks cover
intentional fixture changes separately. Secondary `result.json` confirms pristine
configs, unchanged runtime, retained cancellation staging and successful retry.
Real user configs were not used, copied or read for hashing.

Each root retains two real installation attempts. All 32 test-authored empty
capacity fixtures remain under main `evidence/capacity-fixture-retained`; their
manual relocation by the harness is not application cleanup. Automatic staging
and recovery deletion remain disabled. No production mutation, receipt or backup
publication occurred through the consent UI.

No client CLI or MCP server command was issued. Process sampling observed expected
private Node/npm operations; shutdown metadata found no protected client-config
handles and no remaining executable under either root. Both apps and the owned
Vite listener were stopped, with artifacts preserved (`shutdown.json`). Sampling
has gaps and cannot prove absence of every transient open/exec. Source injection,
fail-closed tests and observed synthetic paths support application-level isolation;
this is not an exhaustive OS access audit or a claim that every macOS framework
bookkeeping path is confined.

After explicit restart a previously set nonsecret localStorage probe returned
`null`, confirming observed nonpersistent storage; inspected viewport overflow was
false (`56-restarted-storage.ax.txt`). Original keyboard and appearance preferences
were restored (`settings-restoration.json`).

### Changes, validation and readiness

No additional product source fix was needed in this fresh pass. The previous
focused button-theme, debug environment-loss guard and atomic regression-readiness
fixes remain preserved and covered. Only acceptance documentation changed here.

Fresh ordinary validation: **192 Rust tests passed, seven opt-in tests ignored;
49 frontend tests passed**. Rust formatting and diff whitespace checks passed.
The real desktop debug build/launch succeeded. Locked backend check, frontend
production build and macOS desktop build without bundling passed in the preceding
unchanged-source validation; they were not rerun solely for this documentation
update. No Windows execution was performed.

No new functional blocker was found for PR #6 checkpoint review. Follow-ups:
full VoiceOver/accessibility certification, release signing/notarization/packaging,
Windows execution, exhaustive filesystem/exec auditing and separately approved
production-write activation. Native exact IPC counting and fresh narrow drag
remain harness limitations, not claimed passes. Production writes stay blocked.

<details>
<summary>Earlier acceptance, isolation incident and remediation history</summary>

# Native AI client integration acceptance

## Current verdict

**Qualified functional pass after focused fixes; ready for PR #6 checkpoint review.**
Production configuration mutation remains disabled. This is not production-write
activation, release-package qualification or full accessibility certification.

All core installation/client-review flows below ran in the real macOS arm64 Tauri
application. There was an isolation incident during an automation-driven restart,
described explicitly below. Therefore this report does **not** claim the entire
pre-fix session had zero production-path access. The affected startup path was
fixed, regression-tested with a real debug executable, and the updated application
was successfully restarted against the isolated data. No known functional blocker
remains after those fixes; stricter whole-session filesystem auditing remains a
separate qualification limitation.

## Environment and evidence

- Branch: `feat/client-integration`; macOS arm64, development Tauri/WKWebView.
- Dedicated private temporary root on the acceptance Mac; the machine-specific path is intentionally omitted.
- Synthetic client search: `<root>/bin`; configurations: `<root>/home/.claude.json`
  and `<root>/home/.codex/config.toml`; application data: `<root>/data`.
- Initially no managed runtime/MCP and both synthetic candidates renamed aside.
  Fixtures came from `scripts/prepare-native-acceptance.py`; no real config copies.
- The documented `npm run tauri dev` launch succeeded. Native automation could not
  select its unbundled executable, so an identical debug binary was copied into a
  temporary `.app` inside the root, with the same explicit acceptance variables.
  A standalone Vite server supplied the actual frontend. No mock backend/UI states
  or alternate installer implementation were used. Binary identity is recorded in
  `evidence/final-binary-identity.json`.
- Screenshots, AX observations, before/after fixtures, checksums, process samples,
  diagnostics and retained helper scripts are under `<root>/evidence`. There are
  **25 native screenshots**, listed in `evidence/screenshots.json`; none are added
  to the repository. Test-owned app and Vite listener were stopped afterward;
  no evidence, installations or staging were deleted.

## Actual native scenarios

| Scenario | Result and evidence |
| --- | --- |
| Fresh Setup; missing prerequisites | PASS: not-installed state, previews unavailable; `01-fresh-setup.png`, `02-missing-clients-prerequisites.png` |
| Live installation across navigation | PASS: actual byte progress, Setup → clients → Setup retained the operation and Cancel; `03-runtime-downloading.png`, `04-installation-across-navigation.png` |
| Cancellation and retry | PASS: cancelled during Node verification, terminal “Setup stopped”, retained staging, retry succeeded; `05-cancelled-retained.png` |
| Real managed installation | PASS: Node 24.22.2, private npm 11.19.1, MCP 1.46.0 through production installer/checksum/lockfile validation; no MCP server launched |
| Native reload | PASS after completion: context-menu Reload reconstructed installed state; `06-real-installed-after-reload.png`. Active reload was not conclusively observed: installation completed before the native Reload action executed; an earlier Cmd-R attempt was not counted as proof |
| Application restart | PASS with explicit isolated launch, including the updated guarded debug build: installed status, three retained attempts, fresh client inspection and no resumed approval; `25-installed-after-restart.png`, `28-final-client-overview.png` |
| Both clients missing; Claude detected; Codex detected | PASS: missing candidates stayed missing, then only synthetic Claude and subsequently both candidates detected |
| Existing external and conflicting registrations | PASS: matching Claude entry remained external; conflicting Codex definition blocked preview, neither adopted/overwritten; `12-external-and-conflicting.png` |
| Malformed and unsupported settings | PASS: malformed Claude JSON and an unknown Codex registration field blocked review safely; `13-malformed-and-unsupported.png` |
| Missing and inaccessible configuration | PASS: missing Claude file stayed absent and supported planning; mode-000 synthetic Codex config blocked review; `14-missing-and-inaccessible.png` |
| Backend-owned previews | PASS for both clients: `figcon-figma-console`, exact private Node executable and MCP `dist/local.js`, user scope, addition-only behavior and all warnings; `07-exact-isolated-plan.png`, `23-narrow-exact-path-wrapping.png` |
| Unrelated configuration disclosure | PASS in observed UI/AX: synthetic unrelated sentinel absent; only static status and proposed managed paths shown |
| Explicit consent | PASS: approval disabled until checkbox confirmation; separate approval returned backend-disabled result, never connected/success; `08-blocked-approval.png` |
| Rapid duplicate input | PASS at UI level: double-click produced one terminal blocked outcome and removed the plan. Exact IPC request count was not traced; automated controller/backend tests cover deduplication |
| Decline | PASS: keyboard Escape called rejection, removed review and showed “Plan declined”; `10-plan-declined.png` |
| Real plan expiration | PASS: actual five-minute expiry cleared prior confirmation and disabled checkbox/approval; `16-real-expiration.png`. No clock spoofing; UI prevented expired submission. Backend expired-ID rejection is automated coverage rather than a native direct-call result |
| Changed configuration after preview | PASS: intentional synthetic file edit caused “Settings changed since this preview”, discarded approval and required a new review; `11-stale-configuration-rejected.png` |
| Recovery capacity | PASS: 32 private synthetic retained directories blocked both previews and reported no automatic deletion; `19-capacity-limit.png` |
| Backend failure and refresh/retry | PASS: deliberately invalid acceptance marker yielded sanitized inspection/prerequisite/recovery errors; exact marker restoration and Refresh recovered; `22-backend-validation-error.png` |
| Light/dark appearance | PASS after control-painting fix; fresh light→dark transition with enabled controls verified; `17-light-after-control-fix.png`, `18-dark-enabled-controls-fixed.png`. Original Auto preference restored |
| Window resizing | PASS: native resize, narrow single-column cards, wrapping paths and details without observed clipping; `20-half-width-layout.png`, `21-narrow-single-column.png`, `23-narrow-exact-path-wrapping.png` |
| Keyboard/focus/accessibility | PARTIAL: native Tab to details/checkbox, Space confirmation and Escape decline passed; review heading/result focus and visible outlines observed. Complete keyboard-only navigation from a newly restarted window was not established with this automation/macOS focus configuration. Native AX exposes control names/headings/checkbox label. No VoiceOver audio/announcement certification |
| Modal/dialog focus | N/A: review is inline, not a modal. Heading/result focus was inspected; no modal focus trap is claimed |
| Error announcements | Alert markup exists and native AX exposes error text; spoken screen-reader announcement was not exercised |

The capacity directories were test-authored empty fixtures, not real transaction
backups. The harness retained all 32 together under
`evidence/capacity-fixture-retained` before testing recovered capacity. That manual
fixture relocation is not an application cleanup/reclamation feature. The three
real installation-attempt staging directories remain under managed staging.

## Safety, integrity and storage results

`evidence/after-real-install-and-blocked-approval.json` confirms both synthetic
configurations remained byte-identical to their originals through real installation
and blocked approval. Fixture-phase hashes/snapshots and `fixture-integrity.jsonl`
record unchanged inputs across subsequent inspections; intentional harness edits
are separate scenario setup. Final configs are restored to their original exact
bytes. No backup/receipt publication or client write occurred through the UI.
This is byte evidence for synthetic fixtures, not a transient-write syscall audit.
Real user configurations were not read for hashing or used as fixtures.

`evidence/final-integrity.json` confirms **5,887 runtime and 7,847 MCP entries** retain
their recorded content hashes, modes, inode identities, sizes and symlink targets
after client testing. All three staging attempts and the owned capacity fixture
are retained. Pinning, integrity checks and automatic-deletion policy were not
changed. Actual installation invoked only private Node/npm, never system runtime
fallback or the MCP entrypoint as a server.

Process sampling observed the owned app, private staging Node probes and npm
operations (`npm`, `npm ci`, `npm ls`), with no unexpected descendants observed.
The final selected-process snapshot found no direct child processes and no open
protected client-config handles. Sampling has gaps and cannot exclude every
short-lived process/open; it is not an exhaustive execution/access audit. Client
adapters and consent paths contain no client/server execution, and no real client
CLI command was issued by this test. Existing unrelated user apps were not stopped.

The restarted webview returned `null` for a previously set nonsecret localStorage
sentinel (`24-storage-before-restart.ax.txt`, `26-storage-after-restart.ax.txt`).
The actual restarted backend returned managed paths inside the root,
`managedValid: true`, `safeToApply: false` (`27-restarted-isolated-backend-preview.ax.txt`).
These observations support the configured nonpersistent webview and injected roots;
they do not claim confinement of all macOS framework bookkeeping.
Native Web Inspector showed Vite connection/HMR diagnostics with no application
exception observed at inspection points. Console observation was not continuous.

## Restart isolation incident and focused fixes

After a native Quit, asking the computer-use tool for window state unexpectedly
relaunched the temporary bundle without inherited opt-in variables. It displayed
normal fresh Setup. No AI-clients navigation or configuration operation was issued
in that instance before it was stopped. Source behavior indicates normal Figcon
status may have been inspected and framework/cache bookkeeping may have occurred;
**zero production-path access for that interval is not established**. No real MCP
configuration change was made, and normal user data was not inspected afterward.

The temporary bundle's launch metadata now binds both acceptance variables. More
importantly, debug startup now refuses a dedicated `figcon-native-<32 characters>`
executable copy when both opt-in variables are missing, before constructing Tauri.
The name check is denial-only: it grants no ownership or root-selection authority.
Release builds exclude it. A deterministic regression covers lost environment,
ordinary debug startup and validated explicit selection. An actual updated debug
executable placed under the acceptance root was launched **without** the variables
under a denial sandbox and exited with the static opt-in error before Tauri
initialization (`environment-loss-regression.json`). The updated binary was then
successfully restarted with explicit isolation and inspected natively. Future
restart testing must never assume observation cannot auto-launch an app.

An additional deny sandbox prevented real client-config/global-root access and
private/synthetic executable launches, but the native automation service could not
identify that sandboxed app window. That positive sandboxed-UI qualification was
not completed; the final positive UI restart used the validated application-level
acceptance mode. No claim of OS-sandbox-certified UI acceptance is made.

The other product fix is `appearance: none`/`-webkit-appearance: none` on styled
buttons: WebKit retained stale native light button backgrounds during a theme
transition despite correct computed dark CSS colors. The exact enabled-control
light→dark transition was repeated natively after the reset; backgrounds/text and
focus remained legible. Screenshots 15 (before) and 18 (after) retain the regression
comparison. This is a native visual regression test, not a DOM-only proof.

## Automated validation and checkpoint readiness

- Final locked Rust suite: **192 passed, 7 opt-in tests ignored**.
- Frontend tests: **49 passed**; frontend production build passed.
- Rust formatting and locked backend check passed.
- macOS desktop build without bundling passed; existing compiler warnings remain.
- Diff/secret/artifact checks passed; evidence/binaries/screenshots remain outside
  the repository. No commit, push, merge or PR was performed.
- Release executable inspection confirms neither acceptance-variable names nor the
  new debug-only startup-guard message is included.

One initial full Rust run had an existing cross-process recovery test return
conservative `NeedsReview` instead of `NotApplied`. Inspection found a readiness
file publication race: the parent could observe the path before its ID was fully
written. The test now atomically renames the completed readiness file and asserts
ID completeness. No production recovery behavior/assertion was weakened. The final
full suite passed; recurrence would require further investigation rather than
being dismissed as harmless.

The functional checkpoint is ready for review with the incident/limits disclosed.
Outstanding qualification: uninterrupted full filesystem/process auditing, native
cancellation specifically during npm execution, conclusively active-operation
reload, full keyboard-only/VoiceOver verification, Windows execution, and signed/
notarized release packaging. Production writes, receipt adoption and safe manual
reclamation remain deliberately gated follow-up work. Do not present this report
as proof of live Claude/Codex/Figma connectivity or an incident-free isolation audit.

## Historical isolation preflight and remediation

The following records are retained as history; the current verdict above supersedes
the earlier blocked-before-launch result.

Date: 2026-11-11. Branch: `feat/client-integration`. Host: macOS arm64.

## Verdict and isolation preflight

**Blocked, not accepted.** The native app was not launched. The requested rule was
that all relevant application-data, configuration and discovery paths must be
redirectable into an isolated environment; otherwise testing must stop. Source
inspection found that this condition is not currently met.

The production client context derives home and managed paths from Tauri
(`src-tauri/src/client_integration/mod.rs:61`). An isolated nonempty HOME can
redirect the default user configuration paths on macOS; the locally installed
Tauri/dirs implementation was inspected to establish that resolution. This alone
does not redirect discovery:

- `src-tauri/src/client_integration/mod.rs:21` always adds absolute candidates under
  `/opt/homebrew/bin` and `/usr/local/bin`, in addition to home-relative candidates.
- Detection canonicalizes and inspects these candidates (`mod.rs:108`). HOME and
  PATH changes do not remove them. They may discover real installed clients and
  make synthetic missing/detected/ambiguous cases depend on the host machine.
- Production context construction has no isolated discovery-root selector. The
  synthetic contexts used by Rust tests are not a native acceptance launch mode.
- `CLAUDE_CONFIG_DIR` and `CODEX_HOME` are detected (`mod.rs:70`) but explicitly
  produce `unsupported-location` (`mod.rs:222`), rather than redirecting supported
  inspection and planning into those directories.

This is a test-isolation limitation, not evidence of client execution or a
configuration-write vulnerability. A denial sandbox or replacement discovery
implementation was not substituted for the requested production-path acceptance.
No filesystem permissions or discovery policy were weakened to bypass the blocker.

## Execution matrix

| Area | Result |
| --- | --- |
| Branch, working tree, engineering/docs and source preflight | Inspected; existing uncommitted implementation preserved |
| Complete path redirection | Blocked by fixed global discovery candidates |
| Native launch; Setup / AI clients navigation | Not run |
| Installation progress, cancellation, reload and restart | Not run |
| Client discovery/configuration scenario matrix | Not run |
| Backend plan display, expiration and warnings in native UI | Not run |
| Confirmation, duplicate approval, decline, stale plans and disabled outcome in native UI | Not run |
| Missing/inaccessible configuration, capacity and backend-failure recovery | Not run |
| Light/dark appearance, resizing, keyboard and assistive technology | Not run |
| Native screenshots | None; application was not launched |
| Before/after synthetic configuration comparison | Not run; no fixture environment was launched or mutated |
| Child-process execution monitoring | Not run; no app, client CLI or MCP server was launched by this task |

## Safety and validation

No real client configuration was read, written or snapshotted by the investigation.
No installation was attempted, no managed installation/recovery artifact was
modified or deleted, and no production mutation gate was changed. This report is
the only file added by this acceptance attempt. No code fixes were made.

The production approval branch was inspected: `consent.rs:284` documents and gates
the mutator behind a test-only call; production returns `activation-disabled`.
This is source evidence, not a native execution result.

No test suites were rerun because testing stopped at the isolation precondition and
there were no code changes. The preceding implementation validation reported
185 Rust tests passed, seven opt-in tests ignored and 49 frontend tests passed;
those are historical results, not results of this acceptance attempt. The report
and whitespace diff were checked.

## Required next step before native acceptance / PR #6

Provide a reviewed native qualification mechanism that supplies a verified private
home, managed application-data root and complete executable discovery candidate
set to every relevant backend path, with no fallback to real client roots or
host-wide client candidates. It must leave production mutation disabled and
preserve the real adapters, immutable plan registry and installer validation.
Alternatively qualify a genuinely isolated macOS test environment with synthetic
clients and no real user configurations or clients visible. Do not alter the host's
global installations to make tests pass.

After isolation is established, execute all requested native scenarios, retain
fixtures/recovery artifacts and before/after hashes, capture screenshots, and
record native accessibility observations. Native acceptance remains outstanding;
this report does not qualify PR #6 as desktop-accepted.

## Isolation remediation (subsequent implementation)

The original stop above remains the historical acceptance result. A debug-only
backend-owned native environment now removes that source-level blocker. See
[client inspection: exact launch procedure](client-inspection.md#exact-launch-procedure-macos-development-only).

The development process requires both `FIGCON_NATIVE_ACCEPTANCE=1` and
`FIGCON_NATIVE_ACCEPTANCE_ROOT` pointing to a validated private marked temporary
root. `scripts/prepare-native-acceptance.py` provisions passive synthetic clients,
JSON/TOML and private directories without launching any process or deleting
artifacts. Production/default search roots are preserved; the selected debug
environment uses only its explicit candidates/config home/data and has no fallback.
Legacy discovery is blocked. Tauri app directories are redirected and webview
storage is nonpersistent. The override reader is compiled out of standard release
builds. Production mutation remains disabled in debug acceptance as well.

Regression coverage now includes production candidate lists, empty injected search
roots, private root selection and rejection, linked/replaced roots, marker changes,
Tauri storage/ephemeral-window settings, missing/detected synthetic candidates,
symlink/hard-link candidate rejection, passive JSON/TOML inspection/redaction,
isolated preview paths, and consent binding/staleness with unchanged configuration
bytes on disabled approval. No real client CLI/MCP server is called by these tests.

This does not retroactively pass native acceptance. All native execution, screenshots,
keyboard/focus, light/dark, resizing, restart/reload, progress/cancellation, complete
consent/recovery scenarios, process monitoring and before/after fixture comparisons
in the matrix above remain to be performed using the documented debug launch.

Remediation validation on macOS arm64: locked Rust suite **191 passed / 7 ignored**;
frontend suite **49 passed**; Rust formatting, locked backend check, frontend
production build, macOS desktop build without bundling and whitespace checks passed.
A targeted release-profile test passed with both acceptance variables set to an
invalid root, proving they are ignored in that build. The built release executable
also contains neither acceptance variable name. Fixture-script output was checked
for private modes and passive candidates, with synthetic configuration hashes
retained in its private temporary evidence. Existing compiler warnings remain.
Windows execution and native GUI behavior were not tested in this remediation.
No real client configurations, client CLIs or MCP servers were used.

</details>
