#!/usr/bin/env python3
"""Render an asciicast v2 file to PNG frames using pyte + PIL.

Usage: render_cast.py CAST_FILE OUT_DIR --fps N [--start S] [--end E] [--cell-h PX]
Frames are written as frame_00001.png ... in OUT_DIR. Assemble with ffmpeg.
"""
import argparse, json, os, sys
from PIL import Image, ImageDraw, ImageFont

NAMED = {
    "black": (0, 0, 0), "red": (205, 0, 0), "green": (0, 205, 0),
    "brown": (205, 205, 0), "yellow": (205, 205, 0), "blue": (0, 0, 238),
    "magenta": (205, 0, 205), "cyan": (0, 205, 205), "white": (229, 229, 229),
}
BRIGHT = {
    "black": (85, 85, 85), "red": (255, 85, 85), "green": (85, 255, 85),
    "brown": (255, 255, 85), "yellow": (255, 255, 85), "blue": (85, 85, 255),
    "magenta": (255, 85, 255), "cyan": (85, 255, 255), "white": (255, 255, 255),
}
DEFAULT_FG = (200, 200, 200)
DEFAULT_BG = (0, 0, 0)


def to_rgb(color, bold, is_fg):
    if color == "default":
        return (DEFAULT_FG if is_fg else DEFAULT_BG)
    if color in NAMED:
        return (BRIGHT if bold else NAMED)[color]
    if isinstance(color, str) and len(color) == 6:
        try:
            return (int(color[0:2], 16), int(color[2:4], 16), int(color[4:6], 16))
        except ValueError:
            pass
    return (DEFAULT_FG if is_fg else DEFAULT_BG)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cast")
    ap.add_argument("outdir")
    ap.add_argument("--fps", type=float, default=12)
    ap.add_argument("--start", type=float, default=0.0)
    ap.add_argument("--end", type=float, default=1e9)
    ap.add_argument("--cell-h", type=int, default=16)
    args = ap.parse_args()

    import pyte

    with open(args.cast, "r", encoding="utf-8", errors="replace") as f:
        header = json.loads(f.readline())
        cols = header.get("width", 80)
        rows = header.get("height", 24)
        events = []
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                t, typ, data = json.loads(line)
            except Exception:
                continue
            if typ == "o":
                events.append((t, data))

    if not events:
        print("no output events", file=sys.stderr)
        sys.exit(2)
    last = min(events[-1][0], args.end)

    screen = pyte.Screen(cols, rows)
    stream = pyte.ByteStream(screen)

    # Font metrics
    font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", args.cell_h)
    bbox = font.getbbox("M")
    cw = font.getlength("M")
    cell_w = int(round(cw))
    cell_h = int(round(args.cell_h * 1.25))
    img_w = cell_w * cols
    img_h = cell_h * rows
    ascent, descent = font.getmetrics()
    y_off = (cell_h - (ascent + descent)) // 2

    os.makedirs(args.outdir, exist_ok=True)

    dt = 1.0 / args.fps
    frame_times = []
    t = args.start
    while t <= last:
        frame_times.append(t)
        t += dt

    ei = 0
    frame_no = 0
    for ft in frame_times:
        while ei < len(events) and events[ei][0] <= ft:
            stream.feed(events[ei][1].encode("utf-8", "replace"))
            ei += 1
        if ft < args.start:
            continue
        img = Image.new("RGB", (img_w, img_h), DEFAULT_BG)
        draw = ImageDraw.Draw(img)
        buf = screen.buffer
        for y in range(rows):
            row = buf[y]
            for x in range(cols):
                ch = row[x]
                data = ch.data
                fg = to_rgb(ch.fg, ch.bold, True)
                bg = to_rgb(ch.bg, False, False)
                if ch.reverse:
                    fg, bg = bg, fg
                px = x * cell_w
                py = y * cell_h
                if bg != DEFAULT_BG:
                    draw.rectangle([px, py, px + cell_w, py + cell_h], fill=bg)
                if data and data != " ":
                    draw.text((px, py + y_off), data, font=font, fill=fg)
        frame_no += 1
        img.save(os.path.join(args.outdir, f"frame_{frame_no:05d}.png"))
    print(f"{frame_no} frames, {img_w}x{img_h}")


if __name__ == "__main__":
    main()
