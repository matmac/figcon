# Figcon engineering instructions

## Project and architecture

Figcon is an MIT-licensed, macOS-first desktop application for designers to install and manage Southleft's `figma-console-mcp`. Preserve the license and the existing Tauri 2, Rust, React, and TypeScript architecture.

- `src/` contains the React/TypeScript frontend, built with Vite. Use Tauri commands for backend operations; keep privileged filesystem, download, and process operations in Rust.
- `src-tauri/src/lib.rs` registers Tauri commands. `environment/` detects the platform and clients; `installer/` owns installation paths, discovery, status, download verification, extraction, staging, validation, activation, and cleanup.
- `src-tauri/tauri.conf.json` and `src-tauri/capabilities/` define desktop integration and permissions. Keep permissions limited to the work required.
- macOS is the current implementation target. Windows product support is deferred, but preserve the existing macOS/Windows CI matrix and Windows compilation/build behavior. Guard platform-specific APIs and tests appropriately; unsupported operations must fail clearly without side effects.

## Git and scope discipline

- Inspect the current branch, working tree, existing documentation, and relevant code before making changes. Treat tracked and untracked pre-existing work as user-owned; preserve it exactly.
- Never discard, overwrite, reset, stash, or clean away uncommitted work. Never force-push. Modify only files required by the authorized task; avoid incidental formatting or dependency changes.
- Never commit or push directly to `main`. Use feature branches, reviewed pull requests, and squash merges.
- Do not commit, push, open a pull request, or merge without explicit user approval for that action. An implementation request alone does not authorize these actions.
- Never commit credentials, API tokens, secrets, or sensitive user configuration. Use synthetic fixtures and redact sensitive values from logs, errors, screenshots, and reports.

## External configurations and installation boundaries

- Existing external MCP client configurations are user-owned. Discovery is read-only. Never create changes to, replace, delete, migrate, or otherwise modify an existing external client configuration without explicit user consent covering that change.
- Keep installer operations inside Figcon-managed directories. Resolve production paths through the existing installer path helpers: the app data directory's `figcon-installer` root contains `runtime`, `staging`, `mcp`, and `metadata`.
- Do not install globally, alter system runtimes or shell profiles, or reuse another application's installation as writable Figcon state. Tests must use isolated temporary directories and synthetic client configurations.

## Installer integrity and failure behavior

- Pin managed runtime and package versions to exact releases. Keep the installer manifest, managed npm manifest, and lockfiles consistent. Preserve committed application lockfiles; make dependency updates deliberate and reviewable.
- Verify downloads against trusted, pinned cryptographic checksums or package integrity values before extraction, execution, or activation. Do not bypass verification, silently fall back to unpinned versions, or accept a checksum supplied only by the same untrusted payload.
- Reject absolute paths, parent traversal, paths outside the expected archive root, duplicate/conflicting entries, unapproved symlinks, hard links, devices, and other unexpected filesystem entries. Check filesystem boundaries without following symlinks, enforce extraction limits, and fail closed on invalid input. The sole symlink exception is the pinned Node runtime's three exact executable aliases in `runtime_aliases.rs`: defer reconstruction until regular-file extraction and target validation finish; never unpack arbitrary archive links or allow symlinks in MCP package trees.
- Prefer atomic activation after complete integrity and runtime validation. Preserve existing installations on any failure, including concurrent attempts; never replace an active installation through a destructive fallback.
- Track ownership of resources created by each attempt. Cleanup may remove only resources owned by that attempt, after checking they have not been replaced. Never delete shared staging directories, another attempt's files, active installations, or user data to recover from an error.
- Keep installer modules small, testable, and independently verifiable. Separate download, verification, extraction, validation, activation, and cleanup responsibilities; allow deterministic tests without live downloads or real client configuration writes.
- Cover failure paths and boundaries with relevant tests, including unsafe paths and links, integrity failures, existing installations, concurrency, and cleanup ownership.

## Verification and reporting

After relevant implementation changes, run these commands from the repository root:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
npm run build
```

For changes affecting compilation, platform integration, or packaging, also run the applicable existing CI checks:

```sh
cargo check --locked --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --no-bundle
```

Use `npm ci` when dependency installation is needed. The frontend build runs TypeScript checking and Vite; no separate frontend test script is currently defined. CI runs Rust formatting checks, Rust tests, backend checks, and frontend/desktop builds on macOS and Windows. Never run an automatic formatter over unrelated or pre-existing work.

For documentation-only work, inspect the document and resulting diff; builds are unnecessary unless the documentation change affects executable behavior. Report which checks ran, their results, and anything skipped or blocked. Distinguish existing failures from regressions without changing unrelated work.

Report security concerns, platform limitations, and unresolved risks explicitly. Do not describe an installation flow as safe, verified, or cross-platform without evidence supporting that claim. Documentation requirements describe intended engineering behavior; they are not proof that existing code already satisfies them.
