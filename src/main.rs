// ttysaver — run any command as a fullscreen terminal screensaver.
//
// The command runs inside an off-screen pty of a chosen "virtual" size. We
// parse whatever it draws into a cell grid (vt100), then composite that grid
// onto the real terminal every frame — optionally scaled up (zoom), centered,
// and/or bounced around DVD-logo style. Any keypress tears it down.
//
// The child never sees your keystrokes, which is exactly why a single key
// always exits, even when wrapping an interactive app like htop. And because a
// screensaver should stay up until you dismiss it, the last frame is HELD on
// screen even after the command exits (e.g. `ttysaver hostname`) — a keypress
// is the only thing that ends it (unless --exit-on-eof).

use std::io::{Read, Write};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::Duration;

use crossterm::style::{
    Attribute, Color as CtColor, Print, ResetColor, SetAttribute, SetBackgroundColor,
    SetForegroundColor,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, size, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{cursor, event, execute, queue};

use portable_pty::{native_pty_system, CommandBuilder, PtySize};

/// What the pty reader thread sends back to the render loop.
enum Msg {
    Data(Vec<u8>),
    Eof,
}

#[derive(Clone, Copy, PartialEq)]
struct Style {
    fg: CtColor,
    bg: CtColor,
    bold: bool,
    italic: bool,
    underline: bool,
    inverse: bool,
}

const BLANK: Style = Style {
    fg: CtColor::Reset,
    bg: CtColor::Reset,
    bold: false,
    italic: false,
    underline: false,
    inverse: false,
};

/// Tight bounding box of drawn content, grown (never shrunk) over time. Because
/// it only ever grows, it converges quickly for fixed-ish layouts (a clock's
/// digits swing in width as they change, but the box settles on the widest
/// envelope within seconds) and never jitters the centered/bounced position.
struct InkBox {
    r0: u16,
    c0: u16,
    r1: u16,
    c1: u16,
    any: bool,
}

impl InkBox {
    fn new() -> Self {
        InkBox {
            r0: u16::MAX,
            c0: u16::MAX,
            r1: 0,
            c1: 0,
            any: false,
        }
    }
    fn add(&mut self, r: u16, c: u16) {
        self.r0 = self.r0.min(r);
        self.c0 = self.c0.min(c);
        self.r1 = self.r1.max(r);
        self.c1 = self.c1.max(c);
        self.any = true;
    }
    /// (col0, row0, width, height)
    fn rect(&self) -> (u16, u16, u16, u16) {
        (self.c0, self.r0, self.c1 - self.c0 + 1, self.r1 - self.r0 + 1)
    }
}

struct Config {
    zoom_x: u16,
    zoom_y: u16,
    size: Option<(u16, u16)>, // (cols, rows)
    bounce: bool,
    center: bool,
    fps: u64,
    speed: f64,
    crop: bool,        // auto-crop content box for center/bounce (default on)
    exit_on_eof: bool, // quit when the child exits instead of holding (default off)
    command: Vec<String>,
}

fn usage(full: bool) -> ! {
    eprint!(
        "ttysaver — run any command as a fullscreen terminal screensaver.\n\
\n\
USAGE:\n\
    ttysaver [OPTIONS] [--] <command> [args...]\n\
\n\
OPTIONS:\n\
    --zoom <N | XxY>   Nearest-neighbour scale. \"4\" = 4x both axes;\n\
                       \"4x2\" = 4 wide, 2 tall. Default 1.\n\
    --size <WxH>       Virtual screen size in cells the command thinks it has.\n\
                       Default: fills the terminal (or terminal / zoom).\n\
    --bounce           Bounce the output around the terminal, DVD-logo style.\n\
    --center           Center the output in the terminal.\n\
    --speed <N>        Bounce speed in cells/second (fractions allowed).\n\
                       Default 8. Independent of --fps.\n\
    --fps <N>          Frame rate / render smoothness (1-240). Default 30.\n\
    -h, --help         This help.  (-H / --help-all for advanced options.)\n\
\n\
Any keypress exits. The command's output is held on screen until you press a\n\
key, even if the command exits on its own — so short-lived commands work too:\n\
    ttysaver --bounce hostname     # bounces the hostname around\n\
\n\
When centering or bouncing, ttysaver auto-crops to the drawn content, so a\n\
small clock is centered as the clock, not as an empty full-screen grid:\n\
    ttysaver --center tty-clock\n\
    ttysaver --zoom 6 tty-clock\n\
"
    );
    if full {
        eprint!(
            "\n\
ADVANCED:\n\
    --no-crop          When centering/bouncing, use the whole virtual screen as\n\
                       the box instead of cropping to the drawn content.\n\
    --exit-on-eof      Exit as soon as the command exits, instead of holding its\n\
                       last frame until a keypress.\n\
\n\
NOTES:\n\
    * --size sets an explicit box and disables auto-crop.\n\
    * Optimised for ASCII / box-art TUIs; wide/CJK glyphs may drift a column\n\
      under heavy zoom.\n\
"
        );
    }
    std::process::exit(2);
}

fn parse_scale(s: &str) -> Option<(u16, u16)> {
    if let Some((a, b)) = s.split_once(['x', 'X']) {
        Some((a.parse().ok()?, b.parse().ok()?))
    } else {
        let n: u16 = s.parse().ok()?;
        Some((n, n))
    }
}

fn parse_args() -> Config {
    let mut cfg = Config {
        zoom_x: 1,
        zoom_y: 1,
        size: None,
        bounce: false,
        center: false,
        fps: 30,
        speed: 8.0,
        crop: true,
        exit_on_eof: false,
        command: Vec::new(),
    };

    let mut args = std::env::args().skip(1).peekable();
    let take_val = |args: &mut std::iter::Peekable<std::iter::Skip<std::env::Args>>,
                    inline: Option<String>|
     -> String { inline.unwrap_or_else(|| args.next().unwrap_or_else(|| usage(false))) };

    while let Some(arg) = args.next() {
        if !cfg.command.is_empty() {
            cfg.command.push(arg);
            continue;
        }
        let (flag, inline) = match arg.split_once('=') {
            Some((f, v)) => (f.to_string(), Some(v.to_string())),
            None => (arg.clone(), None),
        };
        match flag.as_str() {
            "--" => cfg.command.extend(args.by_ref()),
            "-h" | "--help" => usage(false),
            "-H" | "--help-all" => usage(true),
            "--zoom" => {
                let (x, y) = parse_scale(&take_val(&mut args, inline)).unwrap_or_else(|| usage(false));
                cfg.zoom_x = x.max(1);
                cfg.zoom_y = y.max(1);
            }
            "--size" => {
                let (w, h) = parse_scale(&take_val(&mut args, inline)).unwrap_or_else(|| usage(false));
                cfg.size = Some((w.max(1), h.max(1)));
            }
            "--bounce" => cfg.bounce = true,
            "--center" => cfg.center = true,
            "--no-crop" => cfg.crop = false,
            "--exit-on-eof" => cfg.exit_on_eof = true,
            "--speed" => {
                cfg.speed = take_val(&mut args, inline)
                    .parse::<f64>()
                    .unwrap_or_else(|_| usage(false))
                    .clamp(0.1, 200.0)
            }
            "--fps" => {
                cfg.fps = take_val(&mut args, inline)
                    .parse::<u64>()
                    .unwrap_or_else(|_| usage(false))
                    .clamp(1, 240)
            }
            other if other.starts_with('-') => {
                eprintln!("ttysaver: unknown option '{other}'\n");
                usage(false);
            }
            _ => cfg.command.push(arg),
        }
    }

    if cfg.command.is_empty() {
        usage(false);
    }
    cfg
}

fn vt_to_ct(c: vt100::Color) -> CtColor {
    match c {
        vt100::Color::Default => CtColor::Reset,
        vt100::Color::Idx(i) => CtColor::AnsiValue(i),
        vt100::Color::Rgb(r, g, b) => CtColor::Rgb { r, g, b },
    }
}

fn style_of(cell: &vt100::Cell) -> Style {
    Style {
        fg: vt_to_ct(cell.fgcolor()),
        bg: vt_to_ct(cell.bgcolor()),
        bold: cell.bold(),
        italic: cell.italic(),
        underline: cell.underline(),
        inverse: cell.inverse(),
    }
}

/// A cell is "ink" if it shows a visible glyph or is painted with a non-default
/// background (tty-clock draws its digits as background-coloured blocks).
fn is_ink(cell: &vt100::Cell) -> bool {
    let c = cell.contents();
    (!c.is_empty() && c != " ") || cell.bgcolor() != vt100::Color::Default
}

fn apply_style<W: Write>(out: &mut W, s: Style) -> std::io::Result<()> {
    queue!(out, SetAttribute(Attribute::Reset))?;
    if s.bold {
        queue!(out, SetAttribute(Attribute::Bold))?;
    }
    if s.italic {
        queue!(out, SetAttribute(Attribute::Italic))?;
    }
    if s.underline {
        queue!(out, SetAttribute(Attribute::Underlined))?;
    }
    if s.inverse {
        queue!(out, SetAttribute(Attribute::Reverse))?;
    }
    queue!(out, SetForegroundColor(s.fg), SetBackgroundColor(s.bg))?;
    Ok(())
}

/// Restores the terminal no matter how we leave (clean exit, error, panic).
struct TermGuard;
impl Drop for TermGuard {
    fn drop(&mut self) {
        let mut out = std::io::stdout();
        let _ = execute!(out, cursor::Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

fn main() -> std::io::Result<()> {
    let cfg = parse_args();

    let (mut term_cols, mut term_rows) = size().unwrap_or((80, 24));
    term_cols = term_cols.max(1);
    term_rows = term_rows.max(1);

    // Virtual pty size the child believes it has.
    let (vcols, vrows) = match cfg.size {
        Some(wh) => wh,
        None => (
            (term_cols / cfg.zoom_x).max(1),
            (term_rows / cfg.zoom_y).max(1),
        ),
    };

    // Spawn the command inside a pty of the virtual size.
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: vrows,
            cols: vcols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let mut cmd = CommandBuilder::new(&cfg.command[0]);
    for a in &cfg.command[1..] {
        cmd.arg(a);
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    drop(pair.slave);

    let mut reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // Pump the child's output to the render loop over a channel.
    let (tx, rx) = mpsc::channel::<Msg>();
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = tx.send(Msg::Eof);
                    break;
                }
                Ok(n) => {
                    if tx.send(Msg::Data(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut parser = vt100::Parser::new(vrows, vcols, 0);

    // Enter raw fullscreen mode; the guard restores it on any exit path.
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    let _guard = TermGuard;

    let frame = Duration::from_millis(1000 / cfg.fps);
    let zx = cfg.zoom_x;
    let zy = cfg.zoom_y;
    let speed = cfg.speed.clamp(0.1, 200.0); // cells per second
    let positioning = cfg.bounce || cfg.center;
    // --size or --no-crop means "use the whole virtual screen as the box".
    let use_crop = cfg.crop && cfg.size.is_none();

    let mut ink = InkBox::new();
    let mut child_done = false;

    // Persistent bounce state: float top-left position (cells) accumulated so
    // movement is frame-rate-independent, plus a ±1 direction per axis. The
    // rounded positions (px/py) are derived per-frame inside the loop.
    let mut fx: f64 = 0.0;
    let mut fy: f64 = 0.0;
    let mut dir_x: f64 = 1.0;
    let mut dir_y: f64 = 1.0;

    loop {
        // Drain everything the child has drawn since last frame.
        loop {
            match rx.try_recv() {
                Ok(Msg::Data(d)) => parser.process(&d),
                Ok(Msg::Eof) => {
                    child_done = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    child_done = true;
                    break;
                }
            }
        }

        let screen = parser.screen();

        // Grow the ink box from the current grid.
        for r in 0..vrows {
            for c in 0..vcols {
                if let Some(cell) = screen.cell(r, c) {
                    if is_ink(cell) {
                        ink.add(r, c);
                    }
                }
            }
        }

        // Source rectangle in virtual-grid coords: the cropped content box when
        // positioning (and crop enabled and we've seen ink), else the whole grid.
        let (sc0, sr0, sw, sh) = if positioning && use_crop && ink.any {
            ink.rect()
        } else {
            (0, 0, vcols, vrows)
        };
        let rw = (sw as u32 * zx as u32).min(u16::MAX as u32) as u16; // rendered width
        let rh = (sh as u32 * zy as u32).min(u16::MAX as u32) as u16;

        // Position the rendered box (rounded positions used to composite).
        let px: i32;
        let py: i32;
        if cfg.bounce {
            let step = speed / cfg.fps as f64; // cells this frame
            if rw < term_cols {
                let maxx = (term_cols - rw) as f64;
                fx += dir_x * step;
                if fx <= 0.0 {
                    fx = 0.0;
                    dir_x = 1.0;
                } else if fx >= maxx {
                    fx = maxx;
                    dir_x = -1.0;
                }
                px = fx.round() as i32;
            } else {
                fx = 0.0;
                px = 0;
            }
            if rh < term_rows {
                let maxy = (term_rows - rh) as f64;
                fy += dir_y * step;
                if fy <= 0.0 {
                    fy = 0.0;
                    dir_y = 1.0;
                } else if fy >= maxy {
                    fy = maxy;
                    dir_y = -1.0;
                }
                py = fy.round() as i32;
            } else {
                fy = 0.0;
                py = 0;
            }
        } else if cfg.center {
            px = ((term_cols as i32 - rw as i32) / 2).max(0);
            py = ((term_rows as i32 - rh as i32) / 2).max(0);
        } else {
            px = 0;
            py = 0;
        }

        // Composite one full frame in a single buffered write (no flicker).
        let mut buf = Vec::with_capacity(term_cols as usize * term_rows as usize * 4);
        for ty in 0..term_rows {
            queue!(buf, cursor::MoveTo(0, ty))?;
            let mut cur: Option<Style> = None;
            for tx_ in 0..term_cols {
                let (glyph, st) =
                    cell_at(screen, tx_, ty, px, py, rw, rh, zx, zy, sc0, sr0, vcols, vrows);
                if cur != Some(st) {
                    apply_style(&mut buf, st)?;
                    cur = Some(st);
                }
                queue!(buf, Print(glyph))?;
            }
        }
        queue!(buf, ResetColor)?;
        stdout.write_all(&buf)?;
        stdout.flush()?;

        // If the child is gone: exit on empty output or when asked; otherwise
        // hold the last frame (still animating any bounce) until a keypress.
        if child_done && (!ink.any || cfg.exit_on_eof) {
            break;
        }

        // Any key exits. Resize just re-reads terminal dims.
        if event::poll(frame)? {
            match event::read()? {
                event::Event::Key(_) => break,
                event::Event::Resize(w, h) => {
                    // Next frame re-clamps the bounce (fx/fy) and recomputes
                    // center/plain positions against these new dimensions.
                    term_cols = w.max(1);
                    term_rows = h.max(1);
                }
                _ => {}
            }
        }
    }

    let _ = child.kill();
    drop(pair.master);
    Ok(())
}

/// The glyph + style to paint at real-terminal cell (tx, ty). Maps the rendered
/// box back through the zoom and the crop origin to a source cell in the grid.
#[allow(clippy::too_many_arguments)]
fn cell_at(
    screen: &vt100::Screen,
    tx: u16,
    ty: u16,
    px: i32,
    py: i32,
    rw: u16,
    rh: u16,
    zx: u16,
    zy: u16,
    sc0: u16,
    sr0: u16,
    vcols: u16,
    vrows: u16,
) -> (String, Style) {
    let bx = tx as i32 - px;
    let by = ty as i32 - py;
    if bx >= 0 && by >= 0 && (bx as u16) < rw && (by as u16) < rh {
        let sc = sc0 + bx as u16 / zx;
        let sr = sr0 + by as u16 / zy;
        if sc < vcols && sr < vrows {
            if let Some(cell) = screen.cell(sr, sc) {
                let mut c = cell.contents();
                if c.is_empty() {
                    c = " ".to_string();
                }
                return (c, style_of(cell));
            }
        }
    }
    (" ".to_string(), BLANK)
}
