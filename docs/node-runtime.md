# Pinned Node.js executable alias policy

Both official Node.js 24.22.2 macOS archives were checked against the manifest's
pinned SHA-256 values **before inspecting their entries**:

| Architecture | Archive | SHA-256 |
| --- | --- | --- |
| arm64 | `node-v24.22.2-darwin-arm64.tar.gz` | `aa9549349e47dffaa42dcf95d6e06ade8c4bdafc980979883b88767670df385a` |
| x64 | `node-v24.22.2-darwin-x64.tar.gz` | `d5e336d588c45be0c37dbd52cc29895593009ca4997a77a1fd02815131ed0e52` |

Sources are the exact versioned archives under
`https://nodejs.org/dist/v24.22.2/`. Both contain exactly these symlink entries,
relative to their `node-v24.22.2-darwin-{architecture}` archive root:

| Link path | Exact relative target |
| --- | --- |
| `bin/npm` | `../lib/node_modules/npm/bin/npm-cli.js` |
| `bin/npx` | `../lib/node_modules/npm/bin/npx-cli.js` |
| `bin/corepack` | `../lib/node_modules/corepack/dist/corepack.js` |

`runtime_aliases.rs` contains this fixed allowlist. Only the pinned release and
the macOS arm64/x64 archive roots may use it. Name and target bytes must match
exactly; absolute targets, other traversal, normalized spellings, unexpected
names or targets, nonzero link payloads, and hard links are rejected.

The production downloader verifies the archive checksum before the extractor
opens it. The extractor never unpacks a symbolic link. It records matching
alias headers while processing every other entry with the existing archive
root, traversal, duplicate-entry, file-count, size, and permissions checks.
Aliases still count toward the 20,000-entry limit. Regular-file content remains
limited to 500 MiB. No link exists while later archive entries are extracted.

Only after extraction and executable checks finish does reconstruction begin.
Each alias target must have been a regular-file entry in that same archive,
must still be a nonempty regular file, and must have real, in-runtime ancestors.
Symlinked ancestors, missing targets, hard-linked target files, and directory
targets fail closed. Links are then created from constants without replacement;
archive-supplied link bytes are never passed to the filesystem link operation.

Runtime validation permits only these exact aliases and verifies their targets.
MCP package trees retain the generic validator that rejects all symlinks. npm
is still invoked as private `bin/node lib/node_modules/npm/bin/npm-cli.js`, so
no system Node.js or npm is needed. An alias's normal `#!/usr/bin/env node`
shebang is not used by the installer.

## Regression and real-archive tests

Ordinary tests cover both architecture profiles, deferred alias reconstruction,
absolute/outside/wrong targets, wrong names, duplicates, missing targets,
symlinked target ancestors, directory targets, and entries below an alias.
Existing traversal, hard-link, duplicate-entry, and size protections remain
covered. MCP tree validation explicitly rejects the reconstructed runtime links.

Three opt-in tests match the `real_node_` filter:

- `real_node_archives_extract_without_executing_x64`: independently verifies
  both archive checksums, extracts them, validates aliases and Mach-O CPU
  headers, and never executes x64 binaries.
- `real_node_archive_first_run_prepares_and_activates`: verifies a real cached
  arm64 archive and its staging copy, then prepares, runs private Node/npm
  validation, activates, and cleans up under initially absent app-data parents.
- `real_node_production_network_first_run`: uses the production downloader,
  checksum verifier, extractor, validation, activation, and cleanup in a fresh
  temporary Figcon installation. It requires network access.

The cached-archive tests use `FIGCON_TEST_NODE_ARM64_ARCHIVE` and
`FIGCON_TEST_NODE_X64_ARCHIVE`. Arm64 execution tests are gated to arm64 macOS;
network/archive fixtures are never required by ordinary CI tests.

## Remaining limitations

Checksums authenticate bytes relative to the reviewed pins; they do not prove
that Node.js, npm, or future package code is benign. Hash verification and
extraction currently reopen pathnames, and some directory checks, link creation,
execution, activation, and cleanup still have same-user TOCTOU windows. Private
staging and directory identity checks reduce exposure but do not replace
descriptor-relative filesystem operations or an OS sandbox. The alias policy
does not expand support to Windows or establish x64 execution correctness.
