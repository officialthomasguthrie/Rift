#!/usr/bin/env bash
# downloads everything in models/manifest.toml into models/cache and prints sha256 lines
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p models/cache

# every file the manifest names, under the name it has on the drive
files=$(python3 - <<'EOF'
import tomllib

with open("models/manifest.toml", "rb") as f:
    manifest = tomllib.load(f)
for table in ("chat", "embedding", "speech", "tts"):
    for model in manifest.get(table, []):
        print(model["file"], model["url"])
        if "tokens" in model:
            print(model["tokens"], model["tokens_url"])
EOF
)

while read -r name url; do
  file="models/cache/$name"
  if [[ ! -s "$file" ]]; then
    echo ">> $url" >&2
    curl -fL --retry 3 -o "$file" "$url"
  fi
  printf '%s  %s\n' "$(sha256sum "$file" | cut -d' ' -f1)" "$name"
done <<< "$files"
