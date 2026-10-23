//! What a [`Parser`](crate::Parser) tells its owner about as it goes.

/// Things a program's output asks for that the screen does not keep, and
/// sequences it does not handle. Each method does nothing by default.
pub trait Callbacks {
    /// BEL.
    fn audible_bell(&mut self, _: &mut crate::Screen) {}

    /// `ESC g`.
    fn visual_bell(&mut self, _: &mut crate::Screen) {}

    /// `CSI 8 ; rows ; cols t`: the program asks for a size.
    fn resize(&mut self, _: &mut crate::Screen, _request: (u16, u16)) {}

    /// OSC 0 and OSC 1.
    fn set_window_icon_name(&mut self, _: &mut crate::Screen, _icon_name: &[u8]) {}

    /// OSC 0 and OSC 2.
    fn set_window_title(&mut self, _: &mut crate::Screen, _title: &[u8]) {}

    /// OSC 52 with data: `ty` the selections, `data` still base64.
    fn copy_to_clipboard(&mut self, _: &mut crate::Screen, _ty: &[u8], _data: &[u8]) {}

    /// OSC 52 with `?`.
    fn paste_from_clipboard(&mut self, _: &mut crate::Screen, _ty: &[u8]) {}

    /// A character not drawn: U+FFFD, or a C1 control.
    fn unhandled_char(&mut self, _: &mut crate::Screen, _c: char) {}

    /// A C0 control not handled.
    fn unhandled_control(&mut self, _: &mut crate::Screen, _b: u8) {}

    /// An escape sequence not handled.
    fn unhandled_escape(
        &mut self,
        _: &mut crate::Screen,
        _i1: Option<u8>,
        _i2: Option<u8>,
        _b: u8,
    ) {
    }

    /// A CSI sequence, or one of its parameters, not handled: its
    /// intermediates, every parameter (with its colon sub-parameters) and
    /// its final character.
    fn unhandled_csi(
        &mut self,
        _: &mut crate::Screen,
        _i1: Option<u8>,
        _i2: Option<u8>,
        _params: &[&[u16]],
        _c: char,
    ) {
    }

    /// An OSC not handled, split at its `;`s.
    fn unhandled_osc(&mut self, _: &mut crate::Screen, _params: &[&[u8]]) {}

    /// A row is lost: off the top of a full history, or off the top of a
    /// screen that keeps none (the alternate screen's when
    /// `alternate_screen`). Not called by [`Screen::set_size`](crate::Screen::set_size),
    /// [`Screen::clear_scrollback`](crate::Screen::clear_scrollback) or a reflow.
    fn on_scroll(&mut self, _contents: crate::capture::RowContents<'_>, _alternate_screen: bool) {}
}

impl Callbacks for () {}
