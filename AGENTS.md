# dash-agent - Project Instructions for Codex

## Critical Thinking (MANDATORY)
Before implementing any request, you MUST:
- **Evaluate tradeoffs**: Analyze pros/cons of the approach, consider alternatives, and mention what could go wrong.
- **Be critical**: If an idea has flaws, edge cases, or better alternatives, say so directly. Do not just agree and implement blindly.
- **Ask when in doubt**: If requirements are ambiguous or you see potential issues, raise them before writing code. It's better to debate first than to rewrite later.
- **Challenge assumptions**: If the user's approach could cause problems (performance, UX, maintainability), flag it explicitly.

## Project Overview
Dashboard web para gestionar tareas de un agente IA autonomo. Permite crear tareas, monitorear su ejecucion en tiempo real via SSE, enviar feedback al agente, revisar cambios (diff) y aprobar la creacion de PRs.

## Project Structure
```
dash-agent/
├── packages/
│   ├── cli/              # npx agent-board CLI wrapper
│   ├── dashboard/        # Vite + React SPA frontend
│   ├── server-rs/        # Rust + Axum backend API
│   └── shared/           # Shared types and schemas
├── package.json          # Root workspace
└── AGENTS.md
```

### CLI Integration Cross-Check (MANDATORY)
When modifying CLI-specific code in `packages/server-rs/src/agent/cli_runner.rs` or `packages/server-rs/src/services/agent_detection_service.rs`, you MUST verify that changes for one CLI type do NOT affect others. The system supports multiple CLI agents (Codex, codex, copilot, gemini), each with:
- **Independent command building** in `build_cli_command()` (separate match arm per agent)
- **Independent output parsing** in `parse_output_line()` (separate parser per agent)
- **Independent model lists** in `agent_detection_service.rs`

After any CLI-specific change:
1. Verify the modified match arm is self-contained (no shared variables/constants affected)
2. Confirm other CLI match arms remain untouched
3. Check that output parser routing in `parse_output_line()` correctly maps each agent type

## Local Binary Testing (Rust Server)

After implementing a feature in `packages/server-rs/`, follow these steps to test it locally:

1. **Build** the release binary:
   ```bash
   cd packages/server-rs && cargo build --release
   ```
2. **Kill** any running instance:
   ```bash
   tasklist | grep -i agent-board
   taskkill //PID {PID} //F
   ```
3. **Copy** the binary to the local alias path:
   ```bash
   cp packages/server-rs/target/release/agent-board.exe dist/local/agent-board.exe
   ```
4. **Start** the server:
   ```bash
   ai-agent-board
   ```

The bash alias is defined in `~/.bashrc`:
```bash
ai-agent-board() { __BIN_MODE__=1 /c/ezequiel/dashboard-agentic/dist/local/agent-board.exe "$@"; }
```

## Release & Publishing Pipeline

### Overview
Releases are automated via GitHub Actions (`.github/workflows/release.yml`). Pushing a semver tag triggers:
1. **build** - Compiles 4 platform binaries, bundles frontend assets + sql-wasm.wasm, creates ZIPs, generates manifest.json with SHA256 checksums
2. **upload-r2** - Uploads ZIPs + manifest to Cloudflare R2 (`agent-board` bucket)
3. **publish-npm** - Publishes `ai-agent-board` to npm via OIDC trusted publishing (no tokens needed)

### How to Release
When the user asks to release or deploy to npm, follow these steps:

1. **Bump version** in `packages/cli/package.json` (this is the npm package version)
2. **Commit** the version bump
3. **Tag and push:**
   ```bash
   git tag v{VERSION}
   git push origin main --tags
   ```
4. **Monitor** the workflow:
   ```bash
   gh run list -w release.yml --limit 3
   gh run view {RUN_ID}           # check status
   gh run view {RUN_ID} --log-failed  # if something fails
   ```
5. **Verify** after all 3 jobs pass:
   ```bash
   npx ai-agent-board@latest
   ```

### Version Bumping Rules
- `packages/cli/package.json` version = release version (this is what npm publishes)
- Tag must match: `v{version}` (e.g., `v0.2.0`)
- Root `package.json` version is informational only (monorepo version)

### If a Release Fails
- **build fails**: Check compilation errors, fix, commit, delete tag, re-tag and push
- **upload-r2 fails**: Check R2 secrets in GitHub repo settings
- **publish-npm fails**:
  - "already published": Version was already published - bump version and re-release
  - OIDC errors: Check trusted publisher config at npmjs.com/package/ai-agent-board/access

### Deleting and Re-creating a Tag
If you need to re-release the same version (e.g., after fixing a build issue):
```bash
git push origin --delete v{VERSION}   # delete remote tag
git tag -d v{VERSION}                 # delete local tag
# ... fix and commit ...
git tag v{VERSION}                    # re-create tag
git push origin main --tags           # push
```