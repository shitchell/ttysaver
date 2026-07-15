# ttysaver

Run any command as a fullscreen terminal screensaver. It takes over the screen
ncurses-style, renders the command's output, and **any keypress drops you back
to your shell** — think `cmatrix -s`, but for *any* program.

The command runs inside an off-screen pty of a chosen "virtual" size. ttysaver
parses whatever it draws into a cell grid, then composites that grid onto your
real terminal each frame — optionally **scaled up**, **centered**, and/or
**bounced around** DVD-logo style. Your keystrokes never reach the child, which
is exactly why a single key always exits, even over an interactive app like
`htop`.

Because a screensaver should stay up until you dismiss it, the last frame is
**held on screen even after the command exits** — so short-lived commands work
too (`ttysaver --bounce hostname` bounces the hostname). A keypress is the only
thing that ends it. (If the command printed nothing, ttysaver just exits rather
than trapping you on a blank screen.)

## Usage

```
ttysaver [OPTIONS] [--] <command> [args...]
```

| Option | Meaning |
|--------|---------|
| `--zoom <N \| XxY>` | Nearest-neighbour scale. `4` = 4× both axes; `4x2` = 4 wide, 2 tall. Colour is preserved. Default 1. |
| `--size <WxH>` | Virtual screen size in cells the command thinks it has. Default: fills the terminal (or terminal ÷ zoom). Also sets an explicit box and disables auto-crop. |
| `--bounce` | Bounce the output around the terminal, DVD-logo style. |
| `--center` | Center the output in the terminal. |
| `--speed <N>` | Bounce speed in **cells/second** (fractions allowed, e.g. `0.5`, `8`, `30`). Default 8. Independent of `--fps`. |
| `--fps <N>` | Frame rate / render smoothness (1–240). Default 30. Does **not** affect bounce pace. |
| `-h`, `--help` | Help (`-H` / `--help-all` for advanced options). |

When centering or bouncing, ttysaver **auto-crops to the drawn content** (the
tight bounding box of everything the command has drawn, grown over time so it
never jitters). So a small clock is centered/bounced as the clock, not as an
empty full-screen grid — no `--size` needed. Use `--` before the command when it
has its own flags, so they aren't parsed as ttysaver options.

### Advanced (`-H`)

| Option | Meaning |
|--------|---------|
| `--no-crop` | When centering/bouncing, use the whole virtual screen as the box instead of cropping to content. |
| `--exit-on-eof` | Exit as soon as the command exits, instead of holding its last frame until a keypress. |

## Config

Set your own defaults in `~/.config/ttysaver/config.toml` (or
`$XDG_CONFIG_HOME/ttysaver/config.toml`). Precedence is **built-in < config <
CLI flag**, so a flag always wins for that run.

```toml
# Supported keys under [defaults]: speed, fps, zoom.
[defaults]
speed = 2      # bounce speed in cells/second (fractions ok)
# fps  = 30    # render smoothness (1-240)
# zoom = 1     # "4" = 4x both axes, or "4x2" = 4 wide x 2 tall
```

A missing file or a malformed value is ignored silently (a screensaver
shouldn't refuse to start over a config typo).

## Examples

```sh
ttysaver htop                              # fullscreen; any key exits
ttysaver --zoom 6 tty-clock                # giant clock, colour intact
ttysaver --center tty-clock                # centered clock (no --size needed)
ttysaver --bounce tty-clock                # bounce the clock itself
ttysaver --bounce hostname                 # bounce a one-shot command's output
ttysaver --zoom 2 --bounce cmatrix         # scaled + bouncing
ttysaver -- sh -c 'while :; do date; sleep 1; done'
```

## Build

```sh
cargo build --release
# binary: target/release/ttysaver
```

Built on `portable-pty` (spawn child in a sized pty), `vt100` (parse its output
into a colour cell grid), and `crossterm` (raw mode, alt-screen, key events).

## Notes / limits

- Optimised for ASCII/box-art TUIs (clocks, `htop`, `cmatrix`). Wide/CJK glyphs
  may misalign by a column under zoom.
- The child is killed on exit; the terminal is always restored (even on panic).
