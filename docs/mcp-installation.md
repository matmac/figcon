# Staged MCP installation

The Rust installer exposes `mcp::stage_mcp` and `mcp::install_mcp` internally.
The [typed installation API](installation-api.md) wraps these production functions
with fixed-purpose Tauri commands, progress, status, and cancellation. The API supports
[guided desktop setup controls](installer-ui.md). They do not read or write external MCP client
configurations. Installation currently requires macOS; unsupported platforms return an error before creating staging resources.

## Contract and stages

The binary embeds `installer/npm/package.json` and `package-lock.json`. Before
installation, it checks the exact top-level version, registry tarball, executable
entry point, and SHA-512 integrity against `manifest.rs`. Every resolved
dependency must have an exact version, HTTPS npm-registry tarball URL, and
SHA-512 integrity. Local/git sources, links, and unsupported optional/bundled
lockfile entries fail closed. Published dependency *ranges* remain in metadata;
the concrete versions and tarball integrity values are frozen by lockfile v3.

1. Require the existing Figcon-managed Node.js 24.22.2 runtime. Validate its
   version and invoke `lib/node_modules/npm/bin/npm-cli.js --version` through
   `bin/node`. Check the returned npm version against the bundled npm manifest.
   The Node archive checksum pins the bundled npm distribution; Figcon never
   falls back to system Node.js, `npm`, or `npx`.
2. Allocate a private, uniquely named attempt under `figcon-installer/staging`.
   Create an `mcp` project and copy the embedded manifest and lockfile verbatim.
3. Invoke private Node.js with the absolute npm CLI path and `npm ci`, disabling
   lifecycle scripts, executable links, audit, funding, and update notifications.
   Clear the inherited environment, including `NODE_OPTIONS`, `NODE_PATH`, npm
   settings, credentials, proxies, and user home. Give npm attempt-owned home,
   config, prefix, cache, and temporary directories with empty user/global
   npmrc files. Stdin and subprocess diagnostics are discarded.
4. Reject symlinks, hard links, and special filesystem entries throughout the
   staged tree. Bound post-install inspection to 200,000 entries and 2 GiB;
   package metadata reads are bounded to 1 MiB. Verify unchanged root manifests,
   all installed dependency versions, the package graph, and the regular,
   nonempty `node_modules/figma-console-mcp/dist/local.js` entry point.
   Run private `npm ls --all --offline --json` to check the logical dependency
   tree. This does not execute MCP code or package lifecycle scripts.
5. Retain a staging handle until activation (or explicitly abandon it). Recheck
   ownership and the staged tree; activate using the same macOS `renameatx_np(RENAME_EXCL)` primitive as
   the runtime installer. An existing destination, including an empty directory
   or dangling symlink, is never replaced.
6. Retire the handle without deleting staging or its receipt/lease paths.
   Automatic staging cleanup is disabled because held directory descriptors do
   not guarantee continued ancestry after rename. Intentional retention is
   diagnosed and does not turn successful activation into an installation
   failure. Failed installation errors also report retained staging. Manual
   review is required before reclamation.

## Interruption and limits

Version probes have 10-second limits, `npm ci` has a 10-minute limit, and npm
dependency inspection has a 30-second limit. Stdout is discarded except for
version probes, which are limited to 256 bytes. On timeout the owned Unix
process group is terminated and direct-child reaping is bounded. macOS exit
observation keeps the leader unreaped until descendant supervision completes.
Dropping an installation future also signals the process group, including
ordinary descendants. A cancelled or abandoned staging handle retains its
private attempt and emits a generic warning: synchronous drop cannot safely
wait for all child processes to exit. Durable leases, non-destructive crash
recovery reporting, and best-effort byte/count/duration safeguards are described in
[installation recovery](installer-recovery.md). Unsealed or externally modified
attempts require manual review; retirement does not adopt or delete partial npm contents.

## Security boundaries and open decisions

- Integrity protects against tarball changes relative to the reviewed lockfile.
  It does not establish publisher trust or prove that locked code is benign.
  `npm ci` is not complete supply-chain security. Package code is not executed
  during installation, but will execute when a later client launches MCP.
- npm and its archive handling are trusted components of the checksum-pinned
  runtime. Installation is not an OS sandbox. Monitored size/entry limits can
  overshoot between scans and do not provide a hard aggregate filesystem quota. Runtime file contents can
  also be modified by another process with the same user's privileges.
- Directory handles and inode/device checks detect many replacements, but the
  remaining pathname operations have TOCTOU windows. Stronger defenses require
  descriptor-relative traversal, extraction, execution, rename, and cleanup.
  Process groups are cancellation containment, not a sandbox against processes
  that deliberately escape their group.
- The pinned official Node archives contain three approved executable aliases.
  The runtime extractor now defers their creation until every regular file and
  alias target has passed validation. The exception is limited to the exact
  pairs in [the runtime alias policy](node-runtime.md); MCP package trees still
  reject every symlink. npm continues to run through private `bin/node` and
  `npm-cli.js`, independently of the aliases.
- Scripts remain disabled even if a future dependency requires installation
  scripts. npm's tree validation and entry-point checks do not prove MCP's full
  runtime behavior. Any script exception needs an explicit security design.

## Tests

The opt-in [complete installation E2E test](installation-e2e.md) starts without
any runtime fixture and exercises both production installers, status, repeat
protection, corrupt-staging rejection, and recovery on macOS arm64.

Normal Rust tests use synthetic runtimes and registry package fixtures. They
cover lockfile mismatch, missing entry points, dependency mismatches, npm
failure, timeout, cancellation, symlink rejection, existing destinations,
concurrent activation, and cleanup ownership.

The ignored `live_registry_install_in_isolated_managed_root` test takes an
explicit `FIGCON_TEST_VERIFIED_RUNTIME` fixture, copies its regular files into a
fresh temporary Figcon root, and exercises the actual npm pipeline. The caller
must establish the fixture's pinned archive checksum/provenance first; version
checks alone do not authenticate it. Do not point this test at a user's active
installation or system runtime. Both an unmodified runtime produced by the
production extractor and the historical regular-file-only fixture are
supported. Runtime fixtures may contain only the three validated aliases; the
test reconstructs these from approved constants rather than following links
while copying. Production first-run preparation is covered separately by the
real-archive tests described in the runtime alias policy.

Command behavior references: [npm ci](https://docs.npmjs.com/cli/v11/commands/npm-ci/)
and [npm ls](https://docs.npmjs.com/cli/v11/commands/npm-ls/).

The supported cooperative same-user threat boundary, non-destructive live
attempt retirement, serialized admission, shared deadline, and disabled automatic
recovery deletion are defined in [the recovery policy](installer-recovery.md).
