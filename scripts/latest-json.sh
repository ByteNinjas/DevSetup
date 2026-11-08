#!/usr/bin/env bash
# Generate or verify the static updater manifest (`latest.json`) for a release.
#
#   scripts/latest-json.sh generate <assets-dir> --version <version> [--tag <tag>]
#       [--repo <owner/repo>] [--notes <text>] [--date <rfc3339>]
#   scripts/latest-json.sh verify <assets-dir> <latest.json> [--repo <owner/repo>]
#       [--tag <tag>]
#
# `generate` refuses to emit a manifest unless every platform asset and its
# `.sig` are present, non-empty, and match the URL the updater will request.
# `verify` applies the same checks to an existing manifest. The release
# workflow runs both before it publishes the draft, so a missing signature or
# a mismatched tag fails the release instead of shipping a broken update.
#
# Requires bash and jq. The platform keys are the updater's
# `{os}-{arch}-{installer}` lookup strings.
set -euo pipefail

fail() { echo "latest-json.sh: $*" >&2; exit 1; }

usage() {
    cat >&2 <<'EOF'
usage:
  scripts/latest-json.sh generate <assets-dir> --version <version> [--tag <tag>]
      [--repo <owner/repo>] [--notes <text>] [--date <rfc3339>]
  scripts/latest-json.sh verify <assets-dir> <latest.json> [--repo <owner/repo>]
      [--tag <tag>]
EOF
    exit "${1:-2}"
}

root="$(cd -- "$(dirname -- "$0")/.." && pwd)"

# The updater target string and the asset Tauri produces for it. @V@ is the
# app version embedded in every bundle name.
PLATFORMS=(
    "linux-x86_64-appimage:DevSetup_@V@_amd64.AppImage"
    "linux-aarch64-appimage:DevSetup_@V@_aarch64.AppImage"
    "linux-x86_64-deb:DevSetup_@V@_amd64.deb"
    "linux-aarch64-deb:DevSetup_@V@_arm64.deb"
    "windows-x86_64-nsis:DevSetup_@V@_x64-setup.exe"
    "darwin-aarch64-app:DevSetup_@V@_aarch64.app.tar.gz"
    "darwin-x86_64-app:DevSetup_@V@_x64.app.tar.gz"
)

trim_sig() { tr -d '[:space:]' < "$1"; }

# verify_manifest <assets-dir> <manifest> <repo> <tag>
verify_manifest() {
    local assets="$1" manifest="$2" repo="$3" tag="$4"
    [ -f "$manifest" ] || fail "manifest not found: $manifest"
    jq -e . "$manifest" >/dev/null 2>&1 || fail "not valid JSON: $manifest"

    local version
    version="$(jq -er '.version' "$manifest")" || fail "manifest has no version"
    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] \
        || fail "version '$version' is not SemVer"

    jq -e '.pub_date | fromdateiso8601' "$manifest" >/dev/null 2>&1 \
        || fail "pub_date must be an RFC 3339 UTC timestamp"
    [ -n "$(jq -r '.notes // empty' "$manifest")" ] || fail "manifest has no notes"

    local expected actual entry target asset asset_path sig_path want_url url sig
    expected="$(for entry in "${PLATFORMS[@]}"; do printf '%s\n' "${entry%%:*}"; done | sort)"
    actual="$(jq -r '.platforms | keys[]' "$manifest" | sort)"
    [ "$actual" = "$expected" ] \
        || fail "platforms must be exactly: $(echo "$expected" | tr '\n' ' ')(got: $(echo "$actual" | tr '\n' ' '))"

    for entry in "${PLATFORMS[@]}"; do
        target="${entry%%:*}"
        asset="${entry#*:}"
        asset="${asset//@V@/$version}"
        asset_path="$assets/$asset"
        sig_path="$asset_path.sig"
        [ -s "$asset_path" ] || fail "missing or empty asset: $asset_path"
        [ -s "$sig_path" ] || fail "missing or empty signature: $sig_path"

        want_url="https://github.com/$repo/releases/download/$tag/$asset"
        url="$(jq -r --arg t "$target" '.platforms[$t].url // empty' "$manifest")"
        sig="$(jq -r --arg t "$target" '.platforms[$t].signature // empty' "$manifest")"
        [ "$url" = "$want_url" ] || fail "$target url must be $want_url (got: ${url:-<empty>})"
        [ "$sig" = "$(trim_sig "$sig_path")" ] || fail "$target signature does not match $sig_path"
    done
    echo "latest-json.sh: verified $manifest ($version, $tag, ${#PLATFORMS[@]} platforms)." >&2
}

# generate_manifest <assets-dir> <version> <tag> <repo> <notes> <date>
generate_manifest() {
    local assets="$1" version="$2" tag="$3" repo="$4" notes="$5" date="$6"
    [ -d "$assets" ] || fail "assets directory not found: $assets"

    local platforms='{}' entry target asset asset_path sig_path url item
    for entry in "${PLATFORMS[@]}"; do
        target="${entry%%:*}"
        asset="${entry#*:}"
        asset="${asset//@V@/$version}"
        asset_path="$assets/$asset"
        sig_path="$asset_path.sig"
        [ -s "$asset_path" ] || fail "missing or empty asset: $asset_path"
        [ -s "$sig_path" ] || fail "missing or empty signature: $sig_path"

        url="https://github.com/$repo/releases/download/$tag/$asset"
        item="$(jq -n --arg url "$url" --rawfile sig "$sig_path" \
            '{url: $url, signature: ($sig | gsub("\\s"; ""))}')"
        platforms="$(jq -c --arg t "$target" --argjson item "$item" '. + {($t): $item}' <<<"$platforms")"
    done

    jq -n \
        --arg version "$version" \
        --arg notes "$notes" \
        --arg pub_date "$date" \
        --argjson platforms "$platforms" \
        '{version: $version, notes: $notes, pub_date: $pub_date, platforms: $platforms}'
}

mode="${1:-}"
case "$mode" in
    -h|--help) usage 0 ;;
    generate|verify) shift ;;
    *) usage ;;
esac

assets=""
manifest=""
version=""
tag=""
repo="${GITHUB_REPOSITORY:-}"
notes=""
date=""

case "$mode" in
    generate)
        [ $# -ge 1 ] || usage
        assets="$1"; shift
        ;;
    verify)
        [ $# -ge 2 ] || usage
        assets="$1"; manifest="$2"; shift 2
        ;;
esac

while [ $# -gt 0 ]; do
    case "$1" in
        --version) version="${2:?--version needs a value}"; shift 2 ;;
        --tag) tag="${2:?--tag needs a value}"; shift 2 ;;
        --repo) repo="${2:?--repo needs a value}"; shift 2 ;;
        --notes) notes="${2:?--notes needs a value}"; shift 2 ;;
        --date) date="${2:?--date needs a value}"; shift 2 ;;
        *) fail "unknown argument: $1" ;;
    esac
done

[ -n "$repo" ] || fail "set --repo or GITHUB_REPOSITORY (owner/repo)"

if [ "$mode" = generate ]; then
    if [ -z "$version" ]; then
        version="$(jq -r .version "$root/src-tauri/tauri.conf.json")" \
            || fail "cannot read the app version from src-tauri/tauri.conf.json"
    fi
    [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] \
        || fail "version '$version' is not SemVer"
    tag="${tag:-${GITHUB_REF_NAME:-v$version}}"
    notes="${notes:-DevSetup $version is available. Release notes: https://github.com/$repo/releases/tag/$tag}"
    date="${date:-$(date -u +%Y-%m-%dT%H:%M:%SZ)}"

    tmp="$(mktemp)"
    trap 'rm -f "$tmp"' EXIT
    generate_manifest "$assets" "$version" "$tag" "$repo" "$notes" "$date" > "$tmp"
    verify_manifest "$assets" "$tmp" "$repo" "$tag"
    cat "$tmp"
else
    jq -er '.version' "$manifest" >/dev/null 2>&1 || fail "manifest has no version"
    version="$(jq -r '.version' "$manifest")"
    tag="${tag:-${GITHUB_REF_NAME:-v$version}}"
    verify_manifest "$assets" "$manifest" "$repo" "$tag"
fi
