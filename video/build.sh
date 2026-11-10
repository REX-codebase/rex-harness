#!/usr/bin/env bash
# Builds the REX Harness launch film end to end.
#
#   ./build.sh                 full build -> rex-harness-launch.mp4
#   OUT=/some/dir ./build.sh   keep the (large) intermediates elsewhere
#
# Needs: node 20+, Playwright's Chromium (`npx playwright install chromium`), ffmpeg,
#        python3 with numpy, scipy and sherpa-onnx (`pip install numpy scipy sherpa-onnx`).
# The narrator is Kokoro-82M (Apache-2.0), run locally through sherpa-onnx; the 350 MB model is
# downloaded once into $OUT/voice.
#
# A full render takes about an hour on a 4-core machine with no GPU (the 3D scene is software-rendered
# at roughly 1 s per frame); compositing, narration and audio take a few minutes more.
set -euo pipefail
cd "$(dirname "$0")"
OUT="${OUT:-out}"
mkdir -p "$OUT"
VOICE_DIR="${KOKORO_DIR:-$OUT/voice/kokoro-multi-lang-v1_0}"

echo "1/8  the narrator's voice model"
if [ ! -f "$VOICE_DIR/model.onnx" ]; then
  mkdir -p "$OUT/voice"
  curl -fSL -o "$OUT/voice/kokoro.tar.bz2" "https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-multi-lang-v1_0.tar.bz2"
  tar xjf "$OUT/voice/kokoro.tar.bz2" -C "$OUT/voice"
fi

echo "2/8  3D plates: the film's own shots (the website's real scene, frame by frame)"
node tools/render-plates.mjs --out "$OUT/plates"

echo "3/8  3D plates: the 16:9 footage the film re-cuts"
node tools/render-plates.mjs --legacy --out "$OUT/legacy"

echo "4/8  cue sheet"
node tools/print-cues.mjs --plates "$OUT/legacy" > "$OUT/cues.json"

echo "5/8  narration"
mkdir -p "$OUT/vo"
python3 -I tools/narrate.py "$OUT/cues.json" "$VOICE_DIR" "$OUT/vo"
cp "$OUT/vo/vo.json" "$OUT/vo.json"

echo "6/8  score and sound design"
python3 tools/synth_audio.py "$OUT/cues.json" "$OUT/vo" "$OUT/score.wav"

echo "7/8  composite"
rm -rf "$OUT/frames"
node tools/render-comp.mjs --plates "$OUT/plates" --legacy "$OUT/legacy" --meta "$OUT" --out "$OUT/frames"

echo "8/8  encode"
ffmpeg -y -loglevel error -framerate 24 -i "$OUT/frames/%05d.jpg" -i "$OUT/score.wav" \
  -c:v libx264 -preset slow -crf 18 -pix_fmt yuv420p -profile:v high -tune film \
  -c:a aac -b:a 192k -af "volume=2dB,alimiter=limit=0.84:level=disabled" \
  -movflags +faststart -shortest rex-harness-launch.mp4
ffmpeg -y -loglevel error -ss 73.6 -i rex-harness-launch.mp4 -frames:v 1 -q:v 2 rex-harness-launch-poster.jpg
echo "done: rex-harness-launch.mp4"
