# Ambience sound pack

The ambience pack holds the recorded sounds the soundscape plays: beds (rain, wind, places, nature),
one-shot events (birds, thunder, distant aircraft, sirens), map-object points (church bells, school
gong, station, crossings) and impulse responses for echo. `slots.toml` names every slot the game asks
for. The pack files each take under its slot and describes it in `pack.json`.

## Files

| file | what |
|---|---|
| `slots.toml` | the slot taxonomy (kind bed / event / point / ir) |
| `assign.csv` | every raw recording -> one or more slots, with take hints (`use A-B`, `avoid A-B`, `max N`; seconds) and a note |
| `sources_extra.csv` | title, author, licence and page of the Wikimedia Commons and radio aporee files |
| `titles.csv` | exact Freesound titles (the manifest holds research labels for some files) |
| `build_pack.py` | builds the pack |
| `CREDITS.md` | attribution for every source, grouped by licence (a copy of the pack's) |

## Rebuilding

```
python3 -E tools/ambience/build_pack.py <raw dir> tools/ambience/assign.csv tools/ambience/slots.toml <out dir>
```

Needs Python 3 with numpy and scipy, and ffmpeg/ffprobe with libmp3lame and flac. Options:
`--bed-seconds 45` (bed take length), `--jobs N`, `--only <regex>` (rebuild a few raw files for testing;
writes a partial pack.json, so use a scratch out dir). A full build takes about 4 minutes on 9 cores.

What the builder does:

- Decodes with ffmpeg and resamples to 32 kHz; mono stays mono, stereo stays stereo (more channels are
  folded to stereo).
- Beds and looping points: the calmest stretch of 45 s (no 0.4 s block more than 10 dB over the file's
  median, smallest level spread), honouring the hints; files longer than 4 min give two non-overlapping
  takes. 0.3 s fades. -27 LUFS integrated (BS.1770 K-weighting with gating), decoded true peak at most
  -1 dBFS; a look-ahead limiter takes at most 6 dB off single peaks, beyond that the take stays quieter.
- Events and event points: onset detection (0.4 s loudness rising more than 8 dB over the local bed),
  0.15 s pre-roll, natural tail until it falls back into the bed, 0.3 s fade, at most 8 cuts per source
  (strongest first). Lengths: birds/animals 1-15 s, thunder 4-30 s, aircraft/helicopter/train/siren 8-45 s,
  peals/Angelus/change ringing kept whole up to 240 s, church strike one stroke of 1.5-8 s, school gong
  and bell whole, door/forklift 1-6 s, steady machines (lawnmower, leaf blower, tractor) the strongest
  30 s stretches, others 2-30 s. Without a clean onset the strongest whole stretch is kept. Peak -3 dBFS;
  `st_max_lufs` in pack.json is the cut's loudest 3 s window so the game can level events.
- Impulse responses: leading silence trimmed, -1 dBFS peak, FLAC 32 kHz.
- MP3 VBR `-q:a 5` at 32 kHz. Every take is decoded again to measure the values written to pack.json.

The 60 s takes the slot file suggests made a ~356 MB pack; 45 s takes keep it under 300 MB with every
recording used.

## Output

`<out>/<slot with dots as folders>/<source>_<n>.mp3` (IRs `.flac`), plus `pack.json`:

```
{"version":1,"sample_rate":32000,"takes":[{"slot":"rain.light","file":"rain/light/262029_1.mp3",
 "kind":"bed","seconds":45.0,"channels":2,"lufs":-27.0,"peak_db":-1.1,"source":"freesound:262029",
 "title":"...","author":"...","license":"https://creativecommons.org/licenses/by/4.0/",
 "page":"https://freesound.org/s/262029/","cut":[5.0,50.0]}]}
```

Events add `st_max_lufs` and `cut_how` (`onset`, `strike`, `whole` or `stretch`). `source` is
`freesound:<id>`, `commons:<file name>` or `aporee:<id>`. `cut` is the stretch of the raw file in seconds.

## Where the raw files come from

`ambience-sounds-raw/` (not in the repository): Freesound previews (128 kbps MP3) listed in its
`manifest.csv` with research heading, title, author, licence and page; original OGG/WAV/FLAC files from
Wikimedia Commons (`commons/`) and radio aporee (`aporee/`). The research notes behind every choice
(content, what to cut, licence checks) are in `reports/ambience-sounds/` (`weather.md`, `nature.md`,
`urban.md`, `objects.md`, `audition.md`).

## Licence policy

Only CC0, CC BY (2.0/3.0/4.0), public domain (Commons PD files, radio aporee Public Domain Mark) and, where
nothing else exists, CC BY-SA. No NC, ND or "no redistribution" licences. Every source is listed once in
`CREDITS.md` with title, author, link and licence and the note that it was trimmed, level-normalised and
re-encoded, as CC BY requires. CC BY-SA takes (currently one: the Commons "Bue Laeutewerk zweifach"
crossing bell) are adaptations shared under the same licence; keep them marked if the pack is split.

## Countries

`regions.csv` marks the takes that belong to one country (`de`, `uk`): a German platform's
announcements, the Berlin S-Bahn's door signal, a British guard's whistle. `tag_regions.py` writes the
mark into `pack.json` (`build_pack.py` runs it at the end). The game plays a country's takes only on its
maps - British on left-hand-traffic maps, German on the others - and unmarked takes everywhere.

## Before the pack is published

Four recordings by the Freesound user Kinoton are in the pack (`freesound:478455`, `611708`: swifts;
`397113`: bumblebee; `416038`: ducks and geese at a lake). They are labelled CC0, but the uploader's
profile forbids making the raw files available to others, which a published pack does. Ask the
author for written permission, or rebuild without them, before the pack goes public.
