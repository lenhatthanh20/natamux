# Site capture kit

Reshoots the media of the website (stills, clips, film) after a release, from the native app.

## What it is

- `native-record.sh` (with `native-record.swift`, `native-record.py`): records the native app on a Mac display. Run `native-record.sh preflight` first.
- `native-agent.py`, `native-demo.py`: scripted demo agents and demo data for the recording.
- `native-record.provenance.py`: writes the proof file of a capture. `native-record.cuts.example.json`: example cuts.
- `ffm.sh <in> <out> [ffmpeg options]`: one ffmpeg call in Docker (`linuxserver/ffmpeg`, no install), for cropping and encoding.
- Guides: `native-capture.md` (the full capture steps and the completed set) and `native-demo.md` (the demo setup).
- Honesty note: keep this sentence next to every recording on the site and in any PR:
  "The app and the stages are real. Only the agents' terminal text comes from a scripted demo program."

## Where files go

- Clips, film, posters: `site/public/media/native/`.
- Article images: `site/public/screens/native/`.
- Playback index: `site/src/data/native-recordings.json`. Proof: `site/public/media/native/capture-provenance.json`.
- Run everything from this folder: `cd scripts/site-capture`.
