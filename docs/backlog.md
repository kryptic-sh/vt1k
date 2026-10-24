# Backlog

What is not done yet, and why.

- **Take over what tmxr does before vt100.** tmxr splits sequences out of a
  pane's output before vt100 sees them (`tmxr_term::modes::ModeSplitter`, the
  DCS split in `dcs.rs`): modes 12 and 1004, DECSCUSR, OSC 4 / 10 / 11 / 12
  queries and more. vt1k could keep these itself; each moved in is one less pass
  over the output.
- **A screen's own rows are full cells** (`Cell`'s size is asserted in
  `cell.rs`): fine for a screen's worth, and what lets `Screen::cell` hand out a
  reference. Only the history is packed.
- **Kept from vt100 where it panics instead**: a character widened by what
  combines with it (U+FE0F) at the end of a one-column row, then a combining
  character, or `CSI @` with the cursor past the row's end, panic in vt100; vt1k
  leaves out what would go off the row. The parity test ends a session there
  once vt1k has gone through it.
- **Not kept from vt100 on purpose**: scrolling vt100's view widens the rows of
  history it shows, which clears their wrap flags, so merely viewing the history
  changed a later reflow. vt1k's view is a separate unpacked copy. The parity
  test scrolls copies of both screens for that reason.
- **Where vt1k keeps more than vt100, the parity test looks away**: autowrap
  (`CSI ? 7`) is left out of its modes; SGR codes only vt1k keeps are not fed
  directly, and a session ends where vt100 is handed one all the same (a partial
  colour leaves one over: 555 of 3000 release-build sessions on 2026-10-24), as
  from there the two differ on purpose. Insert mode, HTS, TBC, REP, SCP and RCP
  are never fed, and HT only from a row's start. These are checked against tmux
  instead, in tmxr's oracle.
- **Not checked**: the formatted and diff writers vt100 has
  (`contents_formatted`, `state_diff`, …) are left out, as neither tmxr nor hjkl
  uses them.
