# vt1k

A terminal emulator's screen for Rust: a program's output in, cells, history,
cursor and modes out. Built for [tmxr](https://github.com/kryptic-sh/tmxr), a
tmux-compatible terminal multiplexer.

```rust
use std::num::NonZeroU16;

let rows = NonZeroU16::new(24).unwrap();
let cols = NonZeroU16::new(80).unwrap();
let mut parser = vt1k::Parser::new(rows, cols, 1000);
parser.process(b"\x1b[1;31mhello\x1b[m world");
let screen = parser.screen();
assert_eq!(screen.contents(), "hello world");
assert!(screen.cell(0, 0).unwrap().bold());
```

Escape sequences are read with [vte](https://crates.io/crates/vte). The screen
keeps:

- text with its colours, bold, dim, italic, inverse, underline style (single,
  double, curly, dotted, dashed) and colour, and OSC 8 hyperlink;
- the normal screen with its history and the alternate screen, the normal screen
  readable under the alternate one;
- rows of history packed small, unpacked only where a whole row is read;
- the cursor, scroll region, origin mode, keypad and cursor-key modes, bracketed
  paste, mouse reporting and cursor style;
- optionally, the normal screen rewrapped on a width change, as tmux's
  `grid_reflow` ([`Screen::set_reflow`]), with each line's new place reported.

What it does not keep (titles, the bell, the clipboard, sequences it does not
handle) goes to a [`Callbacks`] implementation.

Its behaviour follows [vt100](https://crates.io/crates/vt100) as tmxr used it:
`tests/parity.rs` feeds random sessions to both and compares everything either
shows.

## License

MIT
