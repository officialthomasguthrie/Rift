#!/usr/bin/env bash
# probe: can sherpa-onnx say a sentence with the piper voice the manifest already declares,
# using the espeak-ng data the image already has, and what does the wav look like
set -x
set +e

onnx=https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx
json=$onnx.json

curl -fL --retry 3 -o voice.onnx "$onnx"
curl -fL --retry 3 -o voice.onnx.json "$json"
ls -l voice.onnx voice.onnx.json
sha256sum voice.onnx voice.onnx.json

# what piper's own config says about the voice
python3 - <<'EOF'
import json
c = json.load(open("voice.onnx.json"))
print("sample_rate", c["audio"]["sample_rate"])
print("keys", sorted(c.keys()))
print("inference", c.get("inference"))
print("phoneme_type", c.get("phoneme_type"), "espeak", c.get("espeak"))
print("num_symbols", c.get("num_symbols"), "num_speakers", c.get("num_speakers"))
ids = c["phoneme_id_map"]
print("phoneme_id_map size", len(ids))
print("first ten", list(ids.items())[:10])
EOF

# sherpa-onnx wants a tokens file, one "<phoneme> <id>" a line, which is piper's own map flattened
python3 - <<'EOF'
import json
c = json.load(open("voice.onnx.json"))
with open("tokens.txt", "w", encoding="utf-8") as f:
    for symbol, ids in c["phoneme_id_map"].items():
        f.write(f"{symbol} {ids[0]}\n")
EOF
head -5 tokens.txt | cat -A | head -5
wc -l tokens.txt

echo "=== what sherpa-onnx ships ==="
sherpa=$(nix build --no-link --print-out-paths --inputs-from . nixpkgs#sherpa-onnx)
echo "$sherpa"
ls "$sherpa/bin"
du -sh "$sherpa"

echo "=== the espeak the image has, without mbrola ==="
voice=$(nix build --no-link --print-out-paths --impure --expr \
  '(builtins.getFlake (toString ./.)).inputs.nixpkgs.legacyPackages.x86_64-linux.espeak-ng.override { mbrolaSupport = false; }')
echo "$voice"
ls "$voice/share/espeak-ng-data" | head -10
du -sh "$voice/share/espeak-ng-data"

echo "=== help ==="
"$sherpa/bin/sherpa-onnx-offline-tts" --help 2>&1 | head -80

echo "=== say a sentence ==="
time "$sherpa/bin/sherpa-onnx-offline-tts" \
  --vits-model=voice.onnx \
  --vits-tokens=tokens.txt \
  --vits-data-dir="$voice/share/espeak-ng-data" \
  --num-threads=2 \
  --output-filename=out.wav \
  "Rift says this sentence out loud, with the voice that came on the drive."
echo "tts exit=$?"
ls -l out.wav
python3 - <<'EOF'
import wave
w = wave.open("out.wav")
print("channels", w.getnchannels(), "rate", w.getframerate(), "width", w.getsampwidth(),
      "frames", w.getnframes(), "seconds", round(w.getnframes() / w.getframerate(), 2))
raw = w.readframes(w.getnframes())
peak = max(abs(int.from_bytes(raw[i:i+2], "little", signed=True)) for i in range(0, len(raw), 2))
print("peak", peak)
EOF

echo "=== the same voice with no data dir, to see whether it needs one ==="
"$sherpa/bin/sherpa-onnx-offline-tts" --vits-model=voice.onnx --vits-tokens=tokens.txt \
  --output-filename=nodata.wav "A short test." 2>&1 | tail -20
echo "no data dir exit=$?"

echo "=== whisper-cpp, for part two ==="
whisper=$(nix build --no-link --print-out-paths --inputs-from . nixpkgs#whisper-cpp)
ls "$whisper/bin"
