#!/usr/bin/env bash
# Builds the REX Harness launch film end to end.
#
#   ./build.sh                 full build -> rex-harness-launch.mp4
#   OUT=/some/dir ./build.sh   keep the (large) intermediates elsewhere
#
# Needs: node 20+, Playwright's Chromium (`npx playwright install chromium`), ffmpeg,
#        python3 with numpy and scipy (`pip install numpy scipy`).
# A full render takes about 40 minutes on a 4-core machine with no GPU (the 3D scene is
# software-rendered at ~1 s per 1080p frame); the compositing and audio take a few minutes.
set -euo pipefail
cd "$(dirname "$0")"
OUT="${OUT:-out}"
mkdir -p "$OUT"

echo "1/5  3D plates (the website's real scene, frame by frame)"
node tools/render-plates.mjs --out "$OUT/plates"

echo "2/5  cue sheet"
node tools/print-cues.mjs --plates "$OUT/plates" > "$OUT/cues.json"

echo "3/5  score and sound design"
python3 tools/synth_audio.py "$OUT/cues.json" "$OUT/score.wav"

echo "4/5  composite"
rm -rf "$OUT/frames"
node tools/render-comp.mjs --plates "$OUT/plates" --out "$OUT/frames"

echo "5/5  encode"
ffmpeg -y -loglevel error -framerate 30 -i "$OUT/frames/%05d.jpg" -i "$OUT/score.wav" \
  -c:v libx264 -preset slow -crf 19 -pix_fmt yuv420p -profile:v high -tune film \
  -c:a aac -b:a 192k -af "volume=1.8dB,alimiter=limit=0.89:level=disabled" \
  -movflags +faststart -shortest rex-harness-launch.mp4
ffmpeg -y -loglevel error -ss 10.1 -i rex-harness-launch.mp4 -frames:v 1 -q:v 2 rex-harness-launch-poster.jpg
echo "done: rex-harness-launch.mp4"
