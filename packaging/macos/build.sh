#!/bin/bash
# Builds KakaoAdHide.app (universal: Apple silicon + Intel) and, with --dmg, dist/KakaoAdHide-macos.dmg.
# Needs Rust (rustup, with the aarch64-apple-darwin and x86_64-apple-darwin targets) and the Xcode Command
# Line Tools.
#
#   packaging/macos/build.sh                      ad-hoc signed app in target/KakaoAdHide.app (local testing)
#   packaging/macos/build.sh --dmg                ... plus dist/KakaoAdHide-macos.dmg
#   packaging/macos/build.sh --version 1.2.0      override VERSION (the release workflow checks it matches the tag)
#   packaging/macos/build.sh --sign-p12 <file> --sign-password-file <file>
#                                       sign with the release certificate (tools/new-macos-signing-cert.sh);
#                                       or set MACOS_SIGNING_P12 (base64) and MACOS_SIGNING_PASSWORD (CI)
#
# Ad-hoc builds run, but they cannot install updates signed with the release certificate (nor be installed by
# one): the updater checks the code signature. Sign with the certificate to test updates.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
version="$(tr -d '[:space:]' < "$repo/VERSION")"
make_dmg=0
p12="" ; pw_file=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dmg) make_dmg=1 ;;
    --version) version="$2"; shift ;;
    --sign-p12) p12="$2"; shift ;;
    --sign-password-file) pw_file="$2"; shift ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "version must look like 1.2.3 (got '$version')" >&2; exit 1; }

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

# ---- signing identity
identity="-"
if [[ -z "$p12" && -n "${MACOS_SIGNING_P12:-}" ]]; then
  p12="$tmp/signing.p12"; pw_file="$tmp/signing.password"
  printf '%s' "$MACOS_SIGNING_P12" | base64 -D > "$p12"
  printf '%s' "${MACOS_SIGNING_PASSWORD:?MACOS_SIGNING_PASSWORD is not set}" > "$pw_file"
fi
if [[ -n "$p12" ]]; then
  [[ -n "$pw_file" ]] || { echo "--sign-p12 needs --sign-password-file" >&2; exit 2; }
  # A throwaway keychain just for this build: nothing is added to the login keychain.
  keychain="$tmp/build.keychain-db"
  kpw="$(openssl rand -hex 16)"
  security create-keychain -p "$kpw" "$keychain"
  security set-keychain-settings -lut 3600 "$keychain"
  security unlock-keychain -p "$kpw" "$keychain"
  security import "$p12" -k "$keychain" -P "$(cat "$pw_file")" -T /usr/bin/codesign >/dev/null
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$kpw" "$keychain" >/dev/null
  # codesign looks the key up through the search list (--keychain alone fails on the CI runners), so the build
  # keychain joins it for this build; cleanup restores the original list.
  while IFS= read -r k; do k="${k#"${k%%[![:space:]]*}"}"; k="${k#\"}"; k="${k%\"}"; [[ -n "$k" ]] && search_list+=("$k"); done \
    < <(security list-keychains -d user)
  security list-keychains -d user -s "$keychain" "${search_list[@]}"
  identity="$(security find-identity -p codesigning "$keychain" | awk '/KakaoAdHide Code Signing/ {print $2; exit}')"
  [[ -n "$identity" ]] || { echo "no 'KakaoAdHide Code Signing' identity in $p12" >&2; exit 1; }
fi

sign() {
  if [[ "$identity" == "-" ]]; then
    codesign --force --sign - --options runtime "$@"
  else
    codesign --force --sign "$identity" --keychain "$keychain" --options runtime --timestamp=none "$@"
  fi
}

# ---- binary (one build per architecture, then lipo)
cd "$repo"
bins=()
for target in aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target list --installed | grep -qx "$target" || rustup target add "$target"
  KAKAOADHIDE_VERSION="$version" MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release --target "$target"
  bins+=("$repo/target/$target/release/KakaoAdHide")
done

app="$repo/target/KakaoAdHide.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/KakaoAdHide" "${bins[@]}"
sed "s/__VERSION__/$version/g" "$here/Info.plist" > "$app/Contents/Info.plist"
[[ -f "$here/AppIcon.icns" ]] && cp "$here/AppIcon.icns" "$app/Contents/Resources/"
printf 'APPL????' > "$app/Contents/PkgInfo"

sign --identifier io.github.pandemoniummm.KakaoAdHide "$app"
codesign --verify --strict --deep "$app"
echo "built $app (version $version, signed: $([[ "$identity" == "-" ]] && echo ad-hoc || echo "$identity"))"

# ---- disk image
if [[ "$make_dmg" == 1 ]]; then
  dist="$repo/dist"
  mkdir -p "$dist"
  dmg="$dist/KakaoAdHide-macos.dmg"
  stage="$tmp/stage"
  mkdir -p "$stage"
  ditto "$app" "$stage/KakaoAdHide.app"
  ln -s /Applications "$stage/Applications"
  rm -f "$dmg"
  hdiutil create -quiet -volname "KakaoAdHide $version" -srcfolder "$stage" -fs HFS+ -format ULFO -ov "$dmg"
  if [[ "$identity" == "-" ]]; then codesign --force --sign - "$dmg"
  else codesign --force --sign "$identity" --keychain "$keychain" --timestamp=none "$dmg"; fi
  echo "built $dmg"
  shasum -a 256 "$dmg"
fi
