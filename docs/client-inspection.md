# Read-only client inspection

The original read-only commands in the `client_integration/` backend module are:

- `list_ai_clients()` returns summaries for Claude Code and OpenAI Codex CLI.
- `inspect_ai_client({ client })` returns one passive registration summary.
- `preview_ai_client_configuration({ client })` adds the proposed managed launch
  paths and prerequisite status. `client` is a closed `claude-code`/`codex-cli` enum.

The thin TypeScript client is `src/client-integration/`. The original inspection
commands remain passive. A separate bounded consent API is described below; no
functional production writer, receipt adoption or client subprocess is introduced.
The desktop [client review UI](client-connection-ui.md) now consumes inspection and
consent contracts; approval remains blocked by the production backend gate.
Installation commands and retention behavior remain unchanged. See the preserved
[architecture proposal](client-integration-architecture.md) for future writes.

## Actual capabilities

Separate `ClaudeCodeAdapter` and `CodexCliAdapter` provide JSON/TOML parsing and
default configuration/executable candidates. Detection checks conventional CLI
locations, deduplicates resolved symlinks, and reports distinct multiple matches
as ambiguous. A desktop application bundle alone is not a detected Codex CLI.
No executable is run, including for version/help probes. Detected means a suitable
executable candidate exists, not authenticated provenance or a tested CLI version.
Unknown/custom executable locations may be reported missing.

Only default user configurations are supported: `~/.claude.json` and
`~/.codex/config.toml`. Any visible `CLAUDE_CONFIG_DIR`/`CODEX_HOME` override yields
`unsupported-location`, without reading the guessed default or exposing the
variable value. Overrides in a separate terminal are not visible to the GUI;
preview cannot establish effective configuration across client sessions.

On macOS, configuration reads traverse absolute components using directory
handles and `openat` with no-follow/nonblocking flags. Reads reject symlinks,
hard-linked files, non-regular files, unsafe ownership/write permissions, oversized
files (4 MiB maximum), invalid UTF-8, and observable in-place changes while reading.
Root-owned sticky system temporary ancestors are allowed for isolated fixtures.
JSON duplicate keys are rejected recursively; the pinned TOML parser rejects
invalid documents and duplicate definitions. Parser error text never crosses IPC.
Unknown top-level fields are not discarded or rewritten: no serialization back
to disk exists. Windows commands report unsupported inspection; parser/contract
tests still compile there, while macOS filesystem tests are gated.

Inspection reports client identity/name, executable detection, configuration status,
a static default location label, registration classification, ownership assessment,
and closed warning codes. It never returns existing commands/arguments, environment
maps, project names, unrelated settings, credential values, or raw errors.

The reserved proposed name is `figcon-figma-console`. Exact command/argument
matches there are `matching-external`. Other recognized Figma Console registrations
produce `external` plus a duplicate warning. A different reserved definition is
`conflict`; extra fields (including environment/disabled/working-directory fields)
make that definition conservatively `unsupported`. No claim is made that unknown
fields can safely be ignored. Unrecognized aliases/wrappers may be missed; no
arbitrary executable is inspected or launched to resolve them. Known Claude
per-project name collisions produce a boolean shadowing warning only; projects
are not opened. Every report warns that effective project/profile/managed state
is unknown. Configured does not mean connected or Figma-ready.

## Preview and ownership semantics

A preview contains the inspection, proposed name, absolute managed Node executable
and MCP `dist/local.js` argument, user scope, `existingRegistrationWouldBeAffected`,
`approvalRequired`, `safeToApply`, and `managedInstallationValid`.

Proposed paths come exclusively from Figcon app-data resolution. Structural managed
status must validate both runtime and MCP for `managedInstallationValid: true`.
This is not a fresh checksum attestation. Proposed paths are returned even when
prerequisites are missing, accompanied by a warning; they are not read from client
configuration. No arbitrary path or command is accepted through IPC.

`safeToApply` is **always false**, `approvalRequired` is **always true**, and
`existingRegistrationWouldBeAffected` is **always false**: this is a non-executable
add-only preview, with no overwrite/removal plan. A conflict blocks any future
addition rather than predicting an overwrite. This informational preview has no plan ID; the separate consent-preparation API
returns immutable plan IDs only after prerequisites qualify.

`OwnershipRecord` defines a future private durable schema: schema/receipt IDs,
client, user scope, server name, configuration identity, entry fingerprint, managed
installation identity, creation time, prior absence, and transaction lifecycle.
It is never written, loaded from disk, or returned over IPC here. Production
ownership is therefore `absent`. A supplied record can be `mismatched` or
`unverified` in internal assessment; even a matching record labelled verified does
not prove provenance. No registration is reported Figcon-owned based only on its
command. A future trusted ledger loader and current-entry verification must be
designed before offering owned disconnection or rollback.

## Limits and verification

Queries are independent observations, not transactional snapshots or persisted
plans. Files can change afterward; descriptor-relative reads do not prevent an
opened directory being renamed or all concurrent in-place writes. Detection is
also best effort. These limitations authorize no writes. Blocking work runs off
the async executor; kernel operations are not forcibly preempted. Future mutation
must re-inspect, obtain fresh consent, solve/accept the documented concurrent-writer
boundary, and implement conditional ownership checks and scoped rollback. Atomic
rename alone does not solve lost updates. No write readiness is claimed.

Synthetic temporary-home tests cover client presence/ambiguity, missing/malformed/
inaccessible/unsafe configs, duplicate keys, external/conflicting/unsupported
registrations, credential redaction, unchanged file bytes, project warnings,
unsupported roots/platforms, absent/mismatched/untrusted ownership, and missing or
structurally valid managed components. Synthetic executable content is deliberately
not runnable: inspection must never execute it. No tests use real client configs.
The frontend transport tests run through the existing CI `test:installer` script.

## Dormant mutation engine

A separate internal engine is qualified with synthetic homes. Its production
constructor permits only planning and revalidation; only its `cfg(test)` constructor
enables mutation. The approval contract is exposed but its production gate is closed.
These inspection commands and informational previews remain read-only;
`safeToApply` stays false.
The engine's test-only durable transaction receipts are not loaded by this passive
ownership assessment. See the architecture document's implemented-engine section
for backup privacy, uncertain outcomes and unresolved production activation gates.


## Backend consent qualification API

- `prepare_client_configuration({ client })` creates a backend-owned five-minute
  add-only plan after passive client/config inspection and structural validation
  of the pinned managed runtime/MCP. It returns the opaque ID, exact plan-derived
  proposed name/argv/scope/operation, absent-registration status, summary, warning
  codes, display expiration and recovery accounting. No target or argv is accepted
  from the frontend. Installed validity is structural plus immutable payload
  fingerprints, not fresh full supply-chain attestation.
- `approve_client_configuration({ planId })` is the separate explicit approval
  invocation. The originating window label is supplied by Tauri, not the request.
  The backend consumes the plan once, revalidates current roots, prerequisites,
  source and payload identities/fingerprints, and checks capacity. It then returns
  `activation-disabled` in production. No production branch calls the mutator;
  the engine independently rejects mutation from its production planner.
- `reject_client_configuration_plan({ planId })` declines and invalidates a pending
  plan in memory. No external configuration is changed.
- `get_client_configuration_recovery()` returns aggregate counts/bytes/capacity
  only. It never returns filenames, backups, receipt contents or configuration data.

The consent client is `src/client-integration/consent.ts`; constructing it invokes
nothing. Startup, detection, installation and refresh never approve. The review UI requires
explicit confirmation; no enabled production write button is available. Backend invocation expresses consent intent;
it does not cryptographically prove a physical click. Window routing/labels are
not a security boundary against hostile same-user code.

The registry allows at most 16 retained plan records and preparing/approval work
slots, at most 64 KiB of accounted payload per plan and 1 MiB of registry payload.
These are logical payload budgets, not a hard allocator/RSS sandbox. Consumed and
rejected tombstones remain until expiry to reject duplicate submissions. Full
records are dropped immediately on consumption. Expired entries are pruned on
admission and by a 30-second memory-only sweeper; backend restart invalidates all
plans. Expiration authorization uses monotonic time; wall-clock expiration is
presentation only. Expired/used/stale plans require a new preview and approval.

Recovery limits are 32 retained artifacts, 64 MiB apparent file bytes and 4,096
entries, reserving 12 MiB and eight entries for the next transaction plus a 64 MiB
free-space margin. Unknown/unsafe inspection blocks admission. No cleanup is
performed. See the architecture document for precise accounting and the residual
race with external writers. Production activation remains a separate review.

## Debug-only native acceptance environment

The backend now constructs an explicit `DiscoveryEnvironment` from configuration
home, application-data directory, complete executable search roots and platform
support. The ordinary environment preserves the four existing search locations,
default JSON/TOML paths and unsupported-root-override behavior. Injected environments
never append production candidates or consult PATH.

For native qualification, a backend `acceptance::Session` selects the environment
**before Tauri builds any windows**. Only debug-assertion builds read the two
acceptance variables. Both an exact opt-in and valid root are required; invalid or
partial selection aborts startup with a static error. Standard release builds
compile out the override reader and validator and ignore these variables. Windows
debug builds explicitly reject selection. No override is accepted from IPC.

The root must be a dedicated direct child of `/private/tmp` or the canonical OS
temporary directory, named `figcon-native-` plus 32 ASCII alphanumeric characters.
Name alone is not ownership evidence: no-follow directory-handle traversal checks
ancestors, owner and private 0700 modes; an exact private 0600 single-link marker
is required. `home`, `home/.codex`, `data` and `bin` must already exist as private
directories. The root device/inode and marker/directories are revalidated before
backend path resolution. Validation failure never falls back to the real home.

All inspection/preview/consent commands use `<root>/home` and `<root>/bin` only.
Synthetic detection uses no-follow opens instead of canonicalizing links; linked,
nonregular, unsafe-mode or foreign-owned candidates are not detected. It never
executes their contents. Client-root override environment variables are ignored in
this explicitly selected synthetic environment. Config readers retain existing
bounded no-follow/link/ownership checks. There is no alternative parser or consent
implementation: the actual immutable plan registry and two closed write gates run.

Tauri app configuration, data, local data, cache and log paths are redirected by
`appDirectoriesOverride` into `<root>/data` before window creation. Webviews use
incognito/nonpersistent storage. Installer path resolution independently uses the
selected data root and revalidates the acceptance selection. The installer itself
is unchanged: real pinned installation still verifies checksums/lockfiles, uses
private npm and retains staging. No synthetic payload bypass is added. To qualify
previews, install managed prerequisites through Setup inside this root, or provision
independently verified real managed components there; never point the mode at normal
Figcon data. Synthetic client files themselves are never executable probes.

Legacy `detect_environment` returns no host-client detections and legacy Claude
configuration discovery returns a static unavailable error in this mode (including
if the root becomes invalid). They cannot bypass the injected environment. Normal
production behavior of both commands is unchanged.

### Exact launch procedure (macOS development only)

From the repository root, with frontend dependencies already available:

```sh
acceptance_root=$(python3 scripts/prepare-native-acceptance.py)
printf '%s\n' "$acceptance_root"
FIGCON_NATIVE_ACCEPTANCE=1 FIGCON_NATIVE_ACCEPTANCE_ROOT="$acceptance_root" npm run tauri dev
```

The fixture script creates both passive client candidates and synthetic configs;
it does not launch anything or create a managed runtime. Do not set HOME or replace
real client files. Do not use `open` to launch an already running/release copy: the
actual debug process must inherit both variables. Quit that process, then use the
same command/root to test restart. Reloading the window preserves backend state;
restarting invalidates in-memory consent plans. All artifacts are retained. To test
missing clients, rename only the corresponding files inside `<root>/bin`, then
refresh. Change only the synthetic JSON/TOML files for other scenarios and preserve
before/after snapshots inside this root. No production configuration write can be
enabled by this mode or these variables.

This is application-level cooperative test isolation, not an OS sandbox against
malicious same-user processes, compromised WebKit/native code or deliberate root
relocation between syscalls. Directory descriptors do not prove continued ancestry.
No claim is made that all macOS framework bookkeeping is confined; app-specific
storage is redirected/nonpersistent. Keep roots dedicated, do not mutate/relocate
them during a run, and do not copy real secrets into fixtures. Downloads/private
runtime execution occur only when explicitly exercising the real installer. Native
focus, screenshot, process-monitoring and byte-preservation acceptance still needs
to be rerun; unit qualification alone does not satisfy those scenarios.

### Native restart guard

Native acceptance found that an automation observation after Quit can relaunch an
app without the environment supplied to the original shell process. Debug startup
now refuses a dedicated executable copy located under a `figcon-native-` ancestor
with a 32-character alphanumeric suffix when neither opt-in variable is present.
This is a denial-only check, never ownership inference or implicit mode selection;
valid opt-in/root validation is still required. Ordinary repository debug startup
and release behavior are unchanged. The guard itself is excluded from release.
For temporary `.app` automation, bind both variables in launch metadata as well and
never assume reading app state cannot launch it. See the native acceptance report
for the observed incident, negative executable regression and qualified results.
