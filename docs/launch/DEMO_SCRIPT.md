# Sixty-second demo script

1. Show the 18-file checkout and run `python bug.py` three times.
2. Run `reprocut minimize --root demo/source --output minimal -- python bug.py`.
3. While search runs, explain only the three verdicts: preserved, rejected,
   inconclusive.
4. Open `minimal/report.html`; point to 18→3, strict 3/3, and the fingerprint.
5. Run `minimal/reproduce.sh` and show the same diagnostic.
6. Open `attempts.jsonl` for one preserved and one rejected candidate.
7. End on the source tree digest/clean Git state: the original was not edited.

Do not speed up footage in a way that implies benchmark performance. If the
recording is cut, label the elapsed section as edited.
