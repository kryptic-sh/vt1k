# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **The screen**: `Parser` reads a program's output (with vte) into a `Screen`
  of cells, history, cursor and modes, telling `Callbacks` of what it does not
  keep. Its API and behaviour follow vt100 0.18 with the kryptic-sh fork's
  additions (OSC 8 hyperlinks, underline styles and colours, lines read by
  number, the normal screen under the alternate one, reflow on a width change),
  checked against it by a differential test.
- **History rows packed small**: a row that scrolls into the history is kept as
  its text, a byte a cell and runs of attributes, instead of full cells;
  `Screen::line_cells` unpacks one when asked.
- **Blink, hidden, strikethrough and overline kept**, which vt100 ignored: SGR 5
  or 6, 8, 9 and 53 set them and 25, 28, 29 and 55 reset them, as tmux's
  input_csi_dispatch_sgr (`Cell::blink`, `hidden`, `strikethrough`, `overline`).
  A cell takes two bytes more for them.
- **The alternate screen, mouse modes, scroll region and cursor as tmux keeps
  them**: 47, 1047 and 1049 each switch to a cleared alternate screen with the
  cursor where it was (1049 saving it for the way back,
  `Screen::alternate_saved`), and a host can turn the switches off
  (`Screen::set_alternate_allowed`); mode 9 is not taken, resetting any of 1000
  to 1003 stops whichever is set, and 1005 and 1006 are kept apart
  (`Screen::mouse_utf8`); one scroll region serves both screens, DECSTBM ignores
  a region of fewer than two rows, and IL / DL outside it move the rows from the
  cursor down; DECSCUSR's shape survives a full reset and an odd one blinks
  (`Screen::cursor_blinking`, mode 12), focus reports (1004) and a very visible
  cursor (`CSI 34 l`) are modes. `CursorStyle::decscusr` gives a style's number.
  A full reset keeps reflow and the host's settings.
- **REP, SCP and RCP made as tmux makes them**: REP (`CSI b`) repeats only a
  character with nothing after it, and no further than the row's end; SCP and
  RCP (`CSI s`, `CSI u`) save and restore the cursor as `ESC 7` and `ESC 8`.
- **Tab stops kept as tmux keeps them**, where vt100's were fixed every 8
  columns: HTS (`ESC H`) sets one, TBC (`CSI g`, 0 or 3) clears one or all, CBT
  (`CSI Z`) goes back, and HT past the last column leaves the cursor there, as
  tmux's. A new width or a full reset brings back the defaults.
  `Screen::tab_stops` lists them and `Screen::reset_tab_stops` puts the defaults
  back.
- **Insert mode and autowrap kept**, which vt100 ignored: in insert mode
  (`CSI 4 h`, `Screen::insert_mode`) text pushes the rest of its row right;
  without autowrap (`CSI ? 7 l`, `Screen::autowrap`) text stops at the last
  column, overwriting it, and a wide character that does not fit is left out, as
  tmux's screen_write_cell.
