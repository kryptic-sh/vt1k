//! Attributes vt100 did not keep, kept as tmux keeps them.

use core::num::NonZeroU16;

/// Each of blink, hidden, strikethrough and overline as the cell `(row,
/// col)` has it.
fn flags(screen: &vt1k::Screen, row: u16, col: u16) -> [bool; 4] {
    let cell = screen.cell(row, col).unwrap();
    [
        cell.blink(),
        cell.hidden(),
        cell.strikethrough(),
        cell.overline(),
    ]
}

/// SGR 5 or 6, 8, 9 and 53 set them and 25, 28, 29 and 55 reset them, as
/// tmux's input_csi_dispatch_sgr; SGR 0 resets them all.
#[test]
fn sgr_sets_and_resets_each() {
    let mut p = vt1k::Parser::new(NonZeroU16::new(2).unwrap(), NonZeroU16::new(20).unwrap(), 0);
    p.process(b"\x1b[5ma\x1b[6;8mb\x1b[9mc\x1b[53md\x1b[25me\x1b[28mf\x1b[29mg\x1b[55mh");
    p.process(b"\x1b[5;8;9;53mi\x1b[0mj");
    let s = p.screen();
    let got: Vec<_> = (0..10).map(|col| flags(s, 0, col)).collect();
    assert_eq!(
        got,
        [
            [true, false, false, false],
            [true, true, false, false],
            [true, true, true, false],
            [true, true, true, true],
            [false, true, true, true],
            [false, false, true, true],
            [false, false, false, true],
            [false, false, false, false],
            [true, true, true, true],
            [false, false, false, false],
        ]
    );
}

/// They stay with a row packed into the history and read back from it.
#[test]
fn they_survive_the_history() {
    let mut p = vt1k::Parser::new(NonZeroU16::new(2).unwrap(), NonZeroU16::new(8).unwrap(), 10);
    p.process(b"\x1b[9;53mab\x1b[m\x1b[5;8mc\x1b[m\r\n\r\n\r\n");
    assert_eq!(p.screen().scrollback_len(), 2);
    let cells = p.screen().line_cells(0).unwrap();
    let got: Vec<_> = cells[..3]
        .iter()
        .map(|c| [c.blink(), c.hidden(), c.strikethrough(), c.overline()])
        .collect();
    assert_eq!(
        got,
        [
            [false, false, true, true],
            [false, false, true, true],
            [true, true, false, false],
        ]
    );
}
