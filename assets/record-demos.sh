#!/usr/bin/env bash
# Regenerate the README demo GIFs.
#
# This machine had no vhs and no agg (agg won't build here: its rustls/aws-lc-sys
# dependency hits a gcc bug). So the pipeline is:
#   asciinema rec  ->  render_cast.py (pyte + PIL) -> PNG frames  ->  ffmpeg -> GIF
#
# Requirements: a release build of ttysaver, tty-clock, cmatrix,
#   asciinema, ffmpeg, and python3 with pyte + pillow.
#     pip install asciinema pyte pillow
#
# Run from anywhere:  bash assets/record-demos.sh
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
BIN="$REPO/target/release/ttysaver"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

[ -x "$BIN" ] || { echo "build first: cargo build --release" >&2; exit 1; }

rec () { # seconds cols rows outfile cmd...
  local secs=$1 cols=$2 rows=$3 out=$4; shift 4
  # ttysaver waits for a keypress; SIGINT after N seconds ends the recording.
  timeout --signal=INT "$secs" \
    asciinema rec --overwrite --cols "$cols" --rows "$rows" -c "$*" "$out" \
    >/dev/null 2>&1 || true
}

gif () { # framedir fps outfile
  ffmpeg -y -loglevel error -framerate "$2" -i "$1/frame_%05d.png" \
    -vf "palettegen=stats_mode=diff" "$WORK/pal.png"
  ffmpeg -y -loglevel error -framerate "$2" -i "$1/frame_%05d.png" -i "$WORK/pal.png" \
    -lavfi "paletteuse=dither=bayer:bayer_scale=3" -loop 0 "$3"
}

# Demo 1: fullscreen cmatrix. Keep --fps modest so asciinema keeps up with the
# output; at very high fps the pty write blocks and the capture truncates early.
rec 7 90 28 "$WORK/cmatrix.cast" "$BIN --fps 24 cmatrix -u 5"
python3 "$HERE/render_cast.py" "$WORK/cmatrix.cast" "$WORK/f1" --fps 12 --start 0.5 --end 5.0 --cell-h 14
gif "$WORK/f1" 12 "$HERE/cmatrix.gif"

# Demo 2: giant clock (zoom 2), bounced DVD-logo style. No seconds/date so the
# clock stays narrow enough to actually move around.
rec 8 110 32 "$WORK/clock.cast" "$BIN --zoom 2 --bounce --speed 16 tty-clock -D -C 6"
python3 "$HERE/render_cast.py" "$WORK/clock.cast" "$WORK/f2" --fps 12 --start 0.6 --end 7.4 --cell-h 12
gif "$WORK/f2" 12 "$HERE/bounce-clock.gif"

echo "wrote $HERE/cmatrix.gif and $HERE/bounce-clock.gif"
