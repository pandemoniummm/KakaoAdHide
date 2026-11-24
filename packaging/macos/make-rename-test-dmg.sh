#!/bin/bash
# Mac-only manual test helper: makes a disk image holding TalkNap.app, to check that 2.6.0 accepts the renaming
# update and swaps /Applications/KakaoAdHide.app for TalkNap.app.
#
#   packaging/macos/make-rename-test-dmg.sh <built KakaoAdHide.app> <version> \
#       --sign-p12 <TalkNap p12> --sign-password-file <file> <out.dmg>
#
# Copies the app as TalkNap.app, sets CFBundleIdentifier to io.github.pandemoniummm.TalkNap and
# CFBundleShortVersionString to <version>, signs it with the "TalkNap Code Signing" identity from the p12
# (tools/new-macos-signing-cert.sh --name TalkNap) in a throwaway keychain, and puts it in <out.dmg>.
# The password is only read from the file, never printed.
set -euo pipefail
trap 'echo "make-rename-test-dmg.sh: line $LINENO failed (exit $?): $BASH_COMMAND" >&2' ERR

usage() { echo "usage: $0 <KakaoAdHide.app> <version> --sign-p12 <file> --sign-password-file <file> <out.dmg>" >&2; exit 2; }

src_app="" ; version="" ; p12="" ; pw_file="" ; out=""
positional=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --sign-p12) p12="${2:-}"; shift ;;
    --sign-password-file) pw_file="${2:-}"; shift ;;
    -*) echo "unknown option $1" >&2; usage ;;
    *) positional+=("$1") ;;
  esac
  shift
done
[[ ${#positional[@]} -eq 3 ]] || usage
src_app="${positional[0]}"; version="${positional[1]}"; out="${positional[2]}"
[[ -n "$p12" && -n "$pw_file" ]] || usage
[[ -d "$src_app" ]] || { echo "no app at $src_app" >&2; exit 1; }
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version must look like 1.2.3 (got '$version')" >&2; exit 1; }

bundle_id="io.github.pandemoniummm.TalkNap"
tmp="$(mktemp -d)"
keychain=""
search_list=()
cleanup() {
  # put the user's keychain search list back as it was
  [[ ${#search_list[@]} -gt 0 ]] && security list-keychains -d user -s "${search_list[@]}" 2>/dev/null || true
  [[ -n "$keychain" ]] && security delete-keychain "$keychain" 2>/dev/null || true
  rm -rf "$tmp"
}
trap cleanup EXIT

# ---- signing identity, in a throwaway keychain (same steps as build.sh)
keychain="$tmp/rename.keychain-db"
kpw="$(openssl rand -hex 16)"
security create-keychain -p "$kpw" "$keychain"
security set-keychain-settings -lut 3600 "$keychain"
security unlock-keychain -p "$kpw" "$keychain"
security import "$p12" -k "$keychain" -P "$(cat "$pw_file")" -T /usr/bin/codesign >/dev/null
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$kpw" "$keychain" >/dev/null
while IFS= read -r k; do k="${k#"${k%%[![:space:]]*}"}"; k="${k#\"}"; k="${k%\"}"; [[ -n "$k" ]] && search_list+=("$k"); done \
  < <(security list-keychains -d user)
security list-keychains -d user -s "$keychain" "${search_list[@]}"
identity="$(security find-identity -p codesigning "$keychain" | awk '/TalkNap Code Signing/ {print $2; exit}')"
[[ -n "$identity" ]] || { echo "no 'TalkNap Code Signing' identity in $p12" >&2; exit 1; }

# ---- TalkNap.app
stage="$tmp/stage"
mkdir -p "$stage"
app="$stage/TalkNap.app"
ditto "$src_app" "$app"
plutil -replace CFBundleIdentifier -string "$bundle_id" "$app/Contents/Info.plist"
plutil -replace CFBundleShortVersionString -string "$version" "$app/Contents/Info.plist"
codesign --force --deep --sign "$identity" --keychain "$keychain" --identifier "$bundle_id" \
  --options runtime --timestamp=none "$app"
codesign --verify --strict --deep \
  -R="identifier \"$bundle_id\" and certificate leaf = H\"$(security find-certificate -c "TalkNap Code Signing" -Z "$keychain" | awk '/SHA-1/ {print $3; exit}')\"" "$app"

# ---- disk image
rm -f "$out"
hdiutil create -quiet -volname "TalkNap $version" -srcfolder "$stage" -fs HFS+ -format ULFO -ov "$out"
echo "built $out (TalkNap.app $version, signed: $identity)"
