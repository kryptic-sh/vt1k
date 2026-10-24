//! Modes kept as tmux keeps them, where vt100's differed: the alternate
//! screen, the mouse, the scroll region, the cursor's shape and focus.

use core::num::NonZeroU16;

fn parser(rows: u16, cols: u16) -> vt1k::Parser {
    vt1k::Parser::new(
        NonZeroU16::new(rows).unwrap(),
        NonZeroU16::new(cols).unwrap(),
        10,
    )
}

fn rows(p: &vt1k::Parser) -> Vec<String> {
    p.screen().rows(0, p.screen().size().1.get()).collect()
}

/// Each of 47, 1047 and 1049 switches to a cleared alternate screen with
/// the cursor where it was; back, 1049 puts the cursor it saved back and
/// the others leave it where it is.
#[test]
fn the_alternate_screen_switches_as_tmuxs() {
    for mode in ["47", "1047", "1049"] {
        let mut p = parser(4, 10);
        p.process(format!("\x1b[?{mode}h old\x1b[?{mode}l").as_bytes());
        p.process(b"\x1b[2;3Hmain");
        p.process(format!("\x1b[?{mode}h").as_bytes());
        assert!(p.screen().alternate_screen(), "{mode}");
        assert_eq!(
            p.screen().cursor_position(),
            (1, 6),
            "{mode}: the cursor stays"
        );
        assert!(rows(&p).iter().all(String::is_empty), "{mode}: cleared");
        p.process(b"\x1b[4;1Halt");
        p.process(format!("\x1b[?{mode}l").as_bytes());
        assert!(!p.screen().alternate_screen(), "{mode}");
        let want = if mode == "1049" { (1, 6) } else { (3, 3) };
        assert_eq!(p.screen().cursor_position(), want, "{mode}");
        assert_eq!(rows(&p)[1], "  main", "{mode}");
    }
    let mut p = parser(4, 10);
    p.process(b"\x1b[2;3H\x1b[?1049h\x1b[?1049h");
    assert_eq!(p.screen().alternate_saved(), Some((1, 2)));
    p.process(b"\x1b[?47h");
    assert_eq!(
        p.screen().alternate_saved(),
        Some((1, 2)),
        "47 saves nothing"
    );
    p.process(b"\x1bc");
    assert_eq!(
        p.screen().alternate_saved(),
        Some((1, 2)),
        "a reset keeps it"
    );
}

/// With switches not allowed (tmux's `alternate-screen off`) they do
/// nothing.
#[test]
fn switches_not_allowed_do_nothing() {
    let mut p = parser(4, 10);
    p.screen_mut().set_alternate_allowed(false);
    p.process(b"text\x1b[?1049h");
    assert!(!p.screen().alternate_screen());
    assert_eq!(rows(&p)[0], "text");
    p.process(b"\x1bc");
    p.process(b"\x1b[?47h");
    assert!(!p.screen().alternate_screen(), "a full reset keeps it");
}

/// One scroll region for both screens, as tmux keeps it.
#[test]
fn the_scroll_region_is_kept_across_screens() {
    let mut p = parser(6, 10);
    p.process(b"\x1b[2;4r\x1b[?1049h");
    assert_eq!(p.screen().scroll_region(), (1, 3));
    p.process(b"\x1b[3;5r\x1b[?1049l");
    assert_eq!(p.screen().scroll_region(), (2, 4));
}

/// DECSTBM: rows past the screen are its last, fewer than two rows are
/// ignored (the cursor kept), and a region moves the cursor home.
#[test]
fn decstbm_as_tmuxs() {
    let mut p = parser(6, 10);
    p.process(b"\x1b[3;4H\x1b[2;99r");
    assert_eq!(p.screen().scroll_region(), (1, 5));
    assert_eq!(p.screen().cursor_position(), (0, 0));
    p.process(b"\x1b[3;4H\x1b[4;4r\x1b[5;2r\x1b[1:2;5r");
    assert_eq!(p.screen().scroll_region(), (1, 5));
    assert_eq!(p.screen().cursor_position(), (2, 3));
}

/// IL and DL with the cursor outside the region move the rows from it to
/// the screen's bottom; an IL of as many rows as are left does nothing.
#[test]
fn il_and_dl_outside_the_region_as_tmuxs() {
    let mut p = parser(6, 4);
    p.process(b"a\r\nb\r\nc\r\nd\r\ne\r\nf\x1b[2;3r\x1b[5;1H\x1b[L");
    assert_eq!(rows(&p), ["a", "b", "c", "d", "", "e"]);
    p.process(b"\x1b[5;1H\x1b[M");
    assert_eq!(rows(&p), ["a", "b", "c", "d", "e", ""]);
    p.process(b"\x1b[4;1H\x1b[3L");
    assert_eq!(rows(&p), ["a", "b", "c", "d", "e", ""], "too many: nothing");
    for not_a_number in [&b"\x1b[1;1H\x1b[1:2L"[..], b"\x1b[1;1H\x1b[1:2M"] {
        p.process(not_a_number);
        assert_eq!(rows(&p), ["a", "b", "c", "d", "e", ""]);
    }
}

/// The mouse modes as tmux's: 9 is not taken, resetting any of 1000 to
/// 1003 stops whichever is set, and 1005 and 1006 are kept apart.
#[test]
fn mouse_modes_as_tmuxs() {
    use vt1k::{MouseProtocolEncoding as E, MouseProtocolMode as M};
    let mut p = parser(2, 4);
    p.process(b"\x1b[?9h");
    assert_eq!(p.screen().mouse_protocol_mode(), M::None);
    p.process(b"\x1b[?1002h\x1b[?1000l");
    assert_eq!(p.screen().mouse_protocol_mode(), M::None);
    p.process(b"\x1b[?1005h\x1b[?1006h");
    assert_eq!(p.screen().mouse_protocol_encoding(), E::Sgr);
    assert!(p.screen().mouse_utf8());
    p.process(b"\x1b[?1006l");
    assert_eq!(p.screen().mouse_protocol_encoding(), E::Utf8);
}

/// DECSCUSR sets the shape, an odd one blinking; mode 12 the blinking
/// alone; a full reset keeps the shape and stops the blinking.
#[test]
fn the_cursors_shape_and_blinking_as_tmuxs() {
    let mut p = parser(2, 4);
    p.process(b"\x1b[3 q");
    assert_eq!(p.screen().cursor_style(), vt1k::CursorStyle::BlinkUnderline);
    assert!(p.screen().cursor_blinking());
    p.process(b"\x1b[?12l");
    assert!(!p.screen().cursor_blinking());
    p.process(b"\x1b[?12h\x1bc");
    assert_eq!(p.screen().cursor_style(), vt1k::CursorStyle::BlinkUnderline);
    assert!(!p.screen().cursor_blinking());
    p.process(b"\x1b[1:2 q");
    assert_eq!(p.screen().cursor_style(), vt1k::CursorStyle::BlinkUnderline);
    p.process(b"\x1b[6 q");
    assert!(!p.screen().cursor_blinking());
}

/// Focus reports are a mode a full reset clears.
#[test]
fn focus_reports_are_a_mode() {
    let mut p = parser(2, 4);
    p.process(b"\x1b[?1004h");
    assert!(p.screen().focus_reports());
    p.process(b"\x1bc");
    assert!(!p.screen().focus_reports());
}

/// A full reset keeps reflow on.
#[test]
fn a_full_reset_keeps_reflow() {
    let mut p = parser(3, 6);
    p.screen_mut().set_reflow(true);
    p.process(b"\x1bcabcdefgh");
    p.screen_mut()
        .set_size(NonZeroU16::new(3).unwrap(), NonZeroU16::new(8).unwrap());
    assert_eq!(rows(&p)[0], "abcdefgh");
}
