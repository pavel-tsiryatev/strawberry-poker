//! Ввод и время: состояние мыши/касаний/клавиатуры за кадр.

use std::cell::RefCell;

pub use miniquad::{KeyCode, MouseButton};

#[derive(Default)]
struct State {
    mouse: (f32, f32),
    down: bool,
    pressed: bool,
    released: bool,
    wheel: f32,
    chars: Vec<char>,
    keys: Vec<KeyCode>,
    start: f64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> R {
    STATE.with(|s| f(&mut s.borrow_mut()))
}

pub fn init() {
    with(|s| s.start = miniquad::date::now());
}

/// Секунды с запуска.
pub fn get_time() -> f64 {
    miniquad::date::now() - with(|s| s.start)
}

pub fn screen_width() -> f32 {
    miniquad::window::screen_size().0
}

pub fn screen_height() -> f32 {
    miniquad::window::screen_size().1
}

pub fn mouse_position() -> (f32, f32) {
    with(|s| s.mouse)
}

pub fn is_mouse_button_down(_b: MouseButton) -> bool {
    with(|s| s.down)
}

pub fn is_mouse_button_pressed(_b: MouseButton) -> bool {
    with(|s| s.pressed)
}

pub fn is_mouse_button_released(_b: MouseButton) -> bool {
    with(|s| s.released)
}

pub fn mouse_wheel() -> (f32, f32) {
    (0.0, with(|s| s.wheel))
}

pub fn get_char_pressed() -> Option<char> {
    with(|s| if s.chars.is_empty() { None } else { Some(s.chars.remove(0)) })
}

pub fn is_key_pressed(k: KeyCode) -> bool {
    with(|s| s.keys.contains(&k))
}

// ---- события от окна

pub fn on_move(x: f32, y: f32) {
    with(|s| s.mouse = (x, y));
}

pub fn on_down(x: f32, y: f32) {
    with(|s| {
        s.mouse = (x, y);
        s.down = true;
        s.pressed = true;
    });
}

pub fn on_up(x: f32, y: f32) {
    with(|s| {
        s.mouse = (x, y);
        s.down = false;
        s.released = true;
    });
}

pub fn on_wheel(dy: f32) {
    with(|s| s.wheel += dy);
}

pub fn on_char(c: char) {
    with(|s| s.chars.push(c));
}

pub fn on_key(k: KeyCode) {
    with(|s| s.keys.push(k));
}

/// Сбросить «одноразовые» события в конце кадра.
pub fn end_frame() {
    with(|s| {
        s.pressed = false;
        s.released = false;
        s.wheel = 0.0;
        s.keys.clear();
    });
}
