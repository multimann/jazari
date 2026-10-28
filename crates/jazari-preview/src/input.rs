//! egui input to Ruffle `PlayerEvent`s.
//!
//! [`InputMapper`] turns egui events into player actions. It knows nothing
//! of Ruffle's player, so it is tested without a device. It handles:
//!
//! - pointer events in physical texture pixels relative to the image, with
//!   `MouseLeave` when the pointer exits the image (not only the window) or
//!   moves onto another egui layer covering it (a menu, a popup), and moves
//!   still forwarded while a button pressed inside is held (drags); held
//!   buttons are released when the pointer leaves the window, because egui
//!   reports no release after that;
//! - wheel units (points, lines, pages);
//! - keys and text only while the preview has focus, a `TextControl` code
//!   after each key that edits text (as Ruffle desktop does), paste as typed
//!   text, and IME preedit/commit;
//! - releasing held keys when focus is lost, so `Key.isDown` never sticks.

use eframe::egui;
use ruffle_core::PlayerEvent;
use ruffle_core::backend::ui::MouseCursor;
use ruffle_core::events::{
    ImeEvent, KeyDescriptor, KeyLocation, LogicalKey, MouseButton, MouseWheelDelta, NamedKey,
    PhysicalKey, TextControlCode,
};

/// How many lines a wheel "page" scrolls (egui's `MouseWheelUnit::Page`,
/// which Ruffle has no unit for).
const PAGE_LINES: f64 = 10.0;

/// Something to do to the player.
#[derive(Debug, Clone)]
pub(crate) enum InputAction {
    /// Send this event.
    Event(PlayerEvent),
    /// Tell the player whether the pointer is over the stage.
    MouseInStage(bool),
}

/// Where the preview image is and how big its texture is.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Viewport {
    /// The image's rectangle on screen, in points.
    pub(crate) rect: egui::Rect,
    /// The texture size in physical pixels.
    pub(crate) size_px: (u32, u32),
    /// egui's pixels per point.
    pub(crate) ppp: f32,
    /// The pointer is not really over the image although it is inside
    /// `rect`: another egui layer covers it, or that part of the image is
    /// clipped away. Positions inside `rect` then count as outside.
    pub(crate) pointer_blocked: bool,
}

impl Viewport {
    /// Whether `pos` is over the visible, uncovered image.
    fn contains(self, pos: egui::Pos2) -> bool {
        !self.pointer_blocked && self.rect.contains(pos)
    }

    /// A screen position as physical texture pixels relative to the image.
    /// The image may be shown at a different size than the texture (when the
    /// texture is clamped to the device limits), so the scale comes from the
    /// two sizes, not from `ppp`.
    fn to_px(self, pos: egui::Pos2) -> Option<(f64, f64)> {
        let (w, h) = (self.rect.width(), self.rect.height());
        if !(w > 0.0 && h > 0.0) {
            return None;
        }
        let x = f64::from(pos.x - self.rect.min.x) * f64::from(self.size_px.0) / f64::from(w);
        let y = f64::from(pos.y - self.rect.min.y) * f64::from(self.size_px.1) / f64::from(h);
        Some((x, y))
    }
}

/// Pointer, focus and key state between frames.
#[derive(Debug, Default)]
pub(crate) struct InputMapper {
    hovered: bool,
    buttons: u8,
    focused: bool,
    keys_down: Vec<KeyDescriptor>,
    /// The last position sent to the player, for releases that come
    /// without a usable position.
    last_px: Option<(f64, f64)>,
}

impl InputMapper {
    /// Whether a mouse button pressed over the image is still held.
    pub(crate) fn dragging(&self) -> bool {
        self.buttons != 0
    }

    /// Whether the pointer is over the image.
    #[cfg(test)]
    pub(crate) fn hovered(&self) -> bool {
        self.hovered
    }

    /// Whether key and text events are forwarded.
    pub(crate) fn focused(&self) -> bool {
        self.focused
    }

    /// Sets the focus; on a change, sends `FocusGained` or `FocusLost` (and
    /// releases every held key).
    pub(crate) fn set_focused(&mut self, focused: bool, out: &mut Vec<InputAction>) {
        if focused == self.focused {
            return;
        }
        self.focused = focused;
        if focused {
            out.push(InputAction::Event(PlayerEvent::FocusGained));
        } else {
            for key in self.keys_down.drain(..) {
                out.push(InputAction::Event(PlayerEvent::KeyUp { key }));
            }
            out.push(InputAction::Event(PlayerEvent::FocusLost));
        }
    }

    /// Translates one egui event.
    pub(crate) fn map(&mut self, event: &egui::Event, vp: Viewport, out: &mut Vec<InputAction>) {
        match event {
            egui::Event::PointerMoved(pos) => self.pointer_moved(*pos, vp, out),
            egui::Event::PointerButton {
                pos,
                button,
                pressed,
                ..
            } => self.pointer_button(*pos, *button, *pressed, vp, out),
            egui::Event::PointerGone => {
                // egui sends no release once the pointer has left the
                // window, so release held buttons here; otherwise Ruffle
                // would keep them down (and `onRelease` would never fire).
                self.release_all(out);
                if self.hovered {
                    self.leave(out);
                }
            }
            egui::Event::MouseWheel { unit, delta, .. } if self.hovered => {
                let delta = wheel_delta(*unit, f64::from(delta.y), f64::from(vp.ppp));
                out.push(InputAction::Event(PlayerEvent::MouseWheel { delta }));
            }
            egui::Event::Text(text) if self.focused => {
                out.extend(
                    text.chars()
                        .filter(|c| !c.is_control())
                        .map(|codepoint| InputAction::Event(PlayerEvent::TextInput { codepoint })),
                );
            }
            egui::Event::Paste(text) if self.focused => {
                // Ruffle reads pasted text from its UI backend's clipboard,
                // which the null backend does not have; typing it works.
                for c in text.chars() {
                    if c == '\n' {
                        out.push(InputAction::Event(PlayerEvent::TextControl {
                            code: TextControlCode::Enter,
                        }));
                    } else if !c.is_control() {
                        out.push(InputAction::Event(PlayerEvent::TextInput { codepoint: c }));
                    }
                }
            }
            egui::Event::Key {
                key,
                physical_key,
                pressed,
                modifiers,
                ..
            } if self.focused => self.key(*key, *physical_key, *pressed, *modifiers, out),
            egui::Event::Ime(ime) if self.focused => match ime {
                egui::ImeEvent::Preedit { text, .. } => {
                    out.push(InputAction::Event(PlayerEvent::Ime(ImeEvent::Preedit(
                        text.clone(),
                        None,
                    ))));
                }
                egui::ImeEvent::Commit(text) => {
                    out.push(InputAction::Event(PlayerEvent::Ime(ImeEvent::Commit(
                        text.clone(),
                    ))));
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn enter(&mut self, out: &mut Vec<InputAction>) {
        if !self.hovered {
            self.hovered = true;
            out.push(InputAction::MouseInStage(true));
        }
    }

    fn leave(&mut self, out: &mut Vec<InputAction>) {
        self.hovered = false;
        out.push(InputAction::MouseInStage(false));
        out.push(InputAction::Event(PlayerEvent::MouseLeave));
    }

    /// Sends `MouseUp` at the last known position for every held button.
    fn release_all(&mut self, out: &mut Vec<InputAction>) {
        for button in [
            MouseButton::Left,
            MouseButton::Right,
            MouseButton::Middle,
            MouseButton::Unknown,
        ] {
            if self.buttons & button_bit(button) != 0
                && let Some((x, y)) = self.last_px
            {
                out.push(InputAction::Event(PlayerEvent::MouseUp { x, y, button }));
            }
        }
        self.buttons = 0;
    }

    fn pointer_moved(&mut self, pos: egui::Pos2, vp: Viewport, out: &mut Vec<InputAction>) {
        let inside = vp.contains(pos);
        if inside {
            self.enter(out);
        }
        if inside || self.dragging() {
            if let Some((x, y)) = vp.to_px(pos) {
                self.last_px = Some((x, y));
                out.push(InputAction::Event(PlayerEvent::MouseMove { x, y }));
            }
        } else if self.hovered {
            self.leave(out);
        }
    }

    fn pointer_button(
        &mut self,
        pos: egui::Pos2,
        button: egui::PointerButton,
        pressed: bool,
        vp: Viewport,
        out: &mut Vec<InputAction>,
    ) {
        let button = mouse_button(button);
        let bit = button_bit(button);
        let inside = vp.contains(pos);
        if pressed {
            if let (true, Some((x, y))) = (inside, vp.to_px(pos)) {
                self.enter(out);
                self.buttons |= bit;
                self.last_px = Some((x, y));
                out.push(InputAction::Event(PlayerEvent::MouseDown {
                    x,
                    y,
                    button,
                    index: None,
                }));
            }
        } else if self.buttons & bit != 0 {
            // Always clear the button, even when the image has no usable
            // size any more, or the drag would never end.
            self.buttons &= !bit;
            if let Some((x, y)) = vp.to_px(pos).or(self.last_px) {
                self.last_px = Some((x, y));
                out.push(InputAction::Event(PlayerEvent::MouseUp { x, y, button }));
            }
            if !inside && !self.dragging() && self.hovered {
                self.leave(out);
            }
        }
    }

    fn key(
        &mut self,
        key: egui::Key,
        physical: Option<egui::Key>,
        pressed: bool,
        modifiers: egui::Modifiers,
        out: &mut Vec<InputAction>,
    ) {
        let descriptor = key_descriptor(key, physical, modifiers);
        let same_key = |k: &KeyDescriptor| {
            k.physical_key == descriptor.physical_key && k.key_location == descriptor.key_location
        };
        if pressed {
            if !self.keys_down.iter().any(same_key) {
                self.keys_down.push(descriptor);
            }
            out.push(InputAction::Event(PlayerEvent::KeyDown { key: descriptor }));
            if let Some(code) = text_control(key, modifiers) {
                out.push(InputAction::Event(PlayerEvent::TextControl { code }));
            }
        } else {
            self.keys_down.retain(|k| !same_key(k));
            out.push(InputAction::Event(PlayerEvent::KeyUp { key: descriptor }));
        }
    }
}

fn mouse_button(button: egui::PointerButton) -> MouseButton {
    match button {
        egui::PointerButton::Primary => MouseButton::Left,
        egui::PointerButton::Secondary => MouseButton::Right,
        egui::PointerButton::Middle => MouseButton::Middle,
        egui::PointerButton::Extra1 | egui::PointerButton::Extra2 => MouseButton::Unknown,
    }
}

fn button_bit(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
        MouseButton::Unknown => 8,
    }
}

/// egui wheel deltas to Ruffle's. egui's points are winit's physical pixels
/// divided by `ppp`, so they are multiplied back; the sign convention is the
/// same as winit's, which Ruffle desktop passes through unchanged.
fn wheel_delta(unit: egui::MouseWheelUnit, dy: f64, ppp: f64) -> MouseWheelDelta {
    match unit {
        egui::MouseWheelUnit::Point => MouseWheelDelta::Pixels(dy * ppp),
        egui::MouseWheelUnit::Line => MouseWheelDelta::Lines(dy),
        egui::MouseWheelUnit::Page => MouseWheelDelta::Lines(dy * PAGE_LINES),
    }
}

/// Ruffle's cursor as an egui cursor.
pub(crate) fn cursor_icon(cursor: MouseCursor) -> egui::CursorIcon {
    match cursor {
        MouseCursor::Arrow => egui::CursorIcon::Default,
        MouseCursor::Hand => egui::CursorIcon::PointingHand,
        MouseCursor::IBeam => egui::CursorIcon::Text,
        MouseCursor::Grab => egui::CursorIcon::Grab,
    }
}

/// The text-field command for a key press, following Ruffle desktop's
/// `winit_to_ruffle_text_control`. Copy, cut and paste arrive from egui as
/// their own events, not as keys.
fn text_control(key: egui::Key, modifiers: egui::Modifiers) -> Option<TextControlCode> {
    use TextControlCode as C;
    use egui::Key as K;
    let cmd = modifiers.command;
    let shift = modifiers.shift;
    Some(match key {
        K::Enter => C::Enter,
        K::A if cmd => C::SelectAll,
        K::Backspace if cmd => C::BackspaceWord,
        K::Backspace => C::Backspace,
        K::Delete if cmd => C::DeleteWord,
        K::Delete => C::Delete,
        K::ArrowLeft => match (cmd, shift) {
            (true, true) => C::SelectLeftWord,
            (true, false) => C::MoveLeftWord,
            (false, true) => C::SelectLeft,
            (false, false) => C::MoveLeft,
        },
        K::ArrowRight => match (cmd, shift) {
            (true, true) => C::SelectRightWord,
            (true, false) => C::MoveRightWord,
            (false, true) => C::SelectRight,
            (false, false) => C::MoveRight,
        },
        K::Home => match (cmd, shift) {
            (true, true) => C::SelectLeftDocument,
            (true, false) => C::MoveLeftDocument,
            (false, true) => C::SelectLeftLine,
            (false, false) => C::MoveLeftLine,
        },
        K::End => match (cmd, shift) {
            (true, true) => C::SelectRightDocument,
            (true, false) => C::MoveRightDocument,
            (false, true) => C::SelectRightLine,
            (false, false) => C::MoveRightLine,
        },
        _ => return None,
    })
}

/// A Ruffle key descriptor for an egui key event.
///
/// egui's `key` is the logical key when egui knows it (so Shift+1 on a US
/// layout arrives as `Exclamationmark`), otherwise the physical one; its
/// `physical_key` is the position. Letters carry no case, so Shift decides
/// it. egui does not report numpad keys or a non-Latin layout's character,
/// so those map to the main-block key and its Latin character.
pub(crate) fn key_descriptor(
    key: egui::Key,
    physical: Option<egui::Key>,
    modifiers: egui::Modifiers,
) -> KeyDescriptor {
    let position = physical.unwrap_or(key);
    KeyDescriptor {
        physical_key: physical_key(position),
        logical_key: logical_key(key, modifiers.shift),
        key_location: key_location(position),
    }
}

fn key_location(key: egui::Key) -> KeyLocation {
    use egui::Key as K;
    match key {
        K::ShiftLeft | K::ControlLeft | K::AltLeft | K::SuperLeft => KeyLocation::Left,
        K::ShiftRight | K::ControlRight | K::AltRight | K::SuperRight => KeyLocation::Right,
        _ => KeyLocation::Standard,
    }
}

fn letter(key: egui::Key) -> Option<char> {
    use egui::Key as K;
    const LETTERS: [egui::Key; 26] = [
        K::A,
        K::B,
        K::C,
        K::D,
        K::E,
        K::F,
        K::G,
        K::H,
        K::I,
        K::J,
        K::K,
        K::L,
        K::M,
        K::N,
        K::O,
        K::P,
        K::Q,
        K::R,
        K::S,
        K::T,
        K::U,
        K::V,
        K::W,
        K::X,
        K::Y,
        K::Z,
    ];
    let index = LETTERS.iter().position(|k| *k == key)?;
    Some(char::from(b'a' + u8::try_from(index).ok()?))
}

fn digit(key: egui::Key) -> Option<u8> {
    use egui::Key as K;
    const DIGITS: [egui::Key; 10] = [
        K::Num0,
        K::Num1,
        K::Num2,
        K::Num3,
        K::Num4,
        K::Num5,
        K::Num6,
        K::Num7,
        K::Num8,
        K::Num9,
    ];
    DIGITS
        .iter()
        .position(|k| *k == key)
        .and_then(|i| u8::try_from(i).ok())
}

fn function_key(key: egui::Key) -> Option<u8> {
    use egui::Key as K;
    const KEYS: [egui::Key; 35] = [
        K::F1,
        K::F2,
        K::F3,
        K::F4,
        K::F5,
        K::F6,
        K::F7,
        K::F8,
        K::F9,
        K::F10,
        K::F11,
        K::F12,
        K::F13,
        K::F14,
        K::F15,
        K::F16,
        K::F17,
        K::F18,
        K::F19,
        K::F20,
        K::F21,
        K::F22,
        K::F23,
        K::F24,
        K::F25,
        K::F26,
        K::F27,
        K::F28,
        K::F29,
        K::F30,
        K::F31,
        K::F32,
        K::F33,
        K::F34,
        K::F35,
    ];
    KEYS.iter()
        .position(|k| *k == key)
        .and_then(|i| u8::try_from(i + 1).ok())
}

fn logical_key(key: egui::Key, shift: bool) -> LogicalKey {
    use LogicalKey::{Character as Ch, Named};
    use egui::Key as K;
    if let Some(c) = letter(key) {
        return Ch(if shift { c.to_ascii_uppercase() } else { c });
    }
    if let Some(d) = digit(key) {
        return Ch(char::from(b'0' + d));
    }
    if let Some(n) = function_key(key) {
        return named_function_key(n).map_or(LogicalKey::Unknown, Named);
    }
    match key {
        K::ArrowDown => Named(NamedKey::ArrowDown),
        K::ArrowLeft => Named(NamedKey::ArrowLeft),
        K::ArrowRight => Named(NamedKey::ArrowRight),
        K::ArrowUp => Named(NamedKey::ArrowUp),
        K::Escape => Named(NamedKey::Escape),
        K::Tab => Named(NamedKey::Tab),
        K::Backspace => Named(NamedKey::Backspace),
        K::Enter => Named(NamedKey::Enter),
        K::Space => Ch(' '),
        K::Insert => Named(NamedKey::Insert),
        K::Delete => Named(NamedKey::Delete),
        K::Home => Named(NamedKey::Home),
        K::End => Named(NamedKey::End),
        K::PageUp => Named(NamedKey::PageUp),
        K::PageDown => Named(NamedKey::PageDown),
        K::Copy => Named(NamedKey::Copy),
        K::Cut => Named(NamedKey::Cut),
        K::Paste => Named(NamedKey::Paste),
        K::Colon => Ch(':'),
        K::Comma => Ch(if shift { '<' } else { ',' }),
        K::Backslash | K::IntlBackslash => Ch(if shift { '|' } else { '\\' }),
        K::Slash => Ch(if shift { '?' } else { '/' }),
        K::Pipe => Ch('|'),
        K::Questionmark => Ch('?'),
        K::Exclamationmark => Ch('!'),
        K::OpenBracket => Ch(if shift { '{' } else { '[' }),
        K::CloseBracket => Ch(if shift { '}' } else { ']' }),
        K::OpenCurlyBracket => Ch('{'),
        K::CloseCurlyBracket => Ch('}'),
        K::Backtick => Ch(if shift { '~' } else { '`' }),
        K::Minus => Ch(if shift { '_' } else { '-' }),
        K::Period => Ch(if shift { '>' } else { '.' }),
        K::Plus => Ch('+'),
        K::Equals => Ch(if shift { '+' } else { '=' }),
        K::Semicolon => Ch(if shift { ':' } else { ';' }),
        K::Quote => Ch(if shift { '"' } else { '\'' }),
        K::ShiftLeft | K::ShiftRight => Named(NamedKey::Shift),
        K::ControlLeft | K::ControlRight => Named(NamedKey::Control),
        K::AltLeft | K::AltRight => Named(NamedKey::Alt),
        K::SuperLeft | K::SuperRight => Named(NamedKey::Super),
        _ => LogicalKey::Unknown,
    }
}

fn named_function_key(n: u8) -> Option<NamedKey> {
    use NamedKey as N;
    const KEYS: [NamedKey; 35] = [
        N::F1,
        N::F2,
        N::F3,
        N::F4,
        N::F5,
        N::F6,
        N::F7,
        N::F8,
        N::F9,
        N::F10,
        N::F11,
        N::F12,
        N::F13,
        N::F14,
        N::F15,
        N::F16,
        N::F17,
        N::F18,
        N::F19,
        N::F20,
        N::F21,
        N::F22,
        N::F23,
        N::F24,
        N::F25,
        N::F26,
        N::F27,
        N::F28,
        N::F29,
        N::F30,
        N::F31,
        N::F32,
        N::F33,
        N::F34,
        N::F35,
    ];
    KEYS.get(usize::from(n).checked_sub(1)?).copied()
}

fn physical_key(key: egui::Key) -> PhysicalKey {
    use PhysicalKey as P;
    use egui::Key as K;
    const LETTERS: [PhysicalKey; 26] = [
        P::KeyA,
        P::KeyB,
        P::KeyC,
        P::KeyD,
        P::KeyE,
        P::KeyF,
        P::KeyG,
        P::KeyH,
        P::KeyI,
        P::KeyJ,
        P::KeyK,
        P::KeyL,
        P::KeyM,
        P::KeyN,
        P::KeyO,
        P::KeyP,
        P::KeyQ,
        P::KeyR,
        P::KeyS,
        P::KeyT,
        P::KeyU,
        P::KeyV,
        P::KeyW,
        P::KeyX,
        P::KeyY,
        P::KeyZ,
    ];
    const DIGITS: [PhysicalKey; 10] = [
        P::Digit0,
        P::Digit1,
        P::Digit2,
        P::Digit3,
        P::Digit4,
        P::Digit5,
        P::Digit6,
        P::Digit7,
        P::Digit8,
        P::Digit9,
    ];
    const FUNCTION: [PhysicalKey; 35] = [
        P::F1,
        P::F2,
        P::F3,
        P::F4,
        P::F5,
        P::F6,
        P::F7,
        P::F8,
        P::F9,
        P::F10,
        P::F11,
        P::F12,
        P::F13,
        P::F14,
        P::F15,
        P::F16,
        P::F17,
        P::F18,
        P::F19,
        P::F20,
        P::F21,
        P::F22,
        P::F23,
        P::F24,
        P::F25,
        P::F26,
        P::F27,
        P::F28,
        P::F29,
        P::F30,
        P::F31,
        P::F32,
        P::F33,
        P::F34,
        P::F35,
    ];
    if let Some(c) = letter(key) {
        return LETTERS[usize::from(c as u8 - b'a')];
    }
    if let Some(d) = digit(key) {
        return DIGITS[usize::from(d)];
    }
    if let Some(n) = function_key(key) {
        return FUNCTION[usize::from(n - 1)];
    }
    match key {
        K::ArrowDown => P::ArrowDown,
        K::ArrowLeft => P::ArrowLeft,
        K::ArrowRight => P::ArrowRight,
        K::ArrowUp => P::ArrowUp,
        K::Escape => P::Escape,
        K::Tab => P::Tab,
        K::Backspace => P::Backspace,
        K::Enter => P::Enter,
        K::Space => P::Space,
        K::Insert => P::Insert,
        K::Delete => P::Delete,
        K::Home => P::Home,
        K::End => P::End,
        K::PageUp => P::PageUp,
        K::PageDown => P::PageDown,
        // Logical-only keys get the position they have on a US layout.
        K::Colon | K::Semicolon => P::Semicolon,
        K::Comma => P::Comma,
        K::Backslash | K::Pipe => P::Backslash,
        K::IntlBackslash => P::IntlBackslash,
        K::Slash | K::Questionmark => P::Slash,
        K::Exclamationmark => P::Digit1,
        K::OpenBracket | K::OpenCurlyBracket => P::BracketLeft,
        K::CloseBracket | K::CloseCurlyBracket => P::BracketRight,
        K::Backtick => P::Backquote,
        K::Minus => P::Minus,
        K::Period => P::Period,
        K::Plus | K::Equals => P::Equal,
        K::Quote => P::Quote,
        K::ShiftLeft => P::ShiftLeft,
        K::ShiftRight => P::ShiftRight,
        K::ControlLeft => P::ControlLeft,
        K::ControlRight => P::ControlRight,
        K::AltLeft => P::AltLeft,
        K::AltRight => P::AltRight,
        K::SuperLeft => P::SuperLeft,
        K::SuperRight => P::SuperRight,
        _ => P::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Modifiers, Pos2, Rect, pos2, vec2};

    fn viewport() -> Viewport {
        // A 200x100-point image at (10, 20), shown at 2 pixels per point.
        Viewport {
            rect: Rect::from_min_size(pos2(10.0, 20.0), vec2(200.0, 100.0)),
            size_px: (400, 200),
            ppp: 2.0,
            pointer_blocked: false,
        }
    }

    fn events(mapper: &mut InputMapper, evs: &[egui::Event]) -> Vec<InputAction> {
        let mut out = Vec::new();
        for ev in evs {
            mapper.map(ev, viewport(), &mut out);
        }
        out
    }

    fn moved(x: f32, y: f32) -> egui::Event {
        egui::Event::PointerMoved(Pos2::new(x, y))
    }

    fn button(x: f32, y: f32, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: Pos2::new(x, y),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    #[test]
    fn pointer_positions_are_physical_pixels_relative_to_the_image() {
        let mut m = InputMapper::default();
        let out = events(&mut m, &[moved(60.0, 70.0)]);
        assert!(matches!(out[0], InputAction::MouseInStage(true)));
        match &out[1] {
            InputAction::Event(PlayerEvent::MouseMove { x, y }) => {
                assert_eq!((*x, *y), (100.0, 100.0));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn leaving_the_image_sends_mouse_leave() {
        let mut m = InputMapper::default();
        events(&mut m, &[moved(60.0, 70.0)]);
        let out = events(&mut m, &[moved(500.0, 70.0)]);
        assert!(matches!(out[0], InputAction::MouseInStage(false)));
        assert!(matches!(
            out[1],
            InputAction::Event(PlayerEvent::MouseLeave)
        ));
        assert!(!m.hovered());
        // Further moves outside are dropped.
        assert!(events(&mut m, &[moved(510.0, 70.0)]).is_empty());
    }

    #[test]
    fn drags_keep_forwarding_until_release() {
        let mut m = InputMapper::default();
        events(&mut m, &[moved(60.0, 70.0), button(60.0, 70.0, true)]);
        assert!(m.dragging());
        let out = events(&mut m, &[moved(500.0, 70.0)]);
        assert!(matches!(
            out[0],
            InputAction::Event(PlayerEvent::MouseMove { .. })
        ));
        let out = events(&mut m, &[button(500.0, 70.0, false)]);
        assert!(matches!(
            out[0],
            InputAction::Event(PlayerEvent::MouseUp { .. })
        ));
        assert!(matches!(
            out.last(),
            Some(InputAction::Event(PlayerEvent::MouseLeave))
        ));
    }

    #[test]
    fn a_covered_image_gets_no_presses_and_loses_hover() {
        let covered = Viewport {
            pointer_blocked: true,
            ..viewport()
        };
        let mut m = InputMapper::default();
        events(&mut m, &[moved(60.0, 70.0)]);
        assert!(m.hovered());
        // A menu now covers the image: the pointer over it has left the
        // movie, and a click on the menu must not reach the movie.
        let mut out = Vec::new();
        m.map(&moved(61.0, 70.0), covered, &mut out);
        m.map(&button(61.0, 70.0, true), covered, &mut out);
        m.map(&button(61.0, 70.0, false), covered, &mut out);
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(matches!(out[0], InputAction::MouseInStage(false)));
        assert!(matches!(
            out[1],
            InputAction::Event(PlayerEvent::MouseLeave)
        ));
        assert!(!m.hovered());
        assert!(!m.dragging());
    }

    #[test]
    fn leaving_the_window_during_a_drag_releases_the_button() {
        let mut m = InputMapper::default();
        events(&mut m, &[moved(60.0, 70.0), button(60.0, 70.0, true)]);
        let out = events(&mut m, &[moved(300.0, 70.0), egui::Event::PointerGone]);
        assert!(!m.dragging());
        let up = out
            .iter()
            .position(|a| matches!(a, InputAction::Event(PlayerEvent::MouseUp { .. })))
            .expect("a MouseUp");
        match &out[up] {
            InputAction::Event(PlayerEvent::MouseUp { x, y, button }) => {
                // The last position the movie saw (outside the image).
                assert_eq!((*x, *y, *button), (580.0, 100.0, MouseButton::Left));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(matches!(
            out.last(),
            Some(InputAction::Event(PlayerEvent::MouseLeave))
        ));
    }

    #[test]
    fn a_release_on_a_collapsed_image_still_ends_the_drag() {
        let mut m = InputMapper::default();
        events(&mut m, &[moved(60.0, 70.0), button(60.0, 70.0, true)]);
        let collapsed = Viewport {
            rect: Rect::from_min_size(pos2(10.0, 20.0), vec2(0.0, 0.0)),
            ..viewport()
        };
        let mut out = Vec::new();
        m.map(&button(60.0, 70.0, false), collapsed, &mut out);
        assert!(!m.dragging());
        assert!(matches!(
            out[0],
            InputAction::Event(PlayerEvent::MouseUp { x, y, .. }) if (x, y) == (100.0, 100.0)
        ));
    }

    #[test]
    fn presses_outside_the_image_are_ignored() {
        let mut m = InputMapper::default();
        assert!(events(&mut m, &[button(500.0, 70.0, true)]).is_empty());
        assert!(events(&mut m, &[button(500.0, 70.0, false)]).is_empty());
    }

    #[test]
    fn keys_need_focus_and_add_text_control_codes() {
        let mut m = InputMapper::default();
        let backspace = egui::Event::Key {
            key: egui::Key::Backspace,
            physical_key: Some(egui::Key::Backspace),
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        assert!(events(&mut m, std::slice::from_ref(&backspace)).is_empty());
        let mut out = Vec::new();
        m.set_focused(true, &mut out);
        assert!(matches!(
            out[0],
            InputAction::Event(PlayerEvent::FocusGained)
        ));
        let out = events(&mut m, &[backspace]);
        assert!(matches!(
            out[0],
            InputAction::Event(PlayerEvent::KeyDown { .. })
        ));
        assert!(matches!(
            out[1],
            InputAction::Event(PlayerEvent::TextControl {
                code: TextControlCode::Backspace
            })
        ));
        // Losing focus releases the held key.
        let mut out = Vec::new();
        m.set_focused(false, &mut out);
        assert!(matches!(
            out[0],
            InputAction::Event(PlayerEvent::KeyUp { .. })
        ));
        assert!(matches!(out[1], InputAction::Event(PlayerEvent::FocusLost)));
    }

    #[test]
    fn letters_take_their_case_from_shift() {
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let d = key_descriptor(egui::Key::Q, Some(egui::Key::Q), shift);
        assert_eq!(d.logical_key, LogicalKey::Character('Q'));
        assert_eq!(d.physical_key, PhysicalKey::KeyQ);
        let d = key_descriptor(egui::Key::Num7, None, Modifiers::NONE);
        assert_eq!(d.logical_key, LogicalKey::Character('7'));
        assert_eq!(d.physical_key, PhysicalKey::Digit7);
        let d = key_descriptor(egui::Key::F12, None, Modifiers::NONE);
        assert_eq!(d.logical_key, LogicalKey::Named(NamedKey::F12));
        assert_eq!(d.physical_key, PhysicalKey::F12);
        let d = key_descriptor(egui::Key::ShiftRight, None, Modifiers::NONE);
        assert_eq!(d.key_location, KeyLocation::Right);
    }

    #[test]
    fn wheel_units_map_to_ruffle_units() {
        let mut m = InputMapper::default();
        events(&mut m, &[moved(60.0, 70.0)]);
        let wheel = |unit| egui::Event::MouseWheel {
            unit,
            delta: vec2(0.0, 3.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
        };
        let out = events(
            &mut m,
            &[
                wheel(egui::MouseWheelUnit::Point),
                wheel(egui::MouseWheelUnit::Line),
            ],
        );
        match &out[0] {
            InputAction::Event(PlayerEvent::MouseWheel {
                delta: MouseWheelDelta::Pixels(p),
            }) => assert_eq!(*p, 6.0),
            other => panic!("unexpected {other:?}"),
        }
        match &out[1] {
            InputAction::Event(PlayerEvent::MouseWheel {
                delta: MouseWheelDelta::Lines(l),
            }) => assert_eq!(*l, 3.0),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn paste_is_typed() {
        let mut m = InputMapper::default();
        let mut out = Vec::new();
        m.set_focused(true, &mut out);
        let out = events(&mut m, &[egui::Event::Paste("a\nب".to_owned())]);
        assert_eq!(out.len(), 3);
        assert!(matches!(
            out[1],
            InputAction::Event(PlayerEvent::TextControl {
                code: TextControlCode::Enter
            })
        ));
        assert!(matches!(
            out[2],
            InputAction::Event(PlayerEvent::TextInput { codepoint: 'ب' })
        ));
    }

    #[test]
    fn cursors_map() {
        assert_eq!(
            cursor_icon(MouseCursor::Hand),
            egui::CursorIcon::PointingHand
        );
        assert_eq!(cursor_icon(MouseCursor::IBeam), egui::CursorIcon::Text);
    }
}
