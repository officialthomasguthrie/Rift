#!/usr/bin/env bash
# run pdftotext in the sandbox airlock text builds, and say what came out
set -u
name=$1
shift
pdftotext=$(readlink -f "$(nix shell --inputs-from . nixpkgs#poppler-utils -c sh -c 'command -v pdftotext')")
bwrap=$(nix shell --inputs-from . nixpkgs#bubblewrap -c sh -c 'command -v bwrap')
file=$PWD/letter.pdf
set +e
"$bwrap" --unshare-all --unshare-user --disable-userns --die-with-parent --new-session \
  --ro-bind /nix/store /nix/store "$@" --proc /proc --dev /dev --tmpfs /tmp --tmpfs /home \
  --ro-bind "$file" "$file" --chdir /tmp \
  -- "$pdftotext" -q -enc UTF-8 -eol unix -l 200 "$file" - > "$name.txt" 2> "$name.err"
status=$?
echo "--- $name: pdftotext exited with $status"
od -c "$name.txt" | tail -8
echo "--- $name stderr"
cat "$name.err"
if grep -q "insurance card" "$name.txt"; then
  echo "--- $name: the text came out"
else
  echo "--- $name: NO TEXT"
fi
