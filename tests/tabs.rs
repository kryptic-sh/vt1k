//! Tab stops, kept as tmux keeps them (vt100's were fixed every 8 columns).

use core::num::NonZeroU16;

fn parser(cols: u16) -> vt1k::Parser {
    vt1k::Parser::new(
        NonZeroU16::new(3).unwrap(),
        NonZeroU16::new(cols).unwrap(),
        0,
    )
}

fn col(p: &vt1k::Parser) -> u16 {
    p.screen().cursor_position().1
}

/// Every 8 columns to begin with; HT goes to the next, or the last column.
#[test]
fn ht_goes_to_the_next_stop() {
    let mut p = parser(20);
    assert_eq!(p.screen().tab_stops().collect::<Vec<_>>(), [8, 16]);
    p.process(b"\t");
    assert_eq!(col(&p), 8);
    p.process(b"\t\t");
    assert_eq!(col(&p), 19);
}

/// HTS sets a stop at the cursor; TBC 0 clears the one there, TBC 3 all.
#[test]
fn hts_and_tbc_set_and_clear_stops() {
    let mut p = parser(20);
    p.process(b"\x1b[4G\x1bH\x1b[9G\x1b[g\r");
    assert_eq!(p.screen().tab_stops().collect::<Vec<_>>(), [3, 16]);
    p.process(b"\t");
    assert_eq!(col(&p), 3);
    p.process(b"\t");
    assert_eq!(col(&p), 16);
    p.process(b"\x1b[3g\r\t");
    assert_eq!(p.screen().tab_stops().count(), 0);
    assert_eq!(col(&p), 19);
}

/// CBT goes back as many stops, no further than the row's start.
#[test]
fn cbt_goes_back_stops() {
    let mut p = parser(30);
    p.process(b"\x1b[26G\x1b[2Z");
    assert_eq!(col(&p), 16);
    p.process(b"\x1b[3G\x1b[1:2Z");
    assert_eq!(col(&p), 2, "a count with a colon is ignored");
    p.process(b"\x1b[5Z");
    assert_eq!(col(&p), 0);
}

/// From past the last column HT leaves the cursor there, so what comes
/// next wraps, as tmux's input_c0_dispatch.
#[test]
fn ht_past_the_last_column_stays() {
    let mut p = parser(4);
    p.process(b"abcd\tx");
    let rows: Vec<String> = p.screen().rows(0, 4).collect();
    assert_eq!(rows[..2], ["abcd", "x"]);
}

/// A new width or a full reset brings back the default stops.
#[test]
fn a_new_width_or_a_reset_resets_them() {
    let mut p = parser(20);
    p.process(b"\x1b[3g");
    p.screen_mut()
        .set_size(NonZeroU16::new(3).unwrap(), NonZeroU16::new(20).unwrap());
    assert_eq!(
        p.screen().tab_stops().count(),
        0,
        "the same width keeps them"
    );
    p.screen_mut()
        .set_size(NonZeroU16::new(3).unwrap(), NonZeroU16::new(18).unwrap());
    assert_eq!(p.screen().tab_stops().collect::<Vec<_>>(), [8, 16]);
    p.process(b"\x1b[3g\x1bc");
    assert_eq!(p.screen().tab_stops().collect::<Vec<_>>(), [8, 16]);
}
