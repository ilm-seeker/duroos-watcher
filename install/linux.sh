#!/usr/bin/env bash
set -euo pipefail

repo="${DUROOS_WATCHER_REPO:-ilm-seeker/duroos-watcher}"
tag="${DUROOS_WATCHER_TAG:-v0.1.0-alpha.3}"
package="${DUROOS_WATCHER_PACKAGE:-auto}"
appimage_dir="${DUROOS_WATCHER_APPIMAGE_DIR:-$HOME/.local/bin}"
base_url="https://github.com/${repo}/releases/download/${tag}"

usage() {
  cat <<'EOF'
Install the latest Duroos Watcher unsigned Linux alpha.

Usage:
  DUROOS_WATCHER_ACCEPT_UNSIGNED=1 bash install/linux.sh

Optional environment variables:
  DUROOS_WATCHER_TAG=v0.1.0-alpha.3
  DUROOS_WATCHER_PACKAGE=auto|deb|appimage
  DUROOS_WATCHER_APPIMAGE_DIR=$HOME/.local/bin
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
Duroos Watcher Linux packages are unsigned alpha/testing artifacts.
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

select_package() {
  case "$package" in
    auto)
      if command -v dpkg >/dev/null 2>&1; then
        echo "deb"
      else
        echo "appimage"
      fi
      ;;
    deb|appimage) echo "$package" ;;
    *) fail "DUROOS_WATCHER_PACKAGE must be auto, deb, or appimage." ;;
  esac
}

download_and_verify() {
  local asset="$1"
  local checksum="$2"
  local expected_hash=""
  local actual_hash=""

  curl --fail --location --retry 3 --output "${tmpdir}/${asset}" "${base_url}/${asset}"
  curl --fail --location --retry 3 --output "${tmpdir}/${checksum}" "${base_url}/${checksum}"
  expected_hash="$(expected_sha256_for_asset "$asset" "${tmpdir}/${checksum}")" \
    || fail "checksum file does not include ${asset}"
  [[ "$expected_hash" =~ ^[A-Fa-f0-9]{64}$ ]] \
    || fail "checksum file has an invalid SHA-256 entry for ${asset}"
  expected_hash="$(printf '%s' "$expected_hash" | tr '[:upper:]' '[:lower:]')"
  actual_hash="$(sha256sum "${tmpdir}/${asset}")"
  actual_hash="${actual_hash%% *}"
  actual_hash="$(printf '%s' "$actual_hash" | tr '[:upper:]' '[:lower:]')"
  if [[ "$expected_hash" != "$actual_hash" ]]; then
    fail "checksum mismatch for ${asset}"
  fi
  echo "Checksum OK: ${asset}"
}

asset_checksum_names() {
  local asset="$1"

  printf '%s\n' "$asset"
  if [[ "$asset" == *"Duroos.Watcher_"* ]]; then
    printf '%s\n' "${asset/Duroos.Watcher_/Duroos Watcher_}"
  fi
}

expected_sha256_for_asset() {
  local asset="$1"
  local checksum_path="$2"
  local candidate=""
  local line=""
  local hash=""
  local filename=""

  while IFS= read -r candidate; do
    while IFS= read -r line || [[ -n "$line" ]]; do
      line="${line%$'\r'}"
      [[ "$line" == *"  "* ]] || continue
      hash="${line%%  *}"
      filename="${line#*  }"
      if [[ "$filename" == "$candidate" ]]; then
        printf '%s\n' "$hash"
        return 0
      fi
    done < "$checksum_path"
  done < <(asset_checksum_names "$asset")

  return 1
}

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

[[ "$(uname -s)" == "Linux" ]] || fail "this installer is for Linux only."
[[ "$(uname -m)" == "x86_64" ]] || fail "current Linux alpha assets are x86_64 only."
require_command curl
require_command sha256sum
accept_unsigned_alpha

selected_package="$(select_package)"
case "$selected_package" in
  deb)
    asset="Duroos-Watcher-${tag}-linux-unsigned-Duroos.Watcher_0.1.0_amd64.deb"
    checksum="SHA256SUMS-${tag}-linux.txt"
    ;;
  appimage)
    asset="Duroos-Watcher-${tag}-linux-unsigned-Duroos.Watcher_0.1.0_amd64.AppImage"
    checksum="SHA256SUMS-${tag}-linux.txt"
    ;;
esac

echo "Duroos Watcher ${tag} Linux unsigned alpha"
echo "Download: ${base_url}/${asset}"
echo "Package: ${selected_package}"

if [[ "${DUROOS_WATCHER_DRY_RUN:-}" == "1" ]]; then
  echo "Dry run only; no files were downloaded or installed."
  exit 0
fi

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

download_and_verify "$asset" "$checksum"

case "$selected_package" in
  deb)
    require_command sudo
    if command -v apt-get >/dev/null 2>&1; then
      sudo apt-get install -y "${tmpdir}/${asset}"
    else
      sudo dpkg -i "${tmpdir}/${asset}" \
        || fail "dpkg could not install the package. Install missing dependencies manually or use DUROOS_WATCHER_PACKAGE=appimage."
    fi
    ;;
  appimage)
    mkdir -p "$appimage_dir"
    install_path="${appimage_dir}/duroos-watcher"
    install -m 0755 "${tmpdir}/${asset}" "$install_path"
    echo "Installed ${install_path}"
    echo "Run it with: ${install_path}"
    ;;
esac
