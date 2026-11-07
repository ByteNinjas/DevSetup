# AGENTS.md — DevSetup hard rules (auto-loaded)

Short, self-sufficient rulebook for AI agents. Depth and rationale live in
[CONVENTIONS.md](CONVENTIONS.md) — reading it adds depth, it is never required
for compliance.

## Trunks and pull requests

- Only two trunks exist: `dev` (integration) and `main` (release).
- **Never commit or push to the trunks (`dev`, `main`)** — no direct commits,
  no local merges into them, no `--force`, no resets, no branch deletions.
  All of it is refused locally by `.githooks/pre-push` and by CI guards.
- Cut task branches (`feat/… fix/… chore/… ci/… docs/… perf/… refactor/… test/…`) **from `dev`** and
  open PRs only into the integration trunk: `gh pr create --base dev`
  (release PRs `dev` → `main` are the only exception).
- Trunk merges are **merge commits only** — never squash, never fast-forward.
- **Never rewrite shared history** (no rebase/amend of commits already pushed
  to a branch others may build on).
- **Actions that cannot be undone are not options.** If you find yourself
  planning one, stop and present the reversible alternative.
- **Shared checkout:** work in a linked worktree
  (`git worktree add ../devsetup-<topic> -b <branch> origin/dev`) and leave
  another session's uncommitted files untouched.

## Destructive operations

- **Never drop, delete or truncate databases** (`*.db` files, stores, tables)
  and never bulk-`DELETE` rows. No destructive or reversed migrations —
  additive schema changes only. This includes the app's own persisted state.
- **Never commit secrets.** Env files stay local; commit only `*.example`
  templates with placeholders. If a secret lands, remove it, rotate it, and
  state it in the PR.
- **Regenerate; never hand-edit** generated outputs: `dist/`, `src-tauri/target/`,
  icons (`pnpm run icons`), lockfiles (`pnpm install` / `cargo` runs only),
  install-script pins (`pnpm run pin`).
- Bypass hooks only as a deliberate, stated exception (recorded in the PR body).

## Dependencies, audits and releases

- **pnpm is the JavaScript package manager** (`packageManager` in `package.json`);
  `pnpm-lock.yaml` is the only JS lockfile. Never add a `package-lock.json` or
  `yarn.lock`.
- **Keep dependencies on the latest stable releases and runtimes on LTS.**
  Update through `pnpm` / `cargo`; never hand-edit a lockfile.
- **Audits are mandatory.** `deps-audit` runs `pnpm audit --prod`, `cargo audit`
  (reviewed `.cargo/audit.toml` exemptions), OSV over both lockfiles, and the
  install-script pin check. Local parity: `scripts/deps.sh`.
- **Releases are tag-driven.** A `v*` tag builds the supported platforms
  through `.github/workflows/release.yml`: Linux x64 + arm64 (`.deb` + AppImage,
  the full feature set), Windows x64 (`.exe`/NSIS) and macOS arm64/x64
  (`.dmg`). Windows and macOS are **preview** builds: the UI, setup files, Git
  and VS Code helpers work, while apt, pkexec, Flatpak and other Linux-only
  actions stay disabled. `.github/workflows/ci.yml` compiles both preview
  targets on every PR before a tag can be cut.

## Verify before push

- **Run the gate before every push:** `pnpm run check`
  (= `tsc --noEmit && vite build && cargo test`).
- Enable hooks once after cloning: `git config core.hooksPath .githooks`.
  The hooks then enforce trunk refusals, Conventional Commits subjects and the
  gate automatically.
- Commit subjects follow Conventional Commits: `chore(conventions): example`.

## Command map

| Task | Command |
|---|---|
| Install node toolchain | `pnpm install --frozen-lockfile` |
| The gate (before every push) | `pnpm run check` |
| Rust tests only | `cargo test --manifest-path src-tauri/Cargo.toml` |
| Install-script pin check (network) | `pnpm run pin:check` |
| Dependency audit parity (network / `--offline`) | `scripts/deps.sh` |
| Enable hooks | `git config core.hooksPath .githooks` |
| Ship work | push task branch, `gh pr create --base dev` |

## Where the docs live

- `AGENTS.md` (this file) → [CONVENTIONS.md](CONVENTIONS.md) (canonical rule
  book) → [ROADMAP.md](ROADMAP.md) (plans and working notes).
- [README.md](README.md) is product facts only.
