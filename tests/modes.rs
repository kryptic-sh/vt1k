//! Modes vt100 did not keep, kept as tmux keeps them.

use core::num::NonZeroU16;

fn parser(rows: u16, cols: u16) -> vt1k::Parser {
    vt1k::Parser::new(
        NonZeroU16::new(rows).unwrap(),
        NonZeroU16::new(cols).unwrap(),
        0,
    )
}

fn row(p: &vt1k::Parser, row: u16) -> String {
    p.screen()
        .rows(0, p.screen().size().1.get())
        .nth(usize::from(row))
        .unwrap()
}

/// Without autowrap, text stops at the last column, each character after
/// overwriting it, as tmux's screen_write_cell (rows checked against tmux
/// 3.6).
#[test]
fn without_autowrap_text_sticks_at_the_last_column() {
    let mut p = parser(3, 5);
    assert!(p.screen().autowrap());
    p.process(b"\x1b[?7lhello world");
    assert!(!p.screen().autowrap());
    assert_eq!(row(&p, 0), "helld");
    assert_eq!(row(&p, 1), "");
    assert_eq!(p.screen().cursor_position(), (0, 4));
    // A wide character that does not fit is left out.
    p.process(b"\r\x1b[4C\xe6\xbc\xa2");
    assert_eq!(row(&p, 0), "helld");
    // Back on, text wraps again.
    p.process(b"\x1b[?7h\rabcdefg");
    assert_eq!((row(&p, 0).as_str(), row(&p, 1).as_str()), ("abcde", "fg"));
}

/// In insert mode text pushes the rest of the row right, what is pushed
/// past the end lost.
#[test]
fn insert_mode_pushes_the_row_right() {
    let mut p = parser(2, 8);
    p.process(b"abcdef\r\x1b[4hXY");
    assert!(p.screen().insert_mode());
    assert_eq!(row(&p, 0), "XYabcdef");
    p.process(b"Z");
    assert_eq!(row(&p, 0), "XYZabcde");
    p.process(b"\x1b[4lQ");
    assert!(!p.screen().insert_mode());
    assert_eq!(row(&p, 0), "XYZQbcde");
}

/// Other SM / RM parameters go to the callbacks, insert mode does not.
#[test]
fn other_modes_go_to_the_callbacks() {
    #[derive(Default)]
    struct Seen(Vec<Vec<u16>>);
    impl vt1k::Callbacks for Seen {
        fn unhandled_csi(
            &mut self,
            _: &mut vt1k::Screen,
            _: Option<u8>,
            _: Option<u8>,
            params: &[&[u16]],
            _: char,
        ) {
            self.0
                .push(params.iter().flat_map(|p| p.iter().copied()).collect());
        }
    }
    let mut p = vt1k::Parser::new_with_callbacks(
        NonZeroU16::new(2).unwrap(),
        NonZeroU16::new(8).unwrap(),
        0,
        Seen::default(),
    );
    p.process(b"\x1b[4;20h\x1b[34l");
    assert!(p.screen().insert_mode());
    assert_eq!(p.callbacks().0, [vec![4, 20], vec![34]]);
}
