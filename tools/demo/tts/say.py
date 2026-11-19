"""Speak a narration file, one clip per scene: say.py narration/<name>.json out/<name>

Writes <scene>.wav for each line and voice.json with how long each one runs,
which the recording waits for.
"""
import json, os, shutil, sys, tempfile
import soundfile as sf
import espeakng_loader
from kokoro_onnx import Kokoro
from kokoro_onnx.config import EspeakConfig

VOICE, SPEED = "af_heart", 0.98
here = os.path.dirname(os.path.abspath(__file__))

# espeak-ng keeps its data path in 160 characters and quietly falls back to
# a path from the machine it was built on when it's longer, so a deep
# virtualenv gets a short copy.
data = os.path.realpath(espeakng_loader.get_data_path())  # what phonemizer ends up using
if len(data) > 120:
    places = [tempfile.gettempdir(), os.path.expanduser("~/.cache"), "/tmp"]
    short = next(os.path.join(p, "lumiraw-espeak") for p in places if len(p) < 100)
    if not os.path.isdir(short):
        shutil.copytree(data, short)
    data = short

k = Kokoro(f"{here}/kokoro-v1.0.onnx", f"{here}/voices-v1.0.bin",
           espeak_config=EspeakConfig(lib_path=espeakng_loader.get_library_path(), data_path=data))
src, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
durations = {}
for scene, text in json.load(open(src)).items():
    samples, rate = k.create(text, voice=VOICE, speed=SPEED, lang="en-us")
    sf.write(f"{out}/{scene}.wav", samples, rate)
    durations[scene] = round(len(samples) / rate, 2)
json.dump(durations, open(f"{out}/voice.json", "w"), indent=1)
print(src, f"{sum(durations.values()):.1f}s")
