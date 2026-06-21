#!/usr/bin/env bash
set -euo pipefail

repo="${DUROOS_WATCHER_REPO:-ilm-seeker/duroos-watcher}"
tag="${DUROOS_WATCHER_TAG:-v0.1.0-alpha.3}"
install_dir="${DUROOS_WATCHER_INSTALL_DIR:-/Applications}"
asset="Duroos-Watcher-${tag}-macos-unsigned.app.zip"
checksum="SHA256SUMS-${tag}-macos.txt"
base_url="https://github.com/${repo}/releases/download/${tag}"
app_name="Duroos Watcher.app"
bundle_id="io.duroos.watcher"

usage() {
  cat <<'EOF'
Install the latest Duroos Watcher unsigned macOS alpha.

Usage:
  DUROOS_WATCHER_ACCEPT_UNSIGNED=1 bash install/macos.sh

Optional environment variables:
  DUROOS_WATCHER_TAG=v0.1.0-alpha.3
  DUROOS_WATCHER_INSTALL_DIR=/Applications
  DUROOS_WATCHER_REPO=ilm-seeker/duroos-watcher
  DUROOS_WATCHER_DRY_RUN=1
EOF
}

fail() {
  echo "error: $*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "$1 is required."
}

accept_unsigned_alpha() {
  if [[ "${DUROOS_WATCHER_ACCEPT_UNSIGNED:-}" == "1" ]]; then
    return
  fi

  cat >&2 <<'EOF'
Duroos Watcher alpha builds are unsigned and not Apple-notarized.
Only continue if you trust the repository and are comfortable testing unsigned software.
Set DUROOS_WATCHER_ACCEPT_UNSIGNED=1 to skip this prompt.
EOF

  if [[ ! -t 0 ]]; then
    fail "refusing non-interactive unsigned install without DUROOS_WATCHER_ACCEPT_UNSIGNED=1"
  fi

  read -r -p "Continue installing the unsigned alpha? [y/N] " answer
  case "$answer" in
    y|Y|yes|YES) ;;
    *) fail "install cancelled" ;;
  esac
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

[[ "$(uname -s)" == "Darwin" ]] || fail "this installer is for macOS only."
require_command curl
require_command shasum
require_command ditto
require_command codesign
require_command file
accept_unsigned_alpha

echo "Duroos Watcher ${tag} macOS unsigned alpha"
echo "Download: ${base_url}/${asset}"
echo "Install: ${install_dir}/${app_name}"

if [[ "${DUROOS_WATCHER_DRY_RUN:-}" == "1" ]]; then
  echo "Dry run only; no files were downloaded or installed."
  exit 0
fi

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

curl --fail --location --retry 3 --output "${tmpdir}/${asset}" "${base_url}/${asset}"
curl --fail --location --retry 3 --output "${tmpdir}/${checksum}" "${base_url}/${checksum}"

grep "  ${asset}$" "${tmpdir}/${checksum}" > "${tmpdir}/${asset}.sha256" \
  || fail "checksum file does not include ${asset}"
(cd "$tmpdir" && shasum -a 256 -c "${asset}.sha256")

mkdir -p "${tmpdir}/unzipped"
ditto -x -k "${tmpdir}/${asset}" "${tmpdir}/unzipped"

app_path="$(find "${tmpdir}/unzipped" -maxdepth 2 -type d -name "$app_name" -print -quit)"
[[ -n "$app_path" ]] || fail "the zip did not contain ${app_name}"

codesign --verify --deep --strict --verbose=2 "$app_path"

executable="${app_path}/Contents/MacOS/duroos-watcher"
if [[ -f "$executable" ]]; then
  binary_info="$(file "$executable")"
  case "$(uname -m)" in
    arm64)
      [[ "$binary_info" == *"arm64"* ]] || fail "downloaded app does not contain an arm64 macOS binary."
      ;;
    x86_64)
      [[ "$binary_info" == *"x86_64"* ]] || fail "current macOS alpha asset is not compatible with Intel Macs."
      ;;
  esac
fi

if [[ ! -d "$install_dir" ]]; then
  mkdir -p "$install_dir" 2>/dev/null || sudo mkdir -p "$install_dir"
fi

sudo_cmd=()
if [[ ! -w "$install_dir" ]]; then
  sudo_cmd=(sudo)
fi

destination="${install_dir}/${app_name}"
if [[ -e "$destination" ]]; then
  existing_id="$(/usr/libexec/PlistBuddy -c "Print :CFBundleIdentifier" "${destination}/Contents/Info.plist" 2>/dev/null || true)"
  [[ "$existing_id" == "$bundle_id" ]] || fail "${destination} exists but is not Duroos Watcher."
  "${sudo_cmd[@]}" rm -rf "$destination"
fi

"${sudo_cmd[@]}" ditto "$app_path" "$destination"

echo "Installed ${destination}"
echo "Open it with: open \"${destination}\""
