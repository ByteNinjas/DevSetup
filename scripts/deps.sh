#!/usr/bin/env bash
# DevSetup dependency audit — local parity with
# .github/workflows/deps-audit.yml.
#
#   scripts/deps.sh            full audit (needs the network: OSV API, RustSec
#                              advisory DB, pnpm registry, install-script pins)
#   scripts/deps.sh --offline  skips everything network-bound and exits ok,
#                              telling you what was skipped
#
# The scanner is pinned by version AND sha256 (same constants as the workflow)
# and verified with `sha256sum -c -` before it is allowed to run.
set -u

fail() { echo "$1" >&2; exit 1; }

# Same cleanup as .githooks/pre-push: build tools that run their own `git`
# must describe this checkout, not the hook/worktree plumbing repository.
root=$(git rev-parse --show-toplevel 2>/dev/null) || root=""
[ -n "$root" ] || root="$PWD"
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX
cd "$root" || fail "deps.sh: cannot enter repo root $root"

offline=0
if [ "${1:-}" = "--offline" ]; then
    offline=1
elif [ -n "${1:-}" ]; then
    fail "usage: scripts/deps.sh [--offline]"
fi

OSV_VERSION=v2.7.0
OSV_SHA256=160c6253e2922e9e0a6a9771c5c4c40a4b3c6090b34a3cdd5e80c96cc2e4218b

if [ "$offline" -eq 1 ]; then
    echo "deps.sh: --offline — skipped osv-scanner, cargo audit, pnpm audit and pin:check."
    echo "deps.sh: all four need the network; run them before a dependency-affecting PR."
    exit 0
fi

command -v pnpm >/dev/null 2>&1 \
    || fail "deps.sh: pnpm is required (corepack enable pnpm, or npm install -g pnpm)"

tmp_dir=/tmp/opencode/devsetup-osv-scanner
mkdir -p "$tmp_dir" || fail "deps.sh: cannot create $tmp_dir"
curl -fsSL -o "$tmp_dir/osv-scanner-bin" \
    "https://github.com/google/osv-scanner/releases/download/${OSV_VERSION}/osv-scanner_linux_amd64" \
    || fail "deps.sh: could not download osv-scanner ${OSV_VERSION}"
printf '%s  %s\n' "$OSV_SHA256" "$tmp_dir/osv-scanner-bin" | sha256sum -c - \
    || fail "deps.sh: osv-scanner checksum mismatch — refusing the unexpected binary"
chmod +x "$tmp_dir/osv-scanner-bin"

echo "== OSV scan of pnpm-lock.yaml and src-tauri/Cargo.lock =="
"$tmp_dir/osv-scanner-bin" scan source \
    --config=.osv-scanner.toml \
    --lockfile=pnpm-lock.yaml \
    --lockfile=src-tauri/Cargo.lock \
    || fail "deps.sh: OSV findings above — bump the packages and regenerate the lockfiles; a new exemption needs a reviewed PR that adds it to .osv-scanner.toml with a reason"

echo
echo "== cargo audit (Rust deps) =="
command -v cargo-audit >/dev/null 2>&1 \
    || fail "deps.sh: cargo-audit is missing — install it with: cargo install cargo-audit --locked"
cargo audit --file src-tauri/Cargo.lock \
    || fail "deps.sh: cargo audit findings above"

echo
echo "== pnpm audit (production deps) =="
pnpm audit --prod --audit-level=moderate \
    || fail "deps.sh: pnpm audit findings above"

echo
echo "== install-script pins =="
node scripts/pin-scripts.mjs --check \
    || fail "deps.sh: pins diverged upstream — review the script diffs, then pnpm run pin"

echo
echo "deps.sh: all dependency checks passed."
