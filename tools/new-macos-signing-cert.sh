#!/bin/bash
# Creates the self-signed code signing certificate for the macOS app ("KakaoAdHide Code Signing").
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
# Usage:
#   tools/new-macos-signing-cert.sh               create (refuses to overwrite)
#   tools/new-macos-signing-cert.sh --set-secret  store it as the MACOS_SIGNING_P12 / MACOS_SIGNING_PASSWORD
#                                                 repository secrets (gh CLI) for the release workflow
set -euo pipefail

dir="${KEY_DIR:-$HOME/.keys}"
p12="$dir/kakaoadhide-macos-signing.p12"
pw_file="$dir/kakaoadhide-macos-signing.password"
name="KakaoAdHide Code Signing"

if [[ "${1:-}" == "--set-secret" ]]; then
  [[ -f "$p12" && -f "$pw_file" ]] || { echo "no certificate in $dir: run without --set-secret first" >&2; exit 1; }
  base64 -i "$p12" | gh secret set MACOS_SIGNING_P12
  gh secret set MACOS_SIGNING_PASSWORD < "$pw_file"
  echo "stored MACOS_SIGNING_P12 and MACOS_SIGNING_PASSWORD repository secrets"
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
echo "  1. Put the SHA-1 into src/platform/macos.rs (SIGNING_CERT_SHA1) and commit it."
echo "  2. Back up both files (password manager, encrypted USB). GitHub secrets cannot be read back."
echo "  3. For releases: tools/new-macos-signing-cert.sh --set-secret"
