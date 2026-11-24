#!/bin/bash
# Creates the self-signed code signing certificate for the macOS app ("KakaoAdHide Code Signing",
# or "TalkNap Code Signing" with --name TalkNap).
#
# Why: the updater accepts only an app signed with this certificate (on top of the signed feed), and macOS
# remembers apps by who signed them. An ad-hoc signature changes with every build. It is free; the price is that
# the first launch needs "Open Anyway" (System Settings > Privacy & Security), like the unsigned Windows exe
# needs "Run anyway".
#
# Output (outside the repository, never commit these):
#   ~/.keys/kakaoadhide-macos-signing.p12       certificate + private key
#   ~/.keys/kakaoadhide-macos-signing.password  the .p12 password
# and prints the certificate's SHA-1, which goes into src/platform/macos.rs (SIGNING_CERT_SHA1).
#
# With --name TalkNap the certificate is "TalkNap Code Signing" and the files are
#   ~/.keys/talknap-macos-signing.p12 / .password
# and the SHA-1 goes into SIGNING_CERT_SHA1_TALKNAP.
#
# Usage:
#   tools/new-macos-signing-cert.sh               create (refuses to overwrite)
#   tools/new-macos-signing-cert.sh --set-secret  store it as the MACOS_SIGNING_P12 / MACOS_SIGNING_PASSWORD
#                                                 repository secrets (gh CLI) for the release workflow
#   tools/new-macos-signing-cert.sh --name TalkNap [--set-secret]
#                                                 same, for the TalkNap certificate; the secrets are
#                                                 MACOS_SIGNING_P12_TALKNAP / MACOS_SIGNING_PASSWORD_TALKNAP
set -euo pipefail

dir="${KEY_DIR:-$HOME/.keys}"
file_base="kakaoadhide-macos-signing"
name="KakaoAdHide Code Signing"
secret_suffix=""
const_name="SIGNING_CERT_SHA1"
set_secret=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --set-secret) set_secret=1; shift ;;
    --name)
      [[ $# -ge 2 ]] || { echo "--name needs a value (TalkNap)" >&2; exit 1; }
      if [[ "$2" == "TalkNap" ]]; then
        file_base="talknap-macos-signing"
        name="TalkNap Code Signing"
        secret_suffix="_TALKNAP"
        const_name="SIGNING_CERT_SHA1_TALKNAP"
      else
        echo "unknown --name value: $2 (only TalkNap is supported)" >&2
        exit 1
      fi
      shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 1 ;;
  esac
done

p12="$dir/$file_base.p12"
pw_file="$dir/$file_base.password"

if [[ "$set_secret" == 1 ]]; then
  [[ -f "$p12" && -f "$pw_file" ]] || { echo "no certificate in $dir: run without --set-secret first" >&2; exit 1; }
  base64 -i "$p12" | gh secret set "MACOS_SIGNING_P12$secret_suffix"
  gh secret set "MACOS_SIGNING_PASSWORD$secret_suffix" < "$pw_file"
  echo "stored MACOS_SIGNING_P12$secret_suffix and MACOS_SIGNING_PASSWORD$secret_suffix repository secrets"
  exit 0
fi

if [[ -e "$p12" ]]; then
  echo "$p12 already exists. Replacing the certificate changes the signature every installed copy expects:" >&2
  echo "users would have to turn the Accessibility permission on again and update by hand. Not overwriting." >&2
  exit 1
fi

umask 077
mkdir -p "$dir"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

openssl req -x509 -newkey rsa:3072 -sha256 -days 7300 -nodes \
  -keyout "$tmp/key.pem" -out "$tmp/cert.pem" -subj "/CN=$name" \
  -addext "basicConstraints=critical,CA:false" \
  -addext "keyUsage=critical,digitalSignature" \
  -addext "extendedKeyUsage=critical,codeSigning" 2>/dev/null

openssl rand -base64 24 | tr -d '\n' > "$pw_file"
# 3DES/SHA-1 PKCS#12: what `security import` on every supported macOS reads.
openssl pkcs12 -export -legacy -inkey "$tmp/key.pem" -in "$tmp/cert.pem" -name "$name" \
  -out "$p12" -passout "file:$pw_file" 2>/dev/null \
  || openssl pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/cert.pem" -name "$name" \
       -certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1 -out "$p12" -passout "file:$pw_file"

sha1="$(openssl x509 -in "$tmp/cert.pem" -noout -fingerprint -sha1 | sed 's/.*=//; s/://g')"
echo "wrote $p12"
echo "wrote $pw_file"
echo "certificate SHA-1: $sha1"
echo
echo "Next:"
echo "  1. Put the SHA-1 into src/platform/macos.rs ($const_name) and commit it."
echo "  2. Back up both files (password manager, encrypted USB). GitHub secrets cannot be read back."
echo "  3. For releases: tools/new-macos-signing-cert.sh${secret_suffix:+ --name TalkNap} --set-secret"
