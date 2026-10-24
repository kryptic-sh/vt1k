//! REP, SCP and RCP, made as tmux makes them.

use core::num::NonZeroU16;

fn parser(cols: u16) -> vt1k::Parser {
    vt1k::Parser::new(NonZeroU16::new(3).unwrap(), NonZeroU16::new(cols).unwrap(), 0)
}

fn row(p: &vt1k::Parser, row: usize) -> String {
    p.screen().rows(0, p.screen().size().1.get()).nth(row).unwrap()
}

/// REP repeats the character printed just before, no more than the columns
/// left of the row.
#[test]
fn rep_repeats_the_last_character_to_the_rows_end() {
    let mut p = parser(10);
    p.process(b"ab\x1b[3b");
    assert_eq!(row(&p, 0), "abbbb");
    p.process(b"x\x1b[99b");
    assert_eq!(row(&p, 0), "abbbbxxxxx");
    assert_eq!(row(&p, 1), "", "nothing wraps");
}

/// Anything after the character, even another REP, leaves REP nothing to
/// repeat, as tmux's INPUT_LAST.
#[test]
fn rep_after_anything_else_does_nothing() {
    let mut p = parser(20);
    p.process(b"a\x1b[2b\x1b[2b");
    assert_eq!(row(&p, 0), "aaa");
    p.process(b"\rb\x1b[m\x1b[2b");
    assert_eq!(row(&p, 0), "baa");
    p.process(b"\rc\x07\x1b[2b");
    assert_eq!(row(&p, 0), "caa");
    p.process(b"\rd\x1b]0;t\x07\x1b[2b");
    assert_eq!(row(&p, 0), "daa");
    p.process(b"\re\x1b=\x1b[2b");
    assert_eq!(row(&p, 0), "eaa");
}

/// SCP and RCP save and restore the cursor and attributes, as ESC 7 and 8.
#[test]
fn scp_and_rcp_save_and_restore_the_cursor() {
    let mut p = parser(20);
    p.process(b"\x1b[2;5H\x1b[1m\x1b[s\x1b[m\x1b[1;1H\x1b[u");
    assert_eq!(p.screen().cursor_position(), (1, 4));
    p.process(b"x");
    assert!(p.screen().cell(1, 4).unwrap().bold());
}
