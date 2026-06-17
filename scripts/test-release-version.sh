#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

calculate() {
  local message=$1
  shift
  printf '%s\n' "$message" > "$tmp/message"
  printf '%s\n' "$@" > "$tmp/tags"
  "$root/scripts/release-version.sh" "$tmp/message" "$tmp/tags"
}

assert_output() {
  local expected=$1
  shift
  local actual
  actual=$("$@")
  if [[ "$actual" != "$expected" ]]; then
    printf 'expected:\n%s\nactual:\n%s\n' "$expected" "$actual" >&2
    exit 1
  fi
}

assert_output $'skip=false\ntag=v0.1.1' calculate 'Fix typo'
assert_output $'skip=false\ntag=v0.2.0' calculate 'feat: add scheduling'
assert_output $'skip=false\ntag=v2.0.0' calculate 'feat!: replace CLI' v1.4.3
assert_output $'skip=false\ntag=v2.0.0' calculate \
  $'fix: retain CLI\n\nBREAKING CHANGE: remove old flags' v1.4.3
assert_output $'skip=false\ntag=v1.5.0' calculate 'feat(cli): add command' v1.4.3
assert_output $'skip=false\ntag=v1.4.4' calculate 'docs: update readme' v1.4.3
assert_output $'skip=false\ntag=v2.1.0' calculate \
  'feat: use highest tag' v1.9.9 invalid v2.0.4
assert_output $'skip=true' calculate 'docs: no release [SKIP RELEASE]' v2.0.4

ci="$root/.github/workflows/ci.yml"
test -f "$ci"
grep -Fq 'contents: read' "$ci"
grep -Fq 'libasound2-dev' "$ci"
grep -Fq 'pkg-config' "$ci"
grep -Fq 'cargo test --locked' "$ci"
grep -Fq 'cargo clippy --locked --all-targets -- -D warnings' "$ci"
grep -Fq 'bash scripts/test-set-release-version.sh' "$ci"

release="$root/.github/workflows/release.yml"
test -f "$release"
grep -Fq 'contents: write' "$release"
grep -Fq 'cancel-in-progress: false' "$release"
grep -Fq 'workflow_run:' "$release"
grep -Fq 'workflow_dispatch:' "$release"
grep -Fq 'houseabsolute/actions-rust-cross@v1' "$release"
grep -Fq 'actions/upload-artifact@v5' "$release"
grep -Fq 'actions/download-artifact@v5' "$release"
grep -Fq 'SHA256SUMS' "$release"
grep -Fq 'gh release upload' "$release"
grep -Fq 'args: --locked --release --no-default-features' "$release"
grep -Fq 'scripts/set-release-version.sh "$tag"' "$release"
grep -Fq 'git add Cargo.toml Cargo.lock' "$release"
grep -Fq 'git commit -m "chore: release $tag [skip release]"' "$release"
grep -Fq 'git tag "$tag" HEAD' "$release"
grep -Fq 'git push --atomic origin HEAD:master "$tag"' "$release"

readme="$root/README.md"
grep -Fq 'cargo install clck --locked' "$readme"
grep -Fq 'https://github.com/gkk-dev-ops/clck/releases/latest' "$readme"
grep -Fq 'clck-vX.Y.Z-linux-x86_64-musl.tar.gz' "$readme"

releases="$root/docs-site/development/releases.md"
test -f "$releases"
grep -Fq '[skip release]' "$releases"
grep -Fq 'workflow_dispatch' "$releases"
grep -Fq 'contents: write' "$releases"
grep -Fq 'gh workflow run Release' "$releases"
grep -Fq 'https://github.com/gkk-dev-ops/clck/releases' "$releases"
grep -Fq 'SHA256SUMS' "$releases"

docs_config="$root/docs-site/.vitepress/config.mts"
grep -Fq '{ text: "crates.io", link: "https://crates.io/crates/clck" }' "$docs_config"
grep -Fq '{ text: "Releases", link: "https://github.com/gkk-dev-ops/clck/releases" }' "$docs_config"

installation="$root/docs-site/guide/installation.md"
grep -Fq '[crates.io](https://crates.io/crates/clck)' "$installation"
grep -Fq 'https://github.com/gkk-dev-ops/clck/releases)' "$installation"

echo "release-version tests passed"
