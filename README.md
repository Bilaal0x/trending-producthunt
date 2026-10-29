# Product Hunt Today

The day's top 5 Product Hunt launches as a 37.5 s, 1080x1080, 60 fps video made with
[fframes](https://github.com/dmtrKovalenko/fframes): an fframes take on
[eason-dev/product-hunt-today](https://github.com/eason-dev/product-hunt-today).

The whole video is one ranked table on white. Each launch, from #5 to #1, opens out of its own
row into a full card (logo, upvote count rolling up, name, tagline, up to three gallery images),
then folds back into the table, which keeps its vote count. After #1 the rows fill with their
share of the votes and the table becomes the leaderboard.

Design: one typeface (Schibsted Grotesk) in three sizes, a white ground, near-black ink, stone
tone steps instead of lines, square media, and Product Hunt's orange only on the upvote marks.
The winner's row is the one inverted field. The soundtrack is original and synthesized
(`scripts/make_music.py`), 160 BPM half-time; each card is fully open on a downbeat.

## Use it

```sh
python scripts/fetch.py                            # yesterday's top 5 (Pacific time) -> data/
python scripts/fetch.py --today                    # or: trending today so far
python scripts/fetch.py --date 2026-10-27          # or: a specific day
cargo run --release -- render -o product-hunt-today.mp4
```

The video reads `data/` at runtime, so a new day only needs a new fetch, not a rebuild.
Use `--data other-folder` to point it elsewhere.

Credentials go in `.env` (git-ignored). Use either a developer token or the app's key and
secret from <https://www.producthunt.com/v2/oauth/applications>:

```sh
PRODUCT_HUNT_TOKEN=...
# or
PRODUCT_HUNT_API_KEY=...
PRODUCT_HUNT_API_SECRET=...
```

`fetch.py` needs only the Python standard library. It downloads logos as 320 px PNGs and
gallery images at 1600x960, padded to the 5:3 frame by imgix with a blurred fill so nothing
gets cropped.

## Files

| path | what |
| --- | --- |
| `src/lib.rs` | the video: scenes, layout, motion, audio placement |
| `src/main.rs` | the command line (`fframes::cli`), loads `data/` and `audio/` |
| `scripts/fetch.py` | Product Hunt GraphQL API → `data/products.json` + `data/media/` |
| `scripts/make_music.py` | regenerates `audio/` (needs numpy) |
| `media/` | fonts embedded in the binary (Schibsted Grotesk, OFL) |
| `audio/` | music and effects, loaded at runtime to keep them stereo |

## Work on it

```sh
cargo run --release -- preview 0s           # real-time window with sound (space, h/l, j/k, q)
cargo run --release -- timeline             # parts (Intro, Rank5..Rank1, Leaderboard), audio
cargo run --release -- strip Rank3 -n 12    # contact sheet -> strip.png
cargo run --release -- frame 5.2s           # full-size PNG -> frames/
cargo run --release -- inspect              # missing media/fonts, clipped text, panics
cargo run --release -- audio analyze        # loudness (about -14 LUFS)
```
