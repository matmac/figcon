# Clean managed installation E2E test

On macOS arm64, explicitly run:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml clean_managed_installation_e2e -- --ignored --nocapture
```

This test requires access to the pinned Node.js distribution and npm registry.
It is compiled only for macOS arm64 and marked `#[ignore]`, so ordinary CI does
not download or install either dependency. It needs no cached runtime fixture,
Figma account, token, running desktop app, or external MCP client configuration.
Its temporary directory is removed when the test finishes.

The test calls the actual `install_node_runtime` and `install_mcp` functions.
Node.js is downloaded and checksum-verified by the production downloader,
extracted under the approved alias policy, validated with private Node/npm
probes, and atomically activated. The production MCP installer performs frozen
`npm ci` with lockfile integrity and lifecycle scripts disabled, validates the
physical and logical dependency tree and `dist/local.js`, then activates without
replacing an existing destination. MCP server code is never launched.

Assertions cover:

- Initially absent app-data parents and runtime/MCP directories.
- `not-installed` before installation and while only the runtime is present.
- Actual private Node.js/npm versions and absolute command paths; npm's PATH
  contains only the private runtime's `bin` directory.
- `error` for an incomplete active MCP directory, and refusal to overwrite it.
- Failure injection by emptying `dist/local.js` in a completed owned staging
  attempt. Activation must refuse it, clean that attempt, and leave Node intact.
- Recovery through a fresh production MCP installation and `installed` status.
- Every installed path, including reconstructed aliases, resolves inside the
  isolated managed root.
- Repeat Node/MCP installation refusal, with unchanged content hashes, inode
  identities, permissions, and symlink targets across the full active trees.
- Empty staging after completed failure/recovery and repeat attempts.

Reports include only phase/status results and public package/runtime versions,
not subprocess diagnostics, environment values, or installation paths. A useful
additional harness is to prepend failing `node`, `npm`, and `npx` executables to
the test process's PATH: the managed installation must still pass without ever
invoking those traps. Do not change the Rust test process's global environment
inside a parallel test suite.

## Installation status

The existing Tauri status command delegates to `status::inspect_installation`.
The same function is exercised by ordinary tests and this E2E test. It checks
the managed root's shape/Unix ownership and permissions, directory identities,
the runtime structure and npm metadata, the pinned manifests, installed package
versions/dependency metadata, and the nonempty MCP entry point. Missing MCP is
`not-installed`; partial, unreadable, or unsafe active state is `error`;
structurally complete managed state is `installed` with the pinned MCP version.

Status is strictly read-only and never executes Node.js, npm, or MCP. It is not
a new checksum attestation of installed bytes, proof that the runtime is benign,
or a functional MCP/Figma health check. Actual executable versions are checked
by the installation pipeline and explicitly reported by the E2E probes. Status
does not infer an installation is active merely because staging directories
exist; it cannot establish process liveness or diagnose an abandoned attempt.

## Release boundaries

This test proves the managed installation path, not a signed/notarized packaged
app release, MCP behavior against Figma, external client integration, x64
execution, or Windows support. Installation is not an OS sandbox. Existing
same-user pathname TOCTOU windows remain between checksum verification,
extraction, execution, activation, and cleanup; stronger guarantees require
descriptor-relative operations. A cancelled future retains private staging
while processes terminate. Automated recovery still needs ownership/liveness
tracking and must never delete another attempt or overwrite an active install.
Extraction does not yet enforce a live disk quota, and locked package integrity
does not establish publisher trust or complete supply-chain security.
