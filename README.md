# ttysaver

Run any command as a fullscreen terminal screensaver. `ttysaver` takes over the
screen, renders the command's output live, and drops you back to your shell the
moment you press a key. Think `cmatrix -s`, but for any program you like.

The command runs inside an off-screen pty at a virtual size you pick. ttysaver
parses whatever it draws into a grid of colored cells and paints that grid onto
your real terminal every frame. From there you can scale it up, center it, or
bounce it around the screen DVD-logo style. Your keystrokes never reach the
child, which is why a single key always gets you out, even on top of something
interactive like `htop`.

A screensaver should stay up until you dismiss it, so ttysaver keeps the last
frame on screen after the command exits. That means short-lived commands work
too: `ttysaver --bounce hostname` bounces your hostname around until you hit a
key. (If the command drew nothing at all, ttysaver just exits rather than
trapping you on a blank screen.)

## Demos

A giant clock, zoomed and bounced around like a DVD logo:

![ttysaver bouncing a zoomed tty-clock](assets/bounce-clock.gif)

`cmatrix` taken fullscreen. Any key drops you back to the shell:

![ttysaver running cmatrix fullscreen](assets/cmatrix.gif)

## Usage

```
ttysaver [OPTIONS] [--] <command> [args...]
```

| Option | Meaning |
|--------|---------|
| `--zoom <N \| XxY>` | Nearest-neighbour scale. `4` = 4× both axes; `4x2` = 4 wide, 2 tall. Colour is preserved. Default 1. |
| `--size <WxH>` | Virtual screen size in cells the command thinks it has. Defaults to filling the terminal (or terminal ÷ zoom). Also sets an explicit box and turns off auto-crop. |
| `--bounce` | Bounce the output around the terminal, DVD-logo style. |
| `--center` | Center the output in the terminal. |
| `--speed <N>` | Bounce speed in cells/second (fractions allowed, e.g. `0.5`, `8`, `30`). Default 8. Independent of `--fps`. |
| `--fps <N>` | Frame rate / render smoothness (1–240). Default 30. Does not change the bounce pace. |
| `-h`, `--help` | Help. Use `-H` / `--help-all` for the advanced options. |

When centering or bouncing, ttysaver auto-crops to the drawn content: the tight
bounding box of everything the command has put on screen, grown over time so it
never jitters. A small clock is centered and bounced as the clock, not as an
empty full-screen grid, so you don't need `--size`. Put `--` before the command
when it has its own flags, so they aren't read as ttysaver options.

### Advanced (`-H`)

| Option | Meaning |
|--------|---------|
| `--no-crop` | When centering or bouncing, use the whole virtual screen as the box instead of cropping to content. |
| `--exit-on-eof` | Exit as soon as the command exits, instead of holding its last frame until a keypress. |

## Config

Set your own defaults in `~/.config/ttysaver/config.toml` (or
`$XDG_CONFIG_HOME/ttysaver/config.toml`). Precedence is built-in < config < CLI
flag, so a flag always wins for that run.

```toml
# Supported keys under [defaults]: speed, fps, zoom.
[defaults]
speed = 2      # bounce speed in cells/second (fractions ok)
# fps  = 30    # render smoothness (1-240)
# zoom = 1     # "4" = 4x both axes, or "4x2" = 4 wide x 2 tall
```

A missing file or a bad value is ignored without complaint. A screensaver
shouldn't refuse to start over a config typo.

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

Pairs nicely with tmux: point `lock-command` at it (with `lock-after-time`) so
it starts when you go idle. It's a screensaver, not a lock, so any key dismisses
it.

## Build

```sh
cargo build --release
# binary: target/release/ttysaver
```

Built on `portable-pty` (spawn the child in a sized pty), `vt100` (parse its
output into a grid of colored cells), and `crossterm` (raw mode, alt-screen, key
events).

## Notes / limits

- Tuned for ASCII and box-art TUIs (clocks, `htop`, `cmatrix`). Wide and CJK
  glyphs can drift a column under zoom.
- The child is killed on exit, and the terminal is always restored, even on a
  panic.
</content>
</invoke>
