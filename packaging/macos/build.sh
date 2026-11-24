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
# name the command that stopped the build (several steps run quietly)
trap 'echo "build.sh: line $LINENO failed (exit $?): $BASH_COMMAND" >&2' ERR

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
  TALKNAP_VERSION="$version" MACOSX_DEPLOYMENT_TARGET=11.0 cargo build --release --target "$target"
  bins+=("$repo/target/$target/release/TalkNap")
done

app="$repo/target/KakaoAdHide.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/TalkNap" "${bins[@]}"
sed "s/__VERSION__/$version/g" "$here/Info.plist" > "$app/Contents/Info.plist"
[[ -f "$here/AppIcon.icns" ]] && cp "$here/AppIcon.icns" "$app/Contents/Resources/"
printf 'APPL????' > "$app/Contents/PkgInfo"

sign --identifier io.github.pandemoniummm.KakaoAdHide "$app"
codesign --verify --strict --deep "$app"
echo "built $app (version $version, signed: $([[ "$identity" == "-" ]] && echo ad-hoc || echo "$identity"))"

# ---- disk image
# hdiutil now and then fails on GitHub's macOS runners right after the app is built (quietly, in a few seconds):
# try again a few times, and show its error when it keeps failing (its output is kept, so no -quiet).
hdiutil_retry() {
  local i out
  for i in 1 2 3 4 5; do
    if out="$(hdiutil "$@" 2>&1)"; then return 0; fi
    echo "hdiutil $1 failed (try $i of 5): $out" >&2
    sleep $((i * 3))
  done
  return 1
}

if [[ "$make_dmg" == 1 ]]; then
  dist="$repo/dist"
  mkdir -p "$dist"
  dmg="$dist/KakaoAdHide-macos.dmg"
  stage="$tmp/stage"
  mkdir -p "$stage"
  ditto "$app" "$stage/KakaoAdHide.app"
  ln -s /Applications "$stage/Applications"
  [[ -f "$here/AppIcon.icns" ]] && cp "$here/AppIcon.icns" "$stage/.VolumeIcon.icns"
  # the window's background (assets/dmg-background.svg, 1x and 2x in one TIFF)
  mkdir -p "$stage/.background"
  tiffutil -cathidpicheck "$here/dmg-background.png" "$here/dmg-background@2x.png" -out "$stage/.background/background.tiff" 2>/dev/null
  rm -f "$dmg"
  # Made writable first so the volume can be flagged to show its icon (.VolumeIcon.icns; the flag needs SetFile
  # from the command line tools) and Finder can lay out the window (saved in the volume's .DS_Store), then
  # compressed. Without Finder (no GUI session, or no permission to script it) the image still works, with a
  # plain window.
  rw="$tmp/rw.dmg"
  volname="KakaoAdHide $version"
  hdiutil_retry create -volname "$volname" -srcfolder "$stage" -fs HFS+ -format UDRW -ov "$rw"
  mnt="/Volumes/$volname"
  [[ -e "$mnt" ]] && { echo "$mnt is already mounted: eject it first" >&2; exit 1; }
  hdiutil_retry attach -noautoopen -mountpoint "$mnt" "$rw"
  if [[ -f "$stage/.VolumeIcon.icns" ]] && command -v SetFile >/dev/null; then SetFile -a C "$mnt"; fi
  if ! /usr/bin/osascript - "$volname" >/dev/null 2>&1 <<'APPLESCRIPT'
on run argv
  with timeout of 60 seconds
    tell application "Finder"
      tell disk (item 1 of argv)
        open
        set w to container window
        set current view of w to icon view
        set toolbar visible of w to false
        set statusbar visible of w to false
        set bounds of w to {200, 120, 740, 480}
        set o to icon view options of w
        set arrangement of o to not arranged
        set icon size of o to 96
        set text size of o to 13
        set background picture of o to file ".background:background.tiff"
        set position of item "KakaoAdHide.app" to {140, 165}
        set position of item "Applications" to {400, 165}
        update without registering applications
        delay 1
        close
      end tell
    end tell
  end timeout
end run
APPLESCRIPT
  then echo "Finder could not lay out the disk image window: plain window" >&2; fi
  sync
  hdiutil detach -quiet "$mnt" || hdiutil detach -quiet -force "$mnt"
  hdiutil_retry convert "$rw" -format ULFO -ov -o "$dmg"
  if [[ "$identity" == "-" ]]; then codesign --force --sign - "$dmg"
  else codesign --force --sign "$identity" --keychain "$keychain" --timestamp=none "$dmg"; fi
  echo "built $dmg"
  shasum -a 256 "$dmg"
fi
