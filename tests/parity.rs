//! vt1k against vt100 (the kryptic-sh fork tmxr used before it): the same
//! random sessions fed to both, everything either shows compared after each
//! step. `VT1K_PARITY_CASES` sets how many sessions (default 300);
//! `VT1K_PARITY_SEED` runs one.

use std::fmt::Write as _;
use std::num::NonZeroU16;

/// xorshift64*, enough to vary sessions reproducibly.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

const TEXT: &[&str] = &[
    "a",
    "hello",
    "word ",
    " ",
    "  ",
    "xyz",
    "0123456789",
    "漢",
    "字",
    "😀",
    "e\u{301}",
    "\u{301}",
    "\u{fe0f}",
    "❤\u{fe0f}",
    "👨\u{200d}👩\u{200d}👧",
    "ñ",
    "\u{fffd}",
    "\u{85}",
    "ｗ",
    "─│┌",
];
/// HT only from a row's start: past the last column vt1k's stays put, as
/// tmux's, where vt100's comes back to the last column.
const CONTROLS: &[&str] = &[
    "\r", "\n", "\r\n", "\x08", "\r\t", "\x07", "\x0b", "\x0c", "\x01", "\x0e", "\x0f", "\x1b7",
    "\x1b8", "\x1b=", "\x1b>", "\x1bM", "\x1bg", "\x1b(B", "\x1bc", "\x1bZ", "\x18",
];
/// Not `g` (TBC), whose tab stops vt1k keeps and vt100 does not.
const CSI_FINALS: &[char] = &[
    '@', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'J', 'K', 'L', 'M', 'P', 'S', 'T', 'X', 'b', 'd',
    'f', 'r', 't', '`', 'n', 'c', 'q', 'u', 's',
];
/// The DEC private modes compared: not 7 (autowrap), which vt100 ignores.
const DEC_MODES: &[u16] = &[
    1, 6, 9, 12, 25, 47, 1000, 1002, 1003, 1004, 1005, 1006, 1047, 1049, 2004, 2026,
];
/// The SGR codes vt1k keeps and vt100 hands to the callbacks: blink,
/// hidden, strikethrough and overline, set and reset.
const VT1K_ONLY_SGR: &[u16] = &[5, 6, 8, 9, 25, 28, 29, 53, 55];

/// An SGR that reaches a code of [`VT1K_ONLY_SGR`], read as both read it:
/// a colour's arguments skipped, and a colour that cannot be read ending
/// it. From there on vt1k's cells may carry attributes vt100's do not,
/// which shows in what a row counts as written, so the session ends.
fn keeps_more(i1: Option<u8>, params: &[&[u16]], c: char) -> bool {
    if c != 'm' || i1.is_some() {
        return false;
    }
    let byte = |p: Option<&[u16]>| matches!(p, Some([n]) if *n <= 255);
    let mut iter = params.iter().copied();
    while let Some(param) = iter.next() {
        match param {
            [38 | 48 | 58] => match iter.next() {
                // All three are read, as the parsers read them.
                Some([2]) if byte(iter.next()) & byte(iter.next()) & byte(iter.next()) => {}
                Some([5]) if byte(iter.next()) => {}
                _ => return false,
            },
            [n] if VT1K_ONLY_SGR.contains(n) => return true,
            _ => {}
        }
    }
    false
}

const SGR: &[&str] = &[
    "0",
    "1",
    "2",
    "3",
    "4",
    "7",
    "21",
    "22",
    "23",
    "24",
    "27",
    "31",
    "39",
    "42",
    "49",
    "93",
    "104",
    "38;5;200",
    "48;5;17",
    "38;2;1;2;3",
    "48;2;9;8;7",
    "4:3",
    "4:0",
    "4:5",
    "4:9",
    "58:2::255:0:0",
    "58:2:1:2:3",
    "58;5;9",
    "58;2;10;20;30",
    "59",
    "38:5:12",
    "38:2::4:5:6",
    "38;5",
    "38;2;1",
    "38;7",
    "48;5;300",
    // Codes neither keeps, handed to the callbacks.
    "26",
    "60",
    "",
];
const OSC: &[&str] = &[
    "0;title",
    "2;a;b",
    "1;icon",
    "8;;https://example.com/a",
    "8;id=x;https://e.com/b;c",
    "8;;",
    "52;c;aGVsbG8=",
    "52;c;?",
    "52;z;aGk=",
    "52;c;!!",
    "7;file:///tmp",
    "133;A",
    "10;?",
];

fn chunk(rng: &mut Rng) -> String {
    let mut s = String::new();
    for _ in 0..1 + rng.below(6) {
        match rng.below(10) {
            0..=3 => s.push_str(rng.pick(TEXT)),
            4 => s.push_str(rng.pick(CONTROLS)),
            5 | 6 => {
                let prefix = *rng.pick(&["", "", "", "?", " ", ">", "<", "="]);
                let n = rng.below(4);
                let params: Vec<String> = (0..n)
                    .map(|_| {
                        rng.pick(&[0u64, 1, 2, 3, 5, 8, 24, 80, 200, 70000])
                            .to_string()
                    })
                    .collect();
                let _ = write!(
                    s,
                    "\x1b[{prefix}{}{}",
                    params.join(";"),
                    rng.pick(CSI_FINALS)
                );
            }
            7 => {
                let modes: Vec<String> = (0..1 + rng.below(2))
                    .map(|_| rng.pick(DEC_MODES).to_string())
                    .collect();
                let _ = write!(s, "\x1b[?{}{}", modes.join(";"), rng.pick(&['h', 'l']));
            }
            8 => {
                let codes: Vec<&str> = (0..1 + rng.below(3)).map(|_| *rng.pick(SGR)).collect();
                let _ = write!(s, "\x1b[{}m", codes.join(";"));
            }
            _ => {
                let end = *rng.pick(&["\x07", "\x1b\\"]);
                let _ = write!(s, "\x1b]{}{end}", rng.pick(OSC));
            }
        }
    }
    s
}

#[derive(Default)]
struct Log(Vec<String>);

macro_rules! callbacks {
    ($krate:ident) => {
        impl $krate::Callbacks for Log {
            fn audible_bell(&mut self, _: &mut $krate::Screen) {
                self.0.push("bell".into());
            }
            fn visual_bell(&mut self, _: &mut $krate::Screen) {
                self.0.push("visual bell".into());
            }
            fn resize(&mut self, _: &mut $krate::Screen, r: (u16, u16)) {
                self.0.push(format!("resize {r:?}"));
            }
            fn set_window_icon_name(&mut self, _: &mut $krate::Screen, s: &[u8]) {
                self.0.push(format!("icon {s:?}"));
            }
            fn set_window_title(&mut self, _: &mut $krate::Screen, s: &[u8]) {
                self.0.push(format!("title {s:?}"));
            }
            fn copy_to_clipboard(&mut self, _: &mut $krate::Screen, t: &[u8], d: &[u8]) {
                self.0.push(format!("copy {t:?} {d:?}"));
            }
            fn paste_from_clipboard(&mut self, _: &mut $krate::Screen, t: &[u8]) {
                self.0.push(format!("paste {t:?}"));
            }
            fn unhandled_char(&mut self, _: &mut $krate::Screen, c: char) {
                self.0.push(format!("char {c:?}"));
            }
            fn unhandled_control(&mut self, _: &mut $krate::Screen, b: u8) {
                self.0.push(format!("control {b}"));
            }
            fn unhandled_escape(
                &mut self,
                _: &mut $krate::Screen,
                i1: Option<u8>,
                i2: Option<u8>,
                b: u8,
            ) {
                self.0.push(format!("escape {i1:?} {i2:?} {b}"));
            }
            fn unhandled_csi(
                &mut self,
                _: &mut $krate::Screen,
                i1: Option<u8>,
                i2: Option<u8>,
                p: &[&[u16]],
                c: char,
            ) {
                self.0.push(format!("csi {i1:?} {i2:?} {p:?} {c}"));
            }
            fn unhandled_osc(&mut self, _: &mut $krate::Screen, p: &[&[u8]]) {
                self.0.push(format!("osc {p:?}"));
            }
            fn on_scroll(&mut self, row: $krate::capture::RowContents<'_>, alternate: bool) {
                let _ = row;
                self.0.push(format!("scroll {alternate}"));
            }
        }
    };
}

callbacks!(vt1k);

/// vt100's callbacks, recorded alike, and whether it was handed an SGR
/// code vt1k keeps.
struct OldLog(Log, bool);

impl vt100::Callbacks for OldLog {
    fn audible_bell(&mut self, _: &mut vt100::Screen) {
        self.0.0.push("bell".into());
    }
    fn visual_bell(&mut self, _: &mut vt100::Screen) {
        self.0.0.push("visual bell".into());
    }
    fn resize(&mut self, _: &mut vt100::Screen, r: (u16, u16)) {
        self.0.0.push(format!("resize {r:?}"));
    }
    fn set_window_icon_name(&mut self, _: &mut vt100::Screen, s: &[u8]) {
        self.0.0.push(format!("icon {s:?}"));
    }
    fn set_window_title(&mut self, _: &mut vt100::Screen, s: &[u8]) {
        self.0.0.push(format!("title {s:?}"));
    }
    fn copy_to_clipboard(&mut self, _: &mut vt100::Screen, t: &[u8], d: &[u8]) {
        self.0.0.push(format!("copy {t:?} {d:?}"));
    }
    fn paste_from_clipboard(&mut self, _: &mut vt100::Screen, t: &[u8]) {
        self.0.0.push(format!("paste {t:?}"));
    }
    fn unhandled_char(&mut self, _: &mut vt100::Screen, c: char) {
        self.0.0.push(format!("char {c:?}"));
    }
    fn unhandled_control(&mut self, _: &mut vt100::Screen, b: u8) {
        self.0.0.push(format!("control {b}"));
    }
    fn unhandled_escape(&mut self, _: &mut vt100::Screen, i1: Option<u8>, i2: Option<u8>, b: u8) {
        self.0.0.push(format!("escape {i1:?} {i2:?} {b}"));
    }
    fn unhandled_csi(
        &mut self,
        _: &mut vt100::Screen,
        i1: Option<u8>,
        i2: Option<u8>,
        p: &[&[u16]],
        c: char,
    ) {
        self.1 |= keeps_more(i1, p, c);
        self.0.0.push(format!("csi {i1:?} {i2:?} {p:?} {c}"));
    }
    fn unhandled_osc(&mut self, _: &mut vt100::Screen, p: &[&[u8]]) {
        self.0.0.push(format!("osc {p:?}"));
    }
    fn on_scroll(&mut self, row: vt100::capture::RowContents<'_>, alternate: bool) {
        let _ = row;
        self.0.0.push(format!("scroll {alternate}"));
    }
}

/// One cell as both crates can describe it, its link resolved.
macro_rules! describe_cell {
    ($screen:expr, $c:expr) => {{
        let c = $c;
        let link = c
            .hyperlink_id()
            .and_then(|id| $screen.hyperlink(id))
            .map(|h| (h.uri.clone(), h.id.clone()));
        format!(
            "{:?} w{}{} fg{:?} bg{:?} {}{}{}{}{} ul{:?}/{:?} link{:?}",
            c.contents(),
            u8::from(c.is_wide()),
            u8::from(c.is_wide_continuation()),
            c.fgcolor(),
            c.bgcolor(),
            u8::from(c.bold()),
            u8::from(c.dim()),
            u8::from(c.italic()),
            u8::from(c.underline()),
            u8::from(c.inverse()),
            c.underline_style(),
            c.underline_color(),
            link,
        )
    }};
}

/// A cell never written, as `describe_cell!` gives it.
const BLANK: &str = "\"\" w00 fgDefault bgDefault 00000 ulNone/Default linkNone";

/// Everything a screen shows, as text.
macro_rules! describe {
    ($screen:expr) => {{
        let s = $screen;
        let (rows, cols) = s.size();
        let mut out = format!(
            "size {rows}x{cols} cursor {:?} scrollback {}/{} alt {} keypad {} cursor-keys {} \
             hide {} paste {} mouse {:?} {:?} cursor-style {:?}\ncontents {:?}\n",
            s.cursor_position(),
            s.scrollback(),
            s.scrollback_len(),
            s.alternate_screen(),
            s.application_keypad(),
            s.application_cursor(),
            s.hide_cursor(),
            s.bracketed_paste(),
            s.mouse_protocol_mode(),
            s.mouse_protocol_encoding(),
            s.cursor_style(),
            s.contents(),
        );
        for row in 0..rows.get() {
            let _ = write!(out, "row {row} wrap {}:", u8::from(s.row_wrapped(row)));
            for col in 0..cols.get() {
                if let Some(c) = s.cell(row, col) {
                    let _ = write!(out, " [{}]", describe_cell!(s, c));
                }
            }
            out.push('\n');
            if let Some(cells) = s.normal_screen_row_cells(row) {
                let _ = write!(
                    out,
                    "  under {}:",
                    u8::from(s.normal_screen_row_wrapped(row))
                );
                for c in cells.iter() {
                    let _ = write!(out, " [{}]", describe_cell!(s, c));
                }
                out.push('\n');
            }
        }
        if s.scrollback() == 0 {
            for line in 0..s.scrollback_len() + usize::from(rows.get()) {
                let cells = s.line_cells(line).unwrap_or_default();
                let cells: Vec<String> = cells.iter().map(|c| describe_cell!(s, c)).collect();
                let blank = BLANK;
                let kept = cells.iter().rposition(|c| *c != blank).map_or(0, |i| i + 1);
                let _ = writeln!(
                    out,
                    "line {line} wrap {}: {}",
                    u8::from(s.line_wrapped(line)),
                    cells[..kept].join(" ")
                );
            }
        }
        out
    }};
}

fn size(n: u64) -> NonZeroU16 {
    NonZeroU16::new(n as u16).unwrap_or(NonZeroU16::MIN)
}

fn run(seed: u64) {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let (rows, cols) = (size(1 + rng.below(12)), size(1 + rng.below(40)));
    let history = *rng.pick(&[0, 3, 50]);
    let reflow = rng.chance(50);
    let mut new = vt1k::Parser::new_with_callbacks(rows, cols, history, Log::default());
    let mut old =
        vt100::Parser::new_with_callbacks(rows, cols, history, OldLog(Log::default(), false));
    new.screen_mut().set_reflow(reflow);
    old.screen_mut().set_reflow(reflow);
    let mut steps = String::new();
    for step in 0..120 {
        match rng.below(40) {
            0 => {
                let (r, c) = (size(1 + rng.below(12)), size(1 + rng.below(40)));
                let _ = writeln!(steps, "set_size {r}x{c}");
                new.set_size(r, c);
                old.set_size(r, c);
            }
            1 => {
                let (r, c) = (size(1 + rng.below(12)), size(1 + rng.below(40)));
                let _ = writeln!(steps, "screen set_size {r}x{c}");
                new.screen_mut().set_size(r, c);
                old.screen_mut().set_size(r, c);
            }
            2 => {
                // On copies: vt100 clears the wrap flags of the history
                // rows its view shows (it widens them), so a view would
                // change what it shows later; vt1k does not.
                let n = rng.below(8) as usize;
                let _ = writeln!(steps, "set_scrollback {n} (on copies)");
                let (mut a, mut b) = (new.screen().clone(), old.screen().clone());
                a.set_scrollback(n);
                b.set_scrollback(n);
                let (a, b) = (describe!(&a), describe!(&b));
                assert_eq!(a, b, "seed {seed} step {step}: views differ\n{steps}");
            }
            3 => {
                let _ = writeln!(steps, "clear_scrollback");
                new.screen_mut().clear_scrollback();
                old.screen_mut().clear_scrollback();
            }
            4 => {
                let _ = writeln!(steps, "clear_hyperlinks");
                new.screen_mut().clear_hyperlinks();
                old.screen_mut().clear_hyperlinks();
            }
            _ => {
                let bytes = chunk(&mut rng);
                let _ = writeln!(steps, "process {bytes:?}");
                // Fed in two pieces at a random byte, to cross sequences.
                let at = rng.below(bytes.len() as u64 + 1) as usize;
                // vt100 first: some input makes it panic (a combining
                // character after a widened one on a one-column screen),
                // which ends the session once vt1k has shown it does not.
                let old_fed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    old.process(&bytes.as_bytes()[..at]);
                    old.process(&bytes.as_bytes()[at..]);
                }));
                let fed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    new.process(&bytes.as_bytes()[..at]);
                    new.process(&bytes.as_bytes()[at..]);
                }));
                assert!(
                    fed.is_ok(),
                    "seed {seed} step {step}: vt1k panicked\nsteps:\n{steps}"
                );
                if old_fed.is_err() || old.callbacks().1 {
                    return;
                }
            }
        }
        let reflowed = (
            new.screen_mut().take_reflow().map(|r| (r.lines, r.dropped)),
            old.screen_mut().take_reflow().map(|r| (r.lines, r.dropped)),
        );
        assert_eq!(
            reflowed.0, reflowed.1,
            "seed {seed} step {step}: reflow differs\n{steps}"
        );
        let (a, b) = (describe!(new.screen()), describe!(old.screen()));
        if a != b {
            let first = a
                .lines()
                .zip(b.lines())
                .find(|(x, y)| x != y)
                .map(|(x, y)| format!("vt1k:  {x}\nvt100: {y}"))
                .unwrap_or_default();
            panic!(
                "seed {seed} step {step} (reflow {reflow}, history {history}): screens differ\n{first}\nsteps:\n{steps}"
            );
        }
        assert_eq!(
            new.callbacks().0,
            old.callbacks().0.0,
            "seed {seed} step {step}: callbacks differ\nsteps:\n{steps}"
        );
    }
}

#[test]
fn vt1k_shows_what_vt100_shows() {
    if let Ok(seed) = std::env::var("VT1K_PARITY_SEED") {
        run(seed.parse().unwrap());
        return;
    }
    let cases: u64 = std::env::var("VT1K_PARITY_CASES")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(300);
    for seed in 0..cases {
        if std::panic::catch_unwind(|| run(seed)).is_err() {
            panic!("seed {seed} failed (VT1K_PARITY_SEED={seed} reruns it)");
        }
    }
}
