"""Frames + scene marks + narration clips -> one MP4.

usage: python3 assemble.py <name>
Reads out/<name>/rec/timeline.json and out/<name>/<scene>.wav, writes out/<name>.mp4.
"""
import json, os, subprocess, sys

name = sys.argv[1]
here = os.path.dirname(os.path.abspath(__file__))
base = f"{here}/out/{name}"
tl = json.load(open(f"{base}/rec/timeline.json"))
frames, cuts = tl["frames"], sorted(tl["cuts"])
t0 = frames[0]["t"]


def out_time(t):
    """Source time -> output time, with the cut stretches removed."""
    o = t - t0
    for a, b in cuts:
        if t > b:
            o -= b - a
        elif t > a:
            o -= t - a
    return o


def in_cut(t):
    return any(a < t < b for a, b in cuts)


keep = [f for f in frames if not in_cut(f["t"])]
end = out_time(tl["end"])
lines = ["ffconcat version 1.0"]
for i, f in enumerate(keep):
    nxt = out_time(keep[i + 1]["t"]) if i + 1 < len(keep) else end
    d = max(nxt - out_time(f["t"]), 0.001)
    lines += [f"file 'rec/{f['file']}'", f"duration {d:.4f}"]
lines.append(f"file 'rec/{keep[-1]['file']}'")
open(f"{base}/frames.txt", "w").write("\n".join(lines) + "\n")

inputs, filters, labels = [], [], []
json.dump({m["id"]: round(out_time(m["t"]) + 0.15, 3) for m in tl["marks"]} | {"end": round(end, 3)},
          open(f"{base}/scenes.json", "w"), indent=1)
for m in tl["marks"]:
    wav = f"{base}/{m['id']}.wav"
    if not os.path.exists(wav):
        continue
    ms = int(round(out_time(m["t"]) * 1000)) + 150
    k = len(inputs) // 2 + 1
    inputs += ["-i", wav]
    filters.append(f"[{k}:a]aresample=48000,adelay={ms}|{ms}[a{k}]")
    labels.append(f"[a{k}]")
    print(f"{m['id']:>12} at {ms / 1000:6.2f}s")
filters.append(f"{''.join(labels)}amix=inputs={len(labels)}:normalize=0:dropout_transition=0,"
               f"apad,atrim=0:{end:.3f},loudnorm=I=-16:TP=-1.5:LRA=11[aout]")
filters.append(f"[0:v]fps=30,format=yuv420p,fade=t=in:d=0.4,fade=t=out:st={end - 0.6:.3f}:d=0.6[vout]")

cmd = ["ffmpeg", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", f"{base}/frames.txt", *inputs,
       "-filter_complex", ";".join(filters), "-map", "[vout]", "-map", "[aout]",
       "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-movflags", "+faststart",
       "-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-t", f"{end:.3f}", f"{here}/out/{name}.mp4"]
subprocess.run(cmd, check=True)
print(f"{name}.mp4 {end:.1f}s, {len(keep)} frames, cuts {sum(b - a for a, b in cuts):.1f}s")
