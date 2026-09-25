#!/usr/bin/env bash
# what whisper.cpp does with the wav the piper voice writes: whether it reads 22050 Hz at all,
# what it prints, how long the model takes to load and how long a sentence takes to read back.
set -x
set +e

# the voice, the same two files the manifest declares, so the wav is the one the drive writes
repo=https://huggingface.co/csukuangfj/vits-piper-en_US-lessac-medium/resolve/main
curl -fL --retry 3 -o voice.onnx "$repo/en_US-lessac-medium.onnx"
curl -fL --retry 3 -o tokens.txt "$repo/tokens.txt"

# the sums for the manifest. large-v3-turbo is 1.6 GB, and hugging face puts the sha256 of a file
# in a header, so a HEAD asks for it without downloading the weights
curl -fL --retry 3 -o ggml-base.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
sha256sum ggml-base.bin
ls -l ggml-base.bin
curl -sIL https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin \
  | grep -iE 'x-linked-etag|content-length|x-linked-size'

sherpa=$(nix build --no-link --print-out-paths --inputs-from . nixpkgs#sherpa-onnx)
espeak=$(nix build --no-link --print-out-paths --impure --expr \
  '(builtins.getFlake (toString ./.)).inputs.nixpkgs.legacyPackages.x86_64-linux.espeak-ng.override { mbrolaSupport = false; }')
whisper=$(nix build --no-link --print-out-paths --inputs-from . nixpkgs#whisper-cpp)
ls "$whisper/bin"
"$whisper/bin/whisper-cli" --help 2>&1 | head -70
"$whisper/bin/whisper-server" --help 2>&1 | head -50

words="Rift runs the model on the drive and says this out loud."
"$sherpa/bin/sherpa-onnx-offline-tts" --vits-model=voice.onnx --vits-tokens=tokens.txt \
  --vits-data-dir="$espeak/share/espeak-ng-data" --num-threads=2 \
  --output-filename=said.wav "$words"
python3 - said.wav <<'EOF'
import sys, wave
w = wave.open(sys.argv[1])
print("said.wav channels", w.getnchannels(), "rate", w.getframerate(), "width", w.getsampwidth(),
      "seconds", round(w.getnframes() / w.getframerate(), 2))
EOF

nproc

echo "=== 1. the wav as the voice wrote it, 22050 Hz mono ==="
time "$whisper/bin/whisper-cli" -m ggml-base.bin -f said.wav -l en -t 4 -nt -np
echo "exit=$?"

echo "=== 2. again, to see it warm, and with the timings it prints ==="
time "$whisper/bin/whisper-cli" -m ggml-base.bin -f said.wav -l en -t 4 -nt
echo "exit=$?"

echo "=== 3. what stdout alone holds, with everything else thrown away ==="
"$whisper/bin/whisper-cli" -m ggml-base.bin -f said.wav -l en -t 4 -nt -np 2>/dev/null \
  | tee stdout.txt | cat -A | head -5
echo "exit=$?"

echo "=== 4. as json, in case the plain text is not enough ==="
"$whisper/bin/whisper-cli" -m ggml-base.bin -f said.wav -l en -t 4 -np -oj -of out 2>/dev/null
echo "exit=$?"
head -c 2000 out.json

echo "=== 5. a wav of 16000 Hz for comparison, made by whisper itself is not possible, so sox-free ==="
python3 - <<'EOF'
import array, wave
src = wave.open("said.wav")
samples = array.array("h")
samples.frombytes(src.readframes(src.getnframes()))
step = src.getframerate() / 16000.0
out = array.array("h")
at = 0.0
while at < len(samples) - 1:
    first = int(at)
    part = at - first
    out.append(int(samples[first] * (1 - part) + samples[first + 1] * part))
    at += step
dst = wave.open("said16.wav", "w")
dst.setnchannels(1)
dst.setsampwidth(2)
dst.setframerate(16000)
dst.writeframes(out.tobytes())
dst.close()
EOF
ls -l said.wav said16.wav
time "$whisper/bin/whisper-cli" -m ggml-base.bin -f said16.wav -l en -t 4 -nt -np
echo "exit=$?"

echo "=== 6. one thread and two, for a machine that has few ==="
time "$whisper/bin/whisper-cli" -m ggml-base.bin -f said.wav -l en -t 1 -nt -np
time "$whisper/bin/whisper-cli" -m ggml-base.bin -f said.wav -l en -t 2 -nt -np

echo "=== 7. a longer sentence, for the time against the seconds of audio ==="
long="The quick brown fox jumps over the lazy dog. Tomatoes want six hours of sun. Oil the chain every three hundred kilometres and change the brake pads when they squeal. The return is due at the end of April."
"$sherpa/bin/sherpa-onnx-offline-tts" --vits-model=voice.onnx --vits-tokens=tokens.txt \
  --vits-data-dir="$espeak/share/espeak-ng-data" --num-threads=2 \
  --output-filename=long.wav "$long"
python3 - long.wav <<'EOF'
import sys, wave
w = wave.open(sys.argv[1])
print("long.wav seconds", round(w.getnframes() / w.getframerate(), 2))
EOF
time "$whisper/bin/whisper-cli" -m ggml-base.bin -f long.wav -l en -t 4 -nt -np
echo "exit=$?"

echo "=== 8. what it does with what is not audio, and with a wav of silence ==="
head -c 2000 /dev/urandom > junk.wav
"$whisper/bin/whisper-cli" -m ggml-base.bin -f junk.wav -l en -nt -np
echo "exit=$?"
python3 - <<'EOF'
import wave
w = wave.open("silence.wav", "w")
w.setnchannels(1)
w.setsampwidth(2)
w.setframerate(22050)
w.writeframes(b"\0" * 22050 * 2 * 2)
w.close()
EOF
"$whisper/bin/whisper-cli" -m ggml-base.bin -f silence.wav -l en -nt -np
echo "exit=$?"
"$whisper/bin/whisper-cli" -m ggml-base.bin -f gone.wav -l en -nt -np
echo "exit=$?"

echo "=== 9. the server beside it: how long it takes to answer and what it answers with ==="
"$whisper/bin/whisper-server" -m ggml-base.bin -t 4 --host 127.0.0.1 --port 8642 &
server=$!
sleep 5
time curl -s -F file=@said.wav -F response_format=json -F temperature=0 \
  http://127.0.0.1:8642/inference
echo
time curl -s -F file=@said.wav -F response_format=text http://127.0.0.1:8642/inference
echo
curl -s -F file=@said.wav -F response_format=json http://127.0.0.1:8642/v1/audio/transcriptions
echo
kill $server
wait $server 2>/dev/null

echo "=== 10. does it leave files behind, and what is in the directory now ==="
ls -l

echo "=== 11. what it costs on top of the image, the way 0075 measured the voice ==="
nix path-info -r -S --json --store https://cache.nixos.org "$whisper" 2>/dev/null \
  | python3 -c 'import json,sys; d=json.load(sys.stdin); print(len(d), "paths", round(sum(p["narSize"] for p in d.values())/1048576), "MiB")'
