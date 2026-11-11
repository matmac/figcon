# Installation recovery and resource safeguards

Recovery is internal Rust functionality, independent of external MCP client
configuration. It runs before production attempt allocation and is available as
`recovery::recover_installations`; there is no recovery Tauri command or UI.
Status inspection remains read-only.

## Supported threat boundary

Figcon supports accidental corruption, crashes and interrupted installations,
malformed or malicious downloaded archives, concurrent **cooperating Figcon
processes**, and unexpected filesystem state. It does **not** claim isolation
from a malicious process already running with the same user's permissions.
No privileged helper or separate security service is introduced.

Attempt IDs and SHA-256 seals detect collisions and ordinary modification;
they are not authentication against another same-user process that can rewrite
both records and payloads. Advisory locks coordinate cooperating Figcon
processes, not hostile writers. Pinned checksums and lockfile integrity do not
prove publisher trust, and npm ci is not complete supply-chain security.

## Durable ownership and lifecycle

Each attempt records a random 128-bit ID, installation kind, creation time,
macOS PID/birth timestamp and boot-session UUID, lifecycle, filesystem
identities, and a content seal. Private JSON records and exclusive `.lease`
locks live in `metadata/attempts`, outside mutable staging. Directories require
current-user ownership and 0700 permissions; records/leases require 0600.
Metadata is bounded, parsed strictly, atomically published, and synchronized.

Lifecycle: `created` → `running` → `prepared` → `activating` → `activated`.
Dropping prepared work attempts to record `cancelled`. Legacy `cleaning`
records remain readable, but this version never enters a deletion lifecycle.
Quiescent checkpoints hash contents, relative names, identities, size,
timestamps, link counts, and exact approved runtime alias targets. Unknown
entries, hard links, special files, changed ownership/permissions, and excessive
inventories fail closed. MCP staging permits no symlinks; only the three pinned
Node aliases are allowed in runtime staging. Retention does not create a new
seal or adopt partial/externally modified contents.

## Recovery is non-destructive

**Automatic deletion of abandoned attempts is disabled**, including attempts
whose owner is demonstrably dead and whose seal matches. A persisted hash alone
is insufficient authority to recursively adopt an abandoned directory. Valid
stale attempts receive `manual-review`; live/locked attempts, unknown ownership,
unknown liveness, running/unsealed attempts, and modified attempts are retained
with their respective reasons. Recovery never launches commands, signals
processes, or deletes from `runtime`, `mcp`, staging, or the ledger.

PID reuse, boot mismatch, inspection errors, corrupt/missing metadata, symlinks,
and replacements all preserve uncertainty. Age alone never authorizes deletion.
Repeated recovery preserves these resources. Insertion into an abandoned tree
never authorizes cleanup. Orphan ledger records/leases/temporaries are counted
and reported without automatic removal or logging their names/contents.
Enumeration is bounded to 128 staging candidates and 1,024 ledger entries.
A crash after staging removal but before ledger cleanup therefore remains
visible on subsequent recovery scans.

## All automatic staging cleanup is disabled

**No Figcon staging tree is automatically deleted**, even when empty, sealed,
owned by a live handle, or left over after successful activation. Descriptor-
relative deletion prevents symlink redirection but cannot establish continued
ancestry: an opened nested directory, attempt ancestor, staging root, or managed
root can be renamed elsewhere while its descriptor remains usable. Additional
pathname/inode checks cannot close that race. Safety therefore takes priority
over automatic reclamation; there is no recursive cleanup implementation or
staging unlink primitive in the production installer.

A live handle's retirement returns an explicit `RetainedForReview` disposition
and emits a generic retention diagnostic, without paths or configuration.
Successful activation remains successful when retention is intentional. Primary
installation failures include a retention diagnostic; ownership/ledger errors
are still reported separately. Receipts, lease paths, npm cache/temp/home,
partial extractions, and verified or partial Node archives remain intact.
The live advisory lease is released when its file handle closes. Retention does
not modify an attempt's payload or delete its ledger records.

Only read-only descriptor helpers remain in `anchored.rs`, for inspection and
the orphan-ledger report. Recovery never reclaims retained data. Manual review
must establish ownership and absence of writers before removing anything; no
destructive repair command or automatic retention expiry is provided.

Remaining production filesystem mutations are deliberate installation writes,
exclusive creation, permission changes during extraction, reconstruction of
approved aliases, atomic no-replace activation, and atomic journal publication.
Journal publication still replaces its own JSON record and removes its own
publication temporary after the initial hard-link publish. npm ci/cache internals
can remove project/cache entries inside the fresh private attempt. These are not
Figcon staging-retirement cleanup, nor a general filesystem confinement claim.
No external MCP client configuration is modified. Tests explicitly remove only
their own isolated temporary fixtures; those operations are not production code.

## Admission, deadline, and subprocess supervision

A private root-level admission lock serializes recovery/counting and allocation
across cooperating processes. At most eight existing staging attempts are
admitted per managed root; **all** retained attempts, including successful
attempts, count toward that cap. Eight retained attempts block further allocation
pending manual review. The lock is released after allocation and durable initialization, not held during npm work.
Orphan ledger counts are separately reported.

Production Node and MCP installation run in bounded blocking workers (maximum
32 per process), so scanning, validation, sealing, and retention reporting do
not occupy the caller's async executor. One monotonic **600-second deadline** is shared through
preparation, checkpoint hashing, validation, and the final pre-activation check;
a staged handle keeps its original deadline. Async users of a staged handle
should call `activate_async`/`discard_async`; synchronous handle methods are for
worker-thread callers.
A dropped caller requests cancellation. Workers check cancellation/deadline
between entries, hash chunks, extraction entries, download chunks, and during
asynchronous waits. Quota monitoring continues every 250 ms plus scan time.

On macOS, subprocess exit is observed using `waitid(WNOWAIT)`: the owned leader
remains unreaped while descendants are inspected. Remaining ordinary process-
group descendants are terminated before the leader is reaped, avoiding signals
to a recycled numeric group ID. Cancellation/timeout also requests group
termination while ownership is held. A shared ownership lock lets a dropped
caller request group termination immediately, independently of worker scheduling.
Direct-child reaping has a five-second supervision bound; inability to terminate/reap is reported and staging retained.
Deliberately escaped process groups are outside the supported isolation model.
Recovery itself never terminates anything. Windows has no invented substitute
for these macOS mechanisms; platform-specific code/tests are gated.

## Best-effort MCP resource safeguards

| Resource | Limit per attempt |
| --- | --- |
| All staging regular-file usage | 1,280 MiB |
| Extracted MCP project | 512 MiB |
| Private npm cache | 512 MiB |
| Private temporary files | 128 MiB |
| Individual regular file | 64 MiB |
| Files/directories including attempt root | 100,000 |

Scans charge the greater of logical size and allocated blocks, reject hard
links/special entries, and run initially, periodically, on completion, and before
activation. npm children inherit a hard macOS `RLIMIT_FSIZE` of 64 MiB. Existing
Node limits remain: 100 MiB download, 500 MiB archive expansion, 20,000 archive
entries. Version probes retain 10-second/256-byte limits, npm inspection 30
seconds, and npm ci 600 seconds, within the overall attempt deadline.

Aggregate byte/count safeguards are **not hard containment**. Writers can
overshoot between scans; directory/inode overhead and open-but-unlinked files
are not fully accounted for. Concurrent attempts multiply per-attempt usage.
A blocked filesystem call cannot be preempted by cooperative cancellation;
worker limits bound outstanding work, not kernel latency. OS/filesystem quotas
would be a separate architectural decision.

Activation retains checksum verification, frozen lockfile resolution, private
npm isolation, disabled scripts, and atomic no-replace rename. Active payload
files are not reauthenticated against their original archives on every use.
Existing checksum-to-extraction and validation-to-execution pathname windows
remain outside a hostile same-user isolation guarantee. Activation, payload
persistence, and journal updates are not one power-loss transaction: a crash can
leave an active installation plus uncertain staging/ledger state. Inspection
reports structural damage; recovery does not overwrite or repair active data.

The deterministic rename-after-open regression opens and validates a nested
directory in the production retirement boundary, pauses on barriers, relocates
the directory outside the managed root, resumes, and checks every relocated file.
It covers nested, attempt-ancestor, staging-root, and managed-root moves, including
unexpected inserted contents. No deletion is attempted in any case.

Admission tests include twelve independent processes synchronized at a shared
gate: exactly eight allocations succeed and four are rejected. Existing tests
cover replacement/insertion, deadlines during hashing/validation, surviving
descendants, orphan reporting, and repeated non-destructive recovery. The opt-in [E2E test](installation-e2e.md) exercises the real pinned
runtime/package without system Node/npm or external client configuration changes.

The [managed installation API](installation-api.md) adds an operation-wide flock
for cooperating API callers, typed retention diagnostics, attempt-scoped progress,
and explicit cancellation sharing the activation boundary. It never enables
staging reclamation. Internal installer entry points retain their admission lock.
