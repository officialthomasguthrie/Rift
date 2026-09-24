#!/usr/bin/env bash
# probe round two: the piper lessac voice as sherpa-onnx reads it, with the espeak data the image
# already has. round one showed the stock rhasspy onnx carries no sample_rate in its metadata
set -x
set +e

repo=https://huggingface.co/csukuangfj/vits-piper-en_US-lessac-medium/resolve/main
curl -fL --retry 3 -o voice.onnx "$repo/en_US-lessac-medium.onnx"
curl -fL --retry 3 -o tokens.txt "$repo/tokens.txt"
ls -l voice.onnx tokens.txt
sha256sum voice.onnx tokens.txt
wc -l tokens.txt
head -4 tokens.txt | cat -A

sherpa=$(nix build --no-link --print-out-paths --inputs-from . nixpkgs#sherpa-onnx)
voice=$(nix build --no-link --print-out-paths --impure --expr \
  '(builtins.getFlake (toString ./.)).inputs.nixpkgs.legacyPackages.x86_64-linux.espeak-ng.override { mbrolaSupport = false; }')
ls "$sherpa/bin"

say() {
  time "$sherpa/bin/sherpa-onnx-offline-tts" \
    --vits-model=voice.onnx --vits-tokens=tokens.txt --num-threads=2 \
    "$@"
  echo "exit=$?"
}

look() {
  python3 - "$1" <<'EOF'
import sys, wave
w = wave.open(sys.argv[1])
raw = w.readframes(w.getnframes())
peak = max(abs(int.from_bytes(raw[i:i+2], "little", signed=True)) for i in range(0, len(raw), 2))
print(sys.argv[1], "channels", w.getnchannels(), "rate", w.getframerate(), "width", w.getsampwidth(),
      "frames", w.getnframes(), "seconds", round(w.getnframes() / w.getframerate(), 2), "peak", peak)
EOF
  head -c 16 "$1" | xxd
}

echo "=== with the espeak data the image has ==="
say --vits-data-dir="$voice/share/espeak-ng-data" --output-filename=image-data.wav \
  "Rift says this sentence out loud, with the voice that came on the drive."
look image-data.wav

echo "=== with no data dir at all ==="
say --output-filename=nodata.wav "Rift says this sentence out loud."
look nodata.wav

echo "=== a long one, for the time it takes ==="
long=$(python3 -c 'print("The quick brown fox jumps over the lazy dog. " * 20)')
say --vits-data-dir="$voice/share/espeak-ng-data" --output-filename=long.wav "$long"
look long.wav
echo "characters: ${#long}"

echo "=== an empty text, and one with a quote and a newline in it ==="
say --vits-data-dir="$voice/share/espeak-ng-data" --output-filename=empty.wav ""
ls -l empty.wav
say --vits-data-dir="$voice/share/espeak-ng-data" --output-filename=odd.wav \
  "It said \"go left\", then it said 'go right'.
And then nothing."
look odd.wav

echo "=== the whole help, and the stdout of one run on its own ==="
"$sherpa/bin/sherpa-onnx-offline-tts" --help 2>&1 | sed -n '/^Options:/,$p' | head -60
