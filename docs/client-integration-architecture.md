# AI client integration architecture

Status: experimentally validated design and isolated engine qualification, 2026-11-11, `feat/client-integration`.
**Production configuration writes remain disabled.** This document does not grant
consent to change real client settings. No onboarding UI is part of this work.

## Decision

Keep separate Claude Code and Codex CLI adapters and the existing passive
inspection API. Do not use either installed CLI as a live configuration writer
for the initial integration. Their observed behavior fails Figcon's conditional
change/preservation requirements in different ways.

Recommend narrow, syntax-preserving entry edits for both clients, with independent
JSON and TOML implementations, shared immutable consent plans, private transaction
receipts, and cross-process Figcon admission. Do not implement a generic whole-file
JSON/TOML serializer. A live writer is still a release gate:

- A genuinely absent target file can use atomic **no-replace creation**, with an
  approved complete initial document, verified existing parent and fresh consent.
  This prevents Figcon overwriting a concurrent file creation. It does not stop
  another program subsequently replacing the file or establish active connectivity.
- For an existing file, no supported compare-and-swap/conditional-update interface
  was established. Neither native CLI invocation nor check-then-rename can promise
  preservation against uncoordinated writers. Remain preview-only unless the user
  approves a deliberately quiescent-client policy with that residual risk, or a
  documented interoperable transaction interface becomes available.
- Do not implement an undocumented Claude lock-directory protocol in production,
  acquire it and invoke the CLI (which can deadlock waiting for it), or interpret
  a standard flock as coordination with either client.

This is a recommendation, not approval of the residual-risk policy. Under a strict
requirement never to overwrite concurrent unknown changes, existing-file writes
cannot yet be enabled safely. Even an approved best-effort mode must never claim
that pre-write revalidation eliminates the final race.

## Official findings and version boundaries

Sources were retrieved on the date above. Installed versions tested: Claude Code
**2.1.267** and Codex CLI **0.167.0**. No installations/upgrades were performed.

| Primary source | Verified behavior relevant to this design |
| --- | --- |
| [Claude MCP reference](https://code.claude.com/docs/en/mcp) | User registrations use `~/.claude.json`; add/remove accept explicit user scope. Local/project/managed definitions can change effective behavior. Get/list can check connectivity, so passive verification must not invoke them. |
| [Claude settings](https://code.claude.com/docs/en/settings) | `.claude.json` also holds account and per-project state. It is distinct from `.claude/settings.json`. Configuration-root overrides require explicit resolution. |
| [Claude error reference](https://code.claude.com/docs/en/errors#mcp-server-was-not-saved-or-removed) | Current documentation describes config lock contention and ambiguous writes. It explicitly notes that versions before 2.1.283 could report success without saving; the configurable lock wait described there is newer than the installed version. These are not expected-hash or conditional-entry APIs. |
| [Codex MCP documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) | User TOML MCP tables support stdio command/args. The official CLI can add/remove registrations. `/mcp` reports session availability, not registration ownership. |
| [Codex configuration basics](https://learn.chatgpt.com/docs/config-file/config-basic) and [advanced configuration](https://learn.chatgpt.com/docs/config-file/config-advanced) | User config defaults to `~/.codex/config.toml` under CODEX_HOME. Project, profile and managed layers affect effective configuration. |
| [Codex tagged MCP commands](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/cli/src/mcp_cmd.rs#L417) | Add loads the global server map, inserts by name, then calls `replace_mcp_servers`; remove similarly loads/removes/replaces. No expected-entry argument is present. |
| [Codex tagged edit engine](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/core/src/config/edit.rs#L697) | Reads and parses a document, applies edits, and calls an atomic-write helper. No transaction lock or expected-version check appears in that path. The MCP replacement uses typed entries. |
| [Codex tagged write helper](https://github.com/openai/codex/blob/rust-v0.167.0/codex-rs/utils/path-utils/src/lib.rs#L154) | Creates a NamedTempFile in the parent, writes contents, and persists it over the target. This helper has no explicit file/directory fsync or conditional replacement. Atomic visibility is different from concurrent-update protection and power-loss durability. |

Tagged source is consistent with experiments but was not reproducibly built to
prove binary/source identity. Public documentation changes independently of local
versions. Requalify every supported version; do not silently assume a newer client
has the same behavior. Help advertised no expected hash, expected entry, transaction
ID, conditional remove, or compare-and-swap option in either installed CLI.

Local inspection of embedded JavaScript in the installed Claude binary found a
config-save path acquiring a `<config>.lock` **directory**, rereading config under
that lock, applying a callback, and replacing the file. It also contains fallback
save paths, stale-lock/heartbeat logic and optional alternate storage-backend paths.
The user-scope add path checks name existence before its save callback; the
callback assigns that name without a conditional entry guard. This inspection is
implementation evidence, not a complete audit or an official interoperability
contract. Do not copy that implementation into Figcon or rely on internal symbol
names, lease timings, fallback behavior, or hidden backends as stable APIs.

## Experiments: methods and observed results

All registration experiments used synthetic JSON/TOML files, dedicated temporary
HOME directories, a neutral working directory, and an existing temporary CODEX_HOME.
CLI environments omitted real auth and unrelated environment variables. No real
configuration, credentials, sessions, plugins or keychain material was copied.

Each CLI ran inside an additional macOS sandbox: network denied, real-home reads
denied, writes allowed only inside the experiment root, and execution from that
root denied. The configured stdio command was a nonexistent path inside that root.
No MCP server could run. Only version/help/add/remove were used, never get/list,
interactive agent sessions, login, OAuth, imports or upgrades. The first attempt to
apply this nested sandbox was denied by the outer execution sandbox; experiments
ran only after explicit tool approval for the restricted runner. An initial Python
3.9 harness lacked TOML support; it was rerun with installed Python 3.13, without
installing a dependency. The completed results below are from the corrected run.

Synthetic fixture: one unrelated MCP registration, an unknown MCP field, unrelated
top-level settings, a synthetic env sentinel, and (for TOML) comments. Native CLI
stdout/stderr and before/after fixture copies stayed in temporary evidence, not
application logs or the repository. Source binaries were fingerprinted with SHA-256
in that evidence. These fixtures contain no real secrets.

| Experiment | Claude Code 2.1.267 | Codex CLI 0.167.0 |
| --- | --- | --- |
| Add/remove in valid synthetic config | Preserved existing unrelated values, including unknown MCP field. Startup added eight migration/first-run metadata keys; whole-file bytes did not remain unchanged. Target inode changed. | Preserved unrelated top-level values and top comment; dropped the unknown field and table/argument comments in the unrelated MCP registration. Target inode changed. Removal did not restore that lost data. |
| Sequential duplicate-name add | Exit 1, existing definition remained. | Exit 0, existing definition overwritten with new args. |
| Six parallel distinct additions, three rounds | All six names survived in every round; all exited 0. | Only one of six names survived in every round; all six exited 0. Confirmed lost updates, not merely a theoretical race. |
| Holding flock on the original config file | Add still completed, about 0.14 seconds. | Add still completed, about 0.05 seconds. |
| Malformed file, no recovery backup | Command exited 1, but startup backed up/replaced the malformed config with migration metadata; original bytes were not preserved at the target. | Command exited 1 and original bytes remained unchanged. |

Additional Claude lock experiments used warmed-up synthetic configs to separate
first-run migrations from registration effects:

- A fresh manually held config lock-directory kept three add processes alive
  until release. In **three rounds**, all three same-name adds then exited 0;
  only one final definition remained. Native serialization preserved the document
  structure but did not enforce conditional name creation.
- While one add waited behind the directory, the harness inserted a different
  definition for that name and an unrelated field. After release, add exited 0,
  overwrote the new definition, and preserved the unrelated field.
- SIGKILL of the harness-owned process while it waited left config bytes unchanged.
  The harness released its own directory gate; retry succeeded. This is not a
  demonstration of automatic recovery of a genuine stale client lock.

The lock gates used bounded timing (0.7 seconds) and observed all processes still
alive before release; they did not instrument an exact internal callback barrier.
The repeated outcomes and embedded save-path inspection support the race finding,
not a formal proof of every internal branch.

Codex interruption samples used the native executable, avoiding termination of only
its Node launcher. At 5 ms it was still alive and was SIGKILLed: original bytes
remained and the registration was absent. At 15/30/50 ms the process had already
completed: documents were parseable and registration was present. All four retries
succeeded. **No sample demonstrably interrupted the write syscall itself**. Do not
claim tested torn-write recovery, fsync durability, sudden power loss, or arbitrary
crash-point safety from these results. Only experiment-owned PIDs were signalled;
no other client process or process group was terminated.

A separate deterministic **model**, not a native-CLI test, checked an open file's
identity and full hash, then performed an uncoordinated in-place update before
publishing the candidate by rename. The external update kept the inode but was
lost. This demonstrates the remaining check-to-commit window; adding more checks
merely moves that window.

Reproduction recipe: use independent disposable homes; seed the documented fixture;
run `claude mcp add --scope user --transport stdio <name> -- <nonexistent-command>
<arg>` or `codex mcp add <name> -- <nonexistent-command> <arg>` via explicit argv;
inspect bytes with a parser, not health-check commands. Repeat parallel adds and
lock gates under the sandbox above. Do not reproduce against real configurations.
Temporary evidence includes runner scripts, sandbox policy, binary hashes, fixture
copies, `results.json`, `concurrency-results.json`, corrected
`interruption-results.json`, and a sanitized `summary.json`. Timing sample fields
in the initial concurrency output were superseded by corrected interruption results
that measure bytes **before** retry; do not use the old fields for that conclusion.

## Current implementation and boundaries

The read-only implementation is described in [client-inspection.md](client-inspection.md).
Separate adapters parse default user JSON/TOML using bounded no-follow reads;
unsupported overrides and unknown registration fields block assumptions. Detection
is passive. Proposed absolute paths point to Figcon's private Node and MCP entrypoint,
not npx/system Node. The passive ownership schema does not load or trust receipts.
Informational previews remain `safeToApply: false`. A separate consent contract now
returns bounded immutable plan IDs, but production approval remains disabled.
The engine's production constructor permits planning only; mutation still occurs
only during synthetic qualification tests. No disconnector or onboarding UI exists.
The earlier CLI experiments did not modify the passive implementation.

Keep installer status and integration state separate. Structural installed status
is not a fresh authentication of every installed byte. Preserve pinned versions,
checksums, lockfile verification, private npm, no-replace installer activation,
process supervision and the disabled automatic staging-deletion policy. Integration
transactions must not consume or reclaim installer attempts. A registered command
does not prove Figma connectivity or successful plugin onboarding.

## Recommended mechanisms by client

### Claude Code: narrow JSON mutation, not live CLI invocation

The official CLI has useful native serialization among its writers, but no exposed
conditional entry check. It can migrate/repair unrelated state before registration,
and this installed version can misreport save failure. Delegating a live add/remove
therefore does not implement Figcon's consent contract.

For a future explicitly approved writer, use strict duplicate-key-rejecting JSON
parsing plus a qualified syntax-tree/span editor. Insert/remove only the selected
user `mcpServers` entry, retaining every untouched value and raw span, including
account/project/usage metadata. Validate the candidate's semantics and changed
spans against the plan before publication. Do not use whole-object pretty printing,
a homemade partial JSON parser, native startup repair, or an unlock/delete-stale-lock
fallback. Reject malformed, unsupported, linked or inaccessible files unchanged.

Retain the CLI as a compatibility oracle in synthetic tests only. Figcon's own
lock can serialize Figcon writers; it does not acquire Claude's version-dependent
native protocol. A supported native conditional API would be preferable if added
in a future qualified client version.

### Codex CLI: narrow TOML mutation, not whole-map CLI replacement

Do not use native `mcp add/remove` on the live config: duplicate overwrite, unknown
field loss, comment loss and concurrent lost additions are confirmed. For a future
approved writer use a pinned, syntax-preserving TOML document editor, modifying
only `[mcp_servers.figcon-figma-console]`. The current inspection-only TOML Value
parser is **not** sufficient to write comments/formatting losslessly. A direct
syntax-tree editing dependency would need separate justification and qualification.
Preserve unknown tables, unknown fields, comments, ordering, profiles and unrelated
MCP entries byte-for-byte outside necessary insertion separators. Reject ambiguous
inline/dotted-table forms until supported and tested; never normalize the whole map.

For both clients: direct mutation improves control of the requested delta but does
**not** solve the uncoordinated read/modify/write race. Do not quietly switch from a
failed native writer to direct editing, or from failed direct editing to a CLI.

## Threat model, admission and publication

Supported: accidents, malformed files, crashes, cancellation, cooperating Figcon
processes and unexpected filesystem state. No isolation is claimed against a
malicious process with the same user's permissions. No privileged helper or
security service is proposed. Ordinary uncoordinated client writes remain a real
correctness risk even inside that threat model; excluding malicious actors does
not make lost updates acceptable or impossible.

Use one durable, never-deleted, private Figcon lock file per client/config-root
identity and an OS advisory lock across processes. Hold it from apply preflight
through journal settlement, with bounded waits. Serialize clients independently;
there is no atomic transaction across Claude and Codex. Lock admission rechecks
all plan preconditions. Never ask a client to stop or terminate it automatically.
Closing clients is a user action, not proof that no unknown writer remains.

Publication for an approved existing-file best-effort policy would:

1. Verify managed runtime/entrypoint, config root, parent handles, ownership,
   regular-file/link properties and bounded content. Reject unsupported ACL/xattr,
   permissions and symlink behavior instead of silently dropping metadata.
2. Bind the expected source bytes and filesystem identities; build the narrow
   candidate privately; prove only the approved entry changes. Create a sibling
   temporary file exclusively with private permissions and flush its contents.
3. Persist/fsync an `applying` journal. Immediately revalidate the complete expected
   source fingerprint, target and parent identities; any mismatch invalidates
   approval. Never merge a new config into an already approved plan.
4. Publish with atomic replacement and fsync the parent, then passively reread and
   verify the exact entry, unrelated-data invariants and new target identity. For
   absent files use atomic no-replace publication, not the existing-file operation.
5. Persist/fsync verified receipt before emitting completion. A post-rename error
   is `needs-review`/possibly applied, not a claim that nothing changed.

This sequence provides durable intent and atomic visibility, **not filesystem CAS**.
An open FD pins an object, not the pathname or continued directory ancestry; a hash
pins observed bytes, not future writes; flock is advisory; rename replaces without
checking the expected version. Rejecting observed ancestor changes is useful but
cannot guarantee confinement after a concurrent rename. Unexpected relocated or
unknown resources must be retained, not recursively cleaned up through open FDs.

No-replace creation is the narrow case that excludes overwriting a concurrent
creation at publication. Existing-file mutation requires explicit acceptance of the
remaining window or a genuinely shared conditional transaction mechanism. Under the
strict policy, refuse it. Do not present process enumeration, file watchers, lease
files or repeated stat/hash checks as proof of quiescence or a new security boundary.

## Immutable preview and approval plan

Current previews are informational. Future executable plans are immutable backend
objects, never reconstructed from frontend-supplied config or argv. Proposed schema:

```text
ApprovalPlan {
  schemaVersion, randomPlanId, operationId, client, adapterVersion,
  operation: add | remove-owned, scope: user, serverName,
  executableAbsolutePath, argumentsExact[], managedPayloadIdentity,
  targetRootIdentity, targetFileIdentityOrAbsent,
  expectedConfigurationSha256OrAbsent, expectedEntryFingerprintOrAbsent,
  proposedEntryFingerprintOrAbsent, candidateSha256,
  unrelatedDataFingerprint, ownershipReceiptIdOrNone,
  writerPolicy: create-only | approved-quiescent-best-effort,
  compensationPermission: none,
  ownerWindow, applicationSessionId,
  createdMonotonic, expiresMonotonic
}
```

Default expiry: **five minutes**, process-local monotonic time. Plans do not survive
backend restart or configuration-root changes. Bind exact ordered argv and types,
not a display command or shell string. No version, URL, executable, scope, path or
raw JSON/TOML is accepted from the frontend; client/action/name choices are enums.
Opaque one-use IDs refer to server-held data, not a client-editable signature blob.
Do not expose whole-config hashes, unrelated names, existing env values or secrets.

A separate confirmation shows one client's user-wide registration, exact managed
launch summary, conflict status, restart guidance, and the chosen concurrency
limitation. Approval does not include replacement, external adoption, future changes,
other clients, or rollback removal. Apply atomically consumes the plan and journals
its attempt; duplicate submissions return that attempt status rather than retrying.

Any full-config byte change, file/root identity change, relevant policy change,
managed payload change, client/adapter qualification change, ownership inconsistency,
expiry, cancellation or app restart invalidates the plan. Even an unrelated usage
counter change requires a new preview/approval. Do not silently refresh fingerprints
or auto-rebase after consent. Validate once more under Figcon admission, while
acknowledging that the final check cannot exclude a later external write.

Existing name: exact external match is an externally managed no-op, never ownership
adoption. A different definition is a conflict; first release offers no replacement.
Another Figma Console name produces a duplicate warning. Project/profile/managed
shadowing is read-only/possibly unknown; do not edit those scopes or disable policy.

## Ownership receipts and stale-state policy

Within the cooperative same-user boundary, provenance comes from a private Figcon
transaction ledger, not from the server name or an entry pointing to Figcon. Use
an owner-verified 0700 metadata directory, exclusive 0600 receipt files, no-follow
bounded reads, hard-link rejection and schema validation. Persist receipts atomically
and flush them. Do not claim this authenticates against a same-user attacker;
adding a signing key readable by that same user would not supply that guarantee.

Extend the current schema with transaction/plan digest, adapter/policy version,
canonical client/config-root identity, exact pre/post entry fingerprints, pre/post
full-config fingerprints and file identities, managed payload identity, prior
absence, creation/verification timestamps and lifecycle. Keep these backend-only;
no plaintext credentials or copies of unrelated account data in routine receipts.
For the initial add-only flow the before entry is absent, so no secret restoration
snapshot is required. Existing external entries are never replaced or adopted.

A verified receipt requires a completed matching journal and a passive current-entry
match across **all** fields, not just command/args. An old verified flag is not a
capability. Any observed target deletion/recreation, entry mutation, unknown extra
field, identity ambiguity or unresolved applying transaction suspends ownership-based
automation. Never delete/rebase the evidence to make a receipt appear current.

First-release conservative removal: require the recorded post-write file identity
and full bytes still match, plus a fresh approved removal plan. If another program
has changed the document, retain the registration and report review rather than
refreshing a stale receipt automatically. This intentionally blocks some legitimate
removals after unrelated Claude usage updates; relaxing it needs a separate decision.
Byte-identical delete/recreate (ABA) cannot always be detected without a client
mutation-generation mechanism. State that limitation; matching bytes do not prove
continuous ownership. Every removal still needs explicit consent and current guards.

Receipt absent/corrupt/mismatched/unverifiable means external or needs-review, never
permission to replace/remove. The passive API writes no ownership records. Internal engine qualification writes
separate transaction receipts only in isolated tests; those receipts are not
adopted by the passive inspection API.

## Narrow rollback and crash recovery

Do not restore an entire saved configuration as rollback. Do not infer from exit 0
that an approved change is present, or from exit failure/timeout that it is absent.
After any ambiguous outcome, inspect without executing the client/MCP and report
what can be established. Never blindly retry add/remove.

| Durable state and current observation | Recovery action |
| --- | --- |
| Prepared, target still exact approved pre-state | Mark no change; expire consent. Keep diagnostics; a future write needs new approval. |
| Applying, target exact proposed post-state | Record observed application, but do not mint unrestricted ownership/removal rights after an ambiguous crash. Require review and settle the journal conservatively. |
| Applying, target exact pre-state | Record not-applied; no automatic retry. |
| Any state, target differs/unknown/unreadable | Needs-review. Preserve all registrations and transaction evidence. |
| Verified receipt, matching current state | Report registered; not connected. |
| Verified receipt, changed or missing current entry | Suspend receipt; never recreate/delete automatically. |

Verification failure after apparent publication leaves `possibly-applied` and
`needs-review`, even if the native process exited successfully. Capture only bounded,
private evidence needed for diagnosis; do not log config/CLI contents or secrets.
A journal and external config cannot be atomically committed together; startup
reconciliation must acknowledge that gap and cannot infer provenance just from
an identical post-state.

An inverse operation is a **new** approved plan against current bytes: remove only
the verified Figcon-created entry, and only when conservative receipt continuity
and writer-policy preconditions hold. No automatic compensation is initially
authorized. Never remove an externally modified entry, OAuth tokens, another
registration, managed payloads, or project approvals. If unrelated changes appeared,
first-release rollback retains data for review. A future narrowly rebased inverse
could preserve unrelated changes via a syntax-tree patch, but it requires a proven
ownership policy and the same concurrency decision; it is not a lost-update cure.

Missing/corrupt receipts, stale locks, orphan candidate files and unexpected links
are diagnostic states, not automatic deletion instructions. No recursive cleanup
of external directories or unknown transaction files. Safe user-controlled
reclamation and privacy/retention of any diagnostic snapshots remain follow-up work.

## API, UX and implementation sequence

Reuse the existing passive list/inspect/preview commands. Future prepare/apply/query
commands take closed client/action choices or opaque plan IDs only. Typed statuses
separate awaiting approval, applying, verified registration, possibly applied and
needs-review. Events are notifications scoped to operation ID/sequence, emitted
outside locks; authoritative queries reconcile missed delivery. Targeting is not
authorization. No shell execution surface or arbitrary filesystem arguments.

User journey: complete managed installation → select client → passive inspection →
review preview and conflict/concurrency implications → distinct approval → passive
verification → start a new client session and check `/mcp` voluntarily → separate
Figma-plugin onboarding. Never terminate/restart a client automatically. A failed
client does not undo a successful change for the other client.

Recommended sequence:

1. Keep current read-only behavior and existing tests. Add version-qualified native
   behavior tests as opt-in synthetic experiments only; never ordinary CI against
   installed user clients. Preserve Windows compilation gates.
2. Obtain explicit approval for writer policy. Create-only can be isolated from
   existing-file support. If strict concurrent preservation remains mandatory,
   existing-file writes stay blocked pending an interoperable conditional API.
3. Qualify separate JSON/TOML span editors with duplicate/malformed/unknown syntax,
   comment preservation, exact untouched-byte assertions, supported metadata,
   symlink/hard-link/ancestor replacement, and conflicting-name tests.
4. Implement immutable plans, inter-process Figcon locking and durable ledgers;
   test expiry, replay, cross-window submissions, changed full fingerprints,
   missing/stale/ABA receipts and post-publication journal failures.
5. Use injected barriers at every read/check/publish/journal boundary. Test
   cooperating processes separately from uncoordinated writers, explicitly showing
   the latter remain outside the strong preservation guarantee. Prove no-replace
   behavior for absent files. Never weaken a guard to make a race test pass.
6. Test crashes before and after publication/receipt persistence, disk-full,
   unsupported permissions, cancellation and ambiguous outcomes. More SIGKILL
   samples are not power-loss tests; qualify filesystem durability separately.
7. Add approval UI only after the backend safety policy is accepted. Opt-in native
   acceptance uses fully isolated clients/private runtime, no account or MCP tool
   calls, and separately approved disconnect. Installer-only external-configuration
   preservation tests continue to pass.

Decisions still requiring approval: whether to permit any existing-file best-effort
mode and its exact user disclosure; create-only first release; qualified versions
and native backend variants; strict removal continuity versus usable reviewed
rebasing; diagnostic privacy/retention; custom config roots; and eventual MCP
runtime environment/credential handling. No automatic replacement, external
adoption, automatic rollback, project-scope writes or client upgrades are proposed.


## Implemented internal engine: qualification behavior

`client_integration/write_engine/` contains the shared coordinator, descriptor-based
filesystem layer, separate JSON/TOML editing paths, and synthetic tests. It is
compiled on macOS. Its production constructor supports only planning/revalidation;
**only a cfg(test) constructor enables mutation**. A separate consent command
contract now prepares plans, but approval still cannot invoke production mutation. Windows excludes the
macOS module; existing read-only contracts continue compiling. No client CLI or
MCP process is called. There is no arbitrary frontend path parameter.

### Plans and admission

An internal plan binds a random 128-bit ID, client, add-only operation, user scope,
fixed `figcon-figma-console` name, exact derived private Node/MCP argv, target path,
expected absent registration, source SHA-256 and device/inode/permission/time
identity (or file absence), parent/data directory identities, managed executable
and entrypoint fingerprints, and candidate digest. Plans expire after five minutes
using monotonic time. A private engine-instance catalog authenticates the complete
binding and expiry and atomically consumes it once; edited, expired, replayed or
foreign-engine plans fail. Restart cannot reuse an approval catalog. This is an
internal qualification object, **not** an implemented user-consent workflow.

The launch fingerprints are bounded streaming hashes of regular private payload
files; they do not execute Node or JavaScript. A future host must additionally
require the existing installer structural/integrity checks and approved versions
before preparing an executable user plan. Current tests provision synthetic,
non-runnable payload files, including binary data.

A persistent per-client lock file under the private transaction root uses macOS
flock across cooperating processes. Creation is exclusive, followed by a validated
open when it already exists. Lock files are never removed. Admission is nonblocking;
busy operations consume their old plan and require fresh approval later. All target
inspection is repeated while admission is held. No native Claude lock is used and
no coordination with uncoordinated Claude/Codex writers is claimed.

### Transaction sequence and artifacts

The test constructor verifies a private isolated home and managed data root. Targets
are derived only as that home's `.claude.json` or `.codex/config.toml`; target parents
must already exist. No project directory or target parent is created automatically.

1. Validate the immutable plan and claim it. Acquire the Figcon lock, inspect the
   current source and payload fingerprints, and reject any mismatch or conflict.
2. Build a fresh candidate with the client-specific editor; its digest must equal
   the approved candidate. The JSON editor uses serde_json RawValue spans after
   full duplicate-key-rejecting validation, inserting only a property without
   reprinting existing fields. Codex uses pinned `toml_edit` document editing;
   unsupported inline/dotted server table forms fail rather than being normalized.
   Both editors parse the candidate and compare its entire semantic document to
   exactly the baseline plus the one intended entry. Equivalent external Figma
   registrations are conservatively conflicts, not implicit migration targets.
3. Create private `<managed-data>/client-config-transactions/<id>/` and persist
   `prepared.json`. Before touching an existing target, exclusively create
   `backup.bin` with **exact source bytes**, mode 0600, fsync it and its parent,
   and verify its digest. Backup creation/verification failure prevents mutation.
   No backup is needed for an absent source. Backups can contain credentials and
   account information: they are private recovery evidence, never logs or repo
   artifacts, and are never automatically restored.
4. Exclusively create a random-ID sibling `.figcon-<id>.candidate` with mode 0600,
   write/sync it and its parent, and persist `applying.json`. It remains private
   during backup and waiting. Revalidate expiry, target/payload fingerprints,
   parent/data identities and the candidate's own identity/bytes. Keep the candidate at 0600 through publication, verify uid/gid compatibility and
   recheck. If the inherited group cannot preserve the original permissions, fail
   before replacing anything.
5. Use descriptor-relative `renameatx_np`: no-replace for absence, atomic replacement
   for an existing qualified test fixture. Through a held owned candidate handle,
   restore the original POSIX mode only after publication, sync that file and the
   target parent. Retained sibling candidates therefore stay 0600 even on late
   expiry/failure. A published file may briefly be more restrictive (0600); a
   permission-finalization failure is uncertain and never triggers restoration.
   The source's final check is **not** a compare-and-swap.
6. Reopen and reparse the target. Verify candidate device/inode, digest, ownership
   and permissions and the exact semantic delta. Post-rename ctime is not required
   to equal pre-rename ctime; the final observed identity is recorded in the receipt.
7. Only after successful mutation and verification, exclusively write/sync
   `receipt.json`, then write/sync `verified.json` containing the receipt digest.
   Any failure after publication returns a sanitized uncertain outcome. No rollback,
   automatic retry, removal or backup restoration occurs.

All target/candidate/backup/receipt IO uses no-follow descriptor-relative opens,
regular-file and hard-link checks, owner/permission checks and bounded reads.
Extended ACLs, file flags, and unsupported xattrs are rejected. macOS automatically
attaches `com.apple.provenance` even to new temporary files; this bookkeeping
attribute is permitted but its value is not authenticated/copied as a setting.
Every other xattr is unsupported rather than silently discarded. POSIX permissions
are preserved; full platform metadata fidelity is not claimed. Parent identity
checks detect observed relocation/replacement but cannot guarantee continued
ancestry between syscalls.

### Receipts and non-destructive recovery

The engine's private receipt binds the approved binding, exact registration digest,
and observed post-write content/file identity. Receipt verification requires matching
prepared/applying journals and the final receipt-digest marker, fixed client/user
scope/name/target, current exact full-file bytes/identity, exact entry, directory
anchors and current managed payload fingerprints. Missing, corrupt, incomplete,
modified or stale evidence never authorizes ownership. A same-command external
entry cannot mint a receipt. Any unrelated subsequent config update also makes
this conservative receipt stale. Receipts offer no resistance to deliberate
same-user forgery or undetectable identical-state ABA.

`recover(id)` is read-only and idempotent: verified complete receipt -> verified;
exact pre-state -> not-applied; exact candidate bytes without settled evidence ->
possibly-applied; everything else -> needs-review. It never restores or deletes
anything. A crash between config publication and receipt settlement cannot be
resolved into proven ownership just by finding matching bytes. All transaction
artifacts and orphan sibling candidates remain for review; there is no recursive
cleanup, candidate unlinking, receipt repair or whole-file rollback. Recovery
inspection of unknown IDs must not initialize transaction directories.

### Qualification tests and production gates

The added tests cover both client formats: missing-file no-replace creation,
existing-file addition, exact private backups, POSIX modes, TOML comments/unknown
settings, JSON untouched spans, immutable/expired/foreign/replayed plans, changed
hash/inode/parent/payload, unsafe file types/links/xattrs, malformed/duplicate or
conflicting configuration, permission/backup failures, competing engines, final
reinspection, post-publication verification failure, receipt persistence failure,
stale/incomplete receipts, secret-safe diagnostics and repeated recovery.
A true separate Rust test process holds the transaction lock at a commit barrier;
parent admission fails busy, SIGKILL of that owned child leaves unchanged config
and retained evidence, and a fresh plan succeeds after lock release. Injected
unwinds additionally cover interruptions before and after publication. These are
not sudden power-loss or every-syscall crash tests.

Before enabling production writes: approve the existing-file concurrency policy
(or limit activation to create-only), provide trusted-root construction and
full managed-installation validation, implement actual immutable preview/explicit
approval binding with bounded plan admission and window/session ownership, integrate
qualified receipt inspection without adopting external registrations, qualify
ACL/xattr and editor compatibility on supported filesystems, define backup privacy,
retention/storage limits and user-controlled recovery, and add isolated consent/API
acceptance tests. There is no automatic backup reclamation. The engine is ready for
review and further consent-layer testing, **not production activation**.

Qualification validation on macOS arm64: formatting and diff checks passed; locked
Rust tests passed (173 passed, 7 opt-in tests ignored), including 20 engine test
entries with the subprocess helper; locked backend check passed; frontend tests
passed (30); frontend production build and macOS desktop build without bundling
passed. Existing compiler warnings remain. Windows execution and power-loss
durability were not verified locally. No production config, client CLI or MCP
server was used by these tests.


## Implemented consent layer and activation qualification

Production mutation remains **disabled by backend code**, with two independent
barriers. The consent approval implementation contains no production call to
`Engine::apply`; that branch is compiled only for synthetic tests. The engine's
production `preview_only` constructor independently sets a private disabled mutation
mode. There is no frontend feature flag, environment variable or request field
that can enable either barrier. Production API qualification does not authorize
activating those writes. No user configuration was changed to validate this layer.

### Actual consent lifecycle

The new `ConsentService` is managed by Tauri alongside the installation service.
A prepare request accepts only a closed client enum. Trusted roots come from Tauri
home/app-data resolution and the existing installer paths, not frontend data.
Preparation requires an unambiguous executable candidate, supported default user
configuration, no recognized registration/conflict, and a structurally valid pinned
managed Node/npm/MCP installation. The engine additionally fingerprints the actual
Node and entrypoint files and binds source contents/identity and directory anchors.
No client CLI or MCP server is executed. Structural inspection/fingerprinting does
not establish a new independent authenticity attestation of all installed bytes.

`prepare_client_configuration` returns an immutable plan-derived `ConsentPreview`:
opaque unpredictable plan ID, client/name, proposed server name, exact executable
and arguments, user scope, add operation, registration state, a summary explaining
private backup retention, static warnings, expiration and aggregate recovery usage.
It never returns unrelated registration fields, credentials, config fingerprints,
backup contents or recovery paths. Old informational inspection previews remain
separate and still report `safeToApply: false`.

`approve_client_configuration` accepts **only planId**. Calling this distinct
command is the explicit approval intent; no command called by detection, installation,
startup or refresh invokes it. Tauri supplies the actual originating window label.
The service verifies ID/owner/expiry/pending state under a short mutex, atomically
consumes the ID, then releases locks before filesystem work. Repeated approvals,
including concurrent submissions, cannot publish twice. Wrong-window attempts
cannot consume another window's plan. Source/root/payload changes or missing
prerequisites invalidate approval; capacity is checked again. Production returns a
sanitized `activation-disabled` error, and the consumed plan cannot be retried.
User rejection has its own planId-only command and discards the pending binding.
No rollback or removal command is exposed.

The registry binds backend-instance lifetime and originating window as well as all
engine plan fields. Plans expire after five monotonic minutes. Wall-clock expiry
is display-only, conservatively rounded. Admission prunes expired records, and a
Weak-owned 30-second worker prunes memory even when idle; it never inspects/deletes
recovery files. At most 16 plan records/work slots are admitted; per-plan accounted
payload is capped at 64 KiB, total registry payload at 1 MiB. These bound stored
payload and concurrent qualifying work, not exact allocator overhead or RSS.
Consumed/rejected records retain only bounded tombstone data until expiry. A new
backend instance has no valid old IDs. Filesystem work runs on blocking workers
without holding registry mutexes; kernel IO is still not forcibly preemptible.

The thin TypeScript consent client offers prepare/approve/reject/recovery methods.
It has no initialization side effects or retry/fallback that could imply consent.
No onboarding UI or functional production write button has been added. A future UI
must explicitly map warnings, show the backup/concurrency implications, and never
call approval while mounting, refreshing or handling an installation event.

### Recovery artifact accounting and admission

Accounting is metadata-only, bounded and non-destructive. It inspects the private
`client-config-transactions` tree plus sibling `.figcon-*.candidate` files in the
two supported user target parents. Names select accounting candidates, **not
ownership or deletion authority**. Transactions are counted regardless of success,
settlement or recognizable names; unowned data is never silently excluded to gain
capacity. Sibling candidates count independently as retained artifacts, so failed
attempts can consume more than one slot. Known persistent zero-length locks do not
consume artifact slots but count as entries. Nonempty/unknown root files, symlinks,
hard links, nonprivate modes, extended ACLs, unsupported xattrs, inaccessible paths
or exceeded traversal bounds produce `unknown` and block admission. Nothing is
followed, repaired or reclaimed. Platform provenance bookkeeping remains permitted
under the engine's existing metadata policy.

Limits for new transactions:

| Resource | Limit / reservation |
| --- | --- |
| Retained artifact count | 32; a new transaction needs one available slot |
| Apparent logical data bytes | 64 MiB total, reserving 12 MiB for the next attempt |
| Filesystem entries | 4,096, reserving eight for the next attempt |
| Free disk margin | At least 64 MiB plus the 12 MiB reservation |
| Traversal | Eight nested levels, bounded directory enumeration |

Sparse files are charged their logical length; hard links are rejected. Counts
and bytes are returned as aggregate optional values, with `available`, `full` or
`unknown`, limits and `automaticCleanupEnabled: false`. Unknown inspection returns
no invented zero usage. Missing application-data roots currently return an
unsupported/unavailable inspection error rather than creating storage. No config,
backup/receipt contents, filenames or private recovery paths leave the backend.

Prepare and approve perform read-only capacity checks. The qualification mutator
rechecks under a **global cross-process capacity lock**, held through settlement,
in addition to its per-client operation lock. Thus cooperating Figcon attempts
cannot both spend the same observed allowance. Locks are nonblocking and never
removed. They coordinate Figcon only; aggregate quotas/free-space estimates remain
best-effort safeguards, not hard containment against other writers, filesystem
quotas, compression or kernel behavior. Unresolved artifacts and backups are never
automatically deleted. Exhaustion requires manual review and future safe,
user-controlled reclamation; success does not exempt a transaction from accounting.

Private recovery directories are owner-verified 0700; files are 0600. The engine
now retains every sibling candidate at 0600 until it becomes the published target.
It restores original file mode via its owned open handle after successful rename,
then verifies the resulting mode and registration. This avoids leaving a readable
copy after late plan expiry. A failure in permission finalization remains an uncertain
possibly-applied transaction with retained artifacts, never a whole-file restore.

### Concurrency policy and remaining activation blockers

Accepted qualification model: Figcon serializes its own writers; other Claude/Codex
writers may not participate. Fingerprints, inode/directory identities and final
reinspection catch observed changes but cannot eliminate a later read/modify/write
race or guarantee continued ancestry. Existing-file mutation needs fresh explicit
consent and a verified exact private backup. Missing-file publication uses no-replace.
Conflicts and stale previews fail closed. These are not transactional guarantees
against uncoordinated writers, and this documentation is not approval to enable
production best-effort existing-file replacement.

Remaining review gates: explicit approval of create-only versus quiescent existing-
file policy and disclosure; managed-payload authenticity expectations; production
permission/ACL/xattr compatibility; backup privacy/retention and safe manual
reclamation; qualified receipt adoption without external ownership inference;
crash/power-loss behavior and mode-finalization failure; and end-to-end explicit
consent/window lifecycle acceptance. Windows execution remains unverified locally,
with platform-specific mutation code gated. No gate may be enabled by frontend
state or a user-supplied executable/configuration argument.

Recommended UI sequence after these reviews: inspect prerequisites -> request a
plan -> render that exact preview and retention/concurrency warnings -> separate
Approve or Decline action -> show disabled/expired/stale/uncertain outcomes without
automatic retry -> request a new preview for any changed state. Only after a
separate backend activation review may approval settle into a verified registration;
Figma-plugin onboarding and client session connectivity remain separate steps.

Consent qualification validation: locked Rust suite 185 passed / 7 opt-in tests
ignored; frontend suite 32 passed. Formatting, locked backend check, frontend
production build, macOS desktop build without bundling, diff and secret/artifact
checks passed. Existing compiler warnings remain. Windows execution and real-client
production writes were not tested or enabled. Synthetic tests cover both gates,
expiry/ownership/replay, stale inputs, registry/count/byte capacity, rejection,
uncertain receipt persistence and the engine's publication-failure regressions.

## Client review UI qualification

The [AI client review view](client-connection-ui.md) implements discovery, exact
backend-plan review, explicit confirmation, approval-contract submission and decline.
Production mutation remains disabled; the UI reports the blocked outcome without
claiming connection success. Installer progress remains mounted across navigation.
This implements the review flow only and does not satisfy or change the production
activation gates above. Isolated native desktop acceptance remains required.
