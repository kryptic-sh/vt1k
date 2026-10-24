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
- **Insert mode and autowrap kept**, which vt100 ignored: in insert mode
  (`CSI 4 h`, `Screen::insert_mode`) text pushes the rest of its row right;
  without autowrap (`CSI ? 7 l`, `Screen::autowrap`) text stops at the last
  column, overwriting it, and a wide character that does not fit is left out, as
  tmux's screen_write_cell.
