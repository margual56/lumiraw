"""out/<name>.mp4 -> web/static/videos/<name>.{mp4,jpg,vtt}: no title or end card, captions, smaller.

usage: python3 web.py <name>
"""
import json, os, re, subprocess, sys

name = sys.argv[1]
here = os.path.dirname(os.path.abspath(__file__))
scenes = json.load(open(f"{here}/out/{name}/scenes.json"))
voice = json.load(open(f"{here}/out/{name}/voice.json"))
lines = json.load(open(f"{here}/narration/{name}.json"))
dest = os.path.normpath(f"{here}/../../web/static/videos")
os.makedirs(dest, exist_ok=True)

start = scenes["intro"] - 0.6           # just after the title card
stop = scenes["outro"] - 0.15           # just before the end card
length = stop - start


def chunks(text):
    """Sentences, and long ones split at a comma, to fit two lines."""
    out = []
    for sentence in re.split(r"(?<=[.?!:])\s+", text):
        while len(sentence) > 84 and "," in sentence[20:84]:
            cut = sentence.rindex(",", 20, 84) + 1
            out.append(sentence[:cut]); sentence = sentence[cut:].strip()
        out.append(sentence)
    return [c.replace("lumiraw dot app", "lumiraw.app") for c in out if c]


def stamp(t):
    t = max(t, 0)
    return f"{int(t // 3600):02}:{int(t % 3600 // 60):02}:{t % 60:06.3f}"


cues = ["WEBVTT", ""]
for key, text in lines.items():
    if key not in scenes or key == "outro":
        continue
    at, dur = scenes[key] - start, voice[key]
    parts = chunks(text)
    total = sum(len(p) for p in parts)
    for p in parts:
        d = dur * len(p) / total
        cues += [f"{stamp(at)} --> {stamp(min(at + d, length))}", p, ""]
        at += d
open(f"{dest}/{name}.vtt", "w").write("\n".join(cues))

src = f"{here}/out/{name}.mp4"
subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-ss", f"{start:.3f}", "-i", src, "-t", f"{length:.3f}",
                "-vf", "fade=t=in:d=0.3", "-af", "afade=t=in:d=0.2",
                "-c:v", "libx264", "-preset", "veryslow", "-crf", "27", "-pix_fmt", "yuv420p", "-movflags", "+faststart",
                "-c:a", "aac", "-b:a", "96k", "-ac", "1", f"{dest}/{name}.mp4"], check=True)
at, pad = {"develop": ("compare", 4), "cull": ("results", 1.5), "merge": ("result", 3), "sky": ("golden", 4)}[name]
poster = scenes[at] - start + pad
subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-ss", f"{poster:.2f}", "-i", f"{dest}/{name}.mp4",
                "-frames:v", "1", "-vf", "scale=1280:-1", "-q:v", "4", f"{dest}/{name}.jpg"], check=True)
size = os.path.getsize(f"{dest}/{name}.mp4") / 1e6
print(f"web/static/videos/{name}.mp4 {length:.1f}s {size:.1f} MB")
