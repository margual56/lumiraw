# Demo videos

The four narrated recordings on `/demo` (`web/static/videos/`): developing a
photo, culling a shoot, merging a bracket and the sky planner. A script drives
the real app in a headless Chromium, the narration is spoken by
[Kokoro](https://github.com/thewh1teagle/kokoro-onnx) on this machine, and
ffmpeg puts the two together. Re-record them when the app changes enough to
show.

## Setup, once

```
cd tools/demo
npm install
npx playwright-core install chromium

python3 -m venv tts/venv
tts/venv/bin/pip install kokoro-onnx soundfile
curl -LO --output-dir tts https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx
curl -LO --output-dir tts https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin
```

Needs ffmpeg. The raw files are our own and aren't in the repository; `raws.mjs`
says where they are (`DEMO_RAWS`, by default `~/Pictures/lumiraw-demo`).

## Recording

```
npm run build && npm run preview          # from the root: the site on :8790
cd tools/demo
./record.sh                               # all four, or: ./record.sh sky
```

Each video goes through three steps:

1. `tts/say.py` speaks `narration/<name>.json`, one clip per scene, and notes
   how long each one runs.
2. `<name>.mjs` plays the scenes in the app. It waits for each clip to finish
   before going on, and drops the waits for a render from the video.
3. `assemble.py` lays the clips over the frames at each scene's start and
   writes `out/<name>.mp4` (with a title and an end card, to post on its own).
   `web.py` then cuts the cards off, adds captions from the narration, and
   writes the smaller copy, poster and `.vtt` into `web/static/videos/`.

The voice is `af_heart`. "LumiRaw" is written that way in the narration so it
is said "loomi raw", not "lumirraw".
