//! 2D-слой интерфейса поверх собственного холста движка: виртуальное разрешение
//! (не меньше 1280×720), чёткий текст, скруглённые фигуры, неоновые панели и кнопки.

use std::cell::{Cell, RefCell};
use std::f32::consts::PI;

use crate::engine::canvas::Canvas;
use crate::engine::prelude::*;
use strawberry_poker::cards::Card;

pub const W: f32 = 1280.0;
pub const H: f32 = 720.0;

pub const K_GOLD: Color = Color::new(0.98, 0.80, 0.33, 1.0);
pub const K_TEXT: Color = Color::new(0.95, 0.96, 1.0, 1.0);
pub const K_MUTED: Color = Color::new(0.66, 0.70, 0.86, 1.0);
pub const K_PANEL: Color = Color::new(0.06, 0.07, 0.16, 0.78);
pub const K_RED: Color = Color::new(0.95, 0.30, 0.38, 1.0);
pub const K_ORANGE: Color = Color::new(0.95, 0.55, 0.12, 1.0);
/// неоновые акценты из референса
pub const K_NEON: Color = Color::new(0.68, 0.33, 1.0, 1.0);
pub const K_CYAN: Color = Color::new(0.30, 0.85, 1.0, 1.0);

pub fn rgb(hex: u32) -> Color {
    Color::from_rgba((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255)
}

pub fn alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

pub fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

pub fn lighten(c: Color, t: f32) -> Color {
    mix(c, Color::new(1.0, 1.0, 1.0, c.a), t)
}

pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub use strawberry_poker::fmt_chips;

#[derive(Clone, Copy, PartialEq)]
pub enum Align {
    Left,
    Center,
    Right,
}

pub struct G<'a> {
    pub s: f32,
    pub ox: f32,
    pub oy: f32,
    /// Размер виртуального экрана (не меньше 1280×720, растягивается под пропорции)
    pub w: f32,
    pub h: f32,
    canvas: &'a RefCell<Canvas>,
    click: Cell<bool>,
}

impl<'a> G<'a> {
    pub fn new(canvas: &'a RefCell<Canvas>) -> Self {
        let sw = screen_width();
        let sh = screen_height();
        let s = (sw / W).min(sh / H);
        G {
            s,
            ox: 0.0,
            oy: 0.0,
            w: sw / s,
            h: sh / s,
            canvas,
            click: Cell::new(is_mouse_button_pressed(MouseButton::Left)),
        }
    }

    /// Для рисования в текстуру: 1 единица = 1 пиксель.
    pub fn fixed(canvas: &'a RefCell<Canvas>, w: f32, h: f32) -> Self {
        G { s: 1.0, ox: 0.0, oy: 0.0, w, h, canvas, click: Cell::new(false) }
    }

    /// Дальше рисуем поверх 3D-сцены.
    pub fn ui(&self) {
        self.canvas.borrow_mut().layer = 1;
    }

    #[inline]
    pub fn tx(&self, x: f32) -> f32 {
        self.ox + x * self.s
    }
    #[inline]
    pub fn ty(&self, y: f32) -> f32 {
        self.oy + y * self.s
    }
    fn p(&self, x: f32, y: f32) -> Vec2 {
        vec2(self.tx(x), self.ty(y))
    }

    pub fn mouse(&self) -> (f32, f32) {
        let (mx, my) = mouse_position();
        ((mx - self.ox) / self.s, (my - self.oy) / self.s)
    }

    pub fn hover(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        let (mx, my) = self.mouse();
        mx >= x && mx <= x + w && my >= y && my <= y + h
    }

    /// Возвращает true один раз за нажатие, если клик пришёлся в область.
    pub fn clicked(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        if self.click.get() && self.hover(x, y, w, h) {
            self.click.set(false);
            true
        } else {
            false
        }
    }

    /// Поглотить клик (например, модальное окно).
    pub fn eat_click(&self) {
        self.click.set(false);
    }

    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32, c: Color) {
        self.canvas.borrow_mut().rect(self.tx(x), self.ty(y), w * self.s, h * self.s, c);
    }

    fn fan(&self, cx: f32, cy: f32, pts: &[Vec2], c: Color) {
        let pts: Vec<Vec2> = pts.iter().map(|p| self.p(p.x, p.y)).collect();
        self.canvas.borrow_mut().fan(self.p(cx, cy), &pts, c);
    }

    fn outline(&self, pts: &[Vec2], th: f32, c: Color) {
        // кольцо постоянной толщины: смещаем вершины по биссектрисе
        let n = pts.len();
        let mut inner = Vec::with_capacity(n);
        let mut outer = Vec::with_capacity(n);
        for i in 0..n {
            let prev = pts[(i + n - 1) % n];
            let next = pts[(i + 1) % n];
            let d = next - prev;
            let l = d.length().max(1e-6);
            let nrm = vec2(d.y / l, -d.x / l) * (th / 2.0);
            inner.push(self.p(pts[i].x - nrm.x, pts[i].y - nrm.y));
            outer.push(self.p(pts[i].x + nrm.x, pts[i].y + nrm.y));
        }
        self.canvas.borrow_mut().strip(&inner, &outer, true, c);
    }

    fn rrect_pts(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<Vec2> {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let seg = 7;
        let corners = [
            (x + w - r, y + h - r, 0.0),
            (x + r, y + h - r, PI / 2.0),
            (x + r, y + r, PI),
            (x + w - r, y + r, PI * 1.5),
        ];
        let mut pts = Vec::with_capacity(4 * (seg + 1));
        for (cx, cy, a0) in corners {
            for i in 0..=seg {
                let a = a0 + (PI / 2.0) * i as f32 / seg as f32;
                pts.push(vec2(cx + r * a.cos(), cy + r * a.sin()));
            }
        }
        pts
    }

    pub fn rrect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color) {
        let pts = Self::rrect_pts(x, y, w, h, r);
        self.fan(x + w / 2.0, y + h / 2.0, &pts, c);
    }

    pub fn rrect_lines(&self, x: f32, y: f32, w: f32, h: f32, r: f32, th: f32, c: Color) {
        let pts = Self::rrect_pts(x, y, w, h, r);
        self.outline(&pts, th, c);
    }

    fn ellipse_pts(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<Vec2> {
        let n = 96;
        (0..n)
            .map(|i| {
                let a = 2.0 * PI * i as f32 / n as f32;
                vec2(cx + rx * a.cos(), cy + ry * a.sin())
            })
            .collect()
    }

    pub fn ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32, c: Color) {
        self.fan(cx, cy, &Self::ellipse_pts(cx, cy, rx, ry), c);
    }

    pub fn circle(&self, x: f32, y: f32, r: f32, c: Color) {
        let n = ((r * self.s) as usize).clamp(16, 64);
        let pts: Vec<Vec2> = (0..n)
            .map(|i| {
                let a = 2.0 * PI * i as f32 / n as f32;
                vec2(x + r * a.cos(), y + r * a.sin())
            })
            .collect();
        self.fan(x, y, &pts, c);
    }

    pub fn ring(&self, x: f32, y: f32, r: f32, th: f32, c: Color) {
        self.arc(x, y, r, th, -90.0, 360.0, c);
    }

    /// Дуга: начало в градусах (0 = вправо), длина в градусах; r — внешний радиус.
    pub fn arc(&self, x: f32, y: f32, r: f32, th: f32, start: f32, len: f32, c: Color) {
        if len <= 0.0 {
            return;
        }
        let n = ((len / 360.0 * 64.0) as usize).max(2);
        let (mut inner, mut outer) = (Vec::new(), Vec::new());
        for i in 0..=n {
            let a = (start + len * i as f32 / n as f32).to_radians();
            inner.push(self.p(x + (r - th) * a.cos(), y + (r - th) * a.sin()));
            outer.push(self.p(x + r * a.cos(), y + r * a.sin()));
        }
        self.canvas.borrow_mut().strip(&inner, &outer, false, c);
    }

    pub fn tri(&self, a: Vec2, b: Vec2, c: Vec2, col: Color) {
        self.canvas.borrow_mut().tri(self.p(a.x, a.y), self.p(b.x, b.y), self.p(c.x, c.y), col);
    }

    pub fn text_w(&self, t: &str, size: f32, bold: bool) -> f32 {
        let fs = (size * self.s).round().max(1.0) as u16;
        self.canvas.borrow_mut().measure(bold as usize, t, fs) / self.s
    }

    /// Текст, вертикально центрированный по `y`.
    pub fn text(&self, t: &str, x: f32, y: f32, size: f32, c: Color, align: Align, bold: bool) {
        let fs = (size * self.s).round().max(1.0) as u16;
        let mut cv = self.canvas.borrow_mut();
        let w = cv.measure(bold as usize, t, fs);
        let px = match align {
            Align::Left => self.tx(x),
            Align::Center => self.tx(x) - w / 2.0,
            Align::Right => self.tx(x) - w,
        };
        let py = self.ty(y) + fs as f32 * 0.36;
        cv.text(bold as usize, t, px.round(), py.round(), fs, c);
    }

    pub fn text_shadow(&self, t: &str, x: f32, y: f32, size: f32, c: Color, align: Align, bold: bool) {
        self.text(t, x + 1.5, y + 2.0, size, alpha(BLACK, 0.6 * c.a), align, bold);
        self.text(t, x, y, size, c, align, bold);
    }

    /// Неоновое свечение вокруг скруглённого прямоугольника.
    pub fn glow(&self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Color, strength: f32) {
        for i in (1..=5).rev() {
            let e = i as f32 * 3.0;
            self.rrect_lines(x - e / 2.0, y - e / 2.0, w + e, h + e, r + e / 2.0, 3.0, alpha(c, strength * 0.07 * (6 - i) as f32));
        }
        self.rrect_lines(x, y, w, h, r, 2.0, alpha(lighten(c, 0.35), strength));
    }

    /// Голографическая стеклянная панель (как таблички игроков на референсе).
    pub fn glass(&self, x: f32, y: f32, w: f32, h: f32, r: f32, accent: Color, active: bool) {
        self.rrect(x, y, w, h, r, Color::new(0.07, 0.09, 0.22, 0.62));
        // блик на стекле
        self.rrect(x + 3.0, y + 3.0, w - 6.0, h * 0.42, r * 0.8, alpha(WHITE, 0.07));
        self.glow(x, y, w, h, r, accent, if active { 1.0 } else { 0.55 });
    }

    /// Неоновая кнопка. Возвращает true при нажатии.
    pub fn button(&self, x: f32, y: f32, w: f32, h: f32, label: &str, base: Color, enabled: bool) -> bool {
        let hover = enabled && self.hover(x, y, w, h);
        let pressed = hover && is_mouse_button_down(MouseButton::Left);
        let accent = if !enabled { mix(base, rgb(0x3a3f55), 0.7) } else if hover { lighten(base, 0.2) } else { base };
        let dy = if pressed { 2.0 } else { 0.0 };
        let r = (h * 0.28).min(14.0);
        let fill = if pressed { 0.42 } else if hover { 0.32 } else { 0.22 };
        self.rrect(x, y + dy, w, h, r, Color::new(accent.r * 0.35, accent.g * 0.3, accent.b * 0.5, 0.55 + fill * 0.5));
        self.rrect(x + 3.0, y + dy + 3.0, w - 6.0, h * 0.4, r * 0.8, alpha(WHITE, 0.06));
        self.glow(x, y + dy, w, h, r, accent, if enabled { 0.95 } else { 0.35 });
        let tc = if enabled { lighten(accent, 0.75) } else { alpha(WHITE, 0.4) };
        let mut size = (h * 0.36).min(22.0);
        while self.text_w(label, size, true) > w - 12.0 && size > 9.0 {
            size -= 1.0;
        }
        self.text_shadow(label, x + w / 2.0, y + dy + h / 2.0, size, tc, Align::Center, true);
        enabled && self.clicked(x, y, w, h)
    }

    // ---------- игровые элементы ----------

    pub fn strawberry(&self, cx: f32, cy: f32, size: f32, a: f32) {
        let red = alpha(rgb(0xe8283f), a);
        let s = size;
        self.circle(cx - s * 0.2, cy - s * 0.05, s * 0.32, red);
        self.circle(cx + s * 0.2, cy - s * 0.05, s * 0.32, red);
        self.tri(
            vec2(cx - s * 0.5, cy - s * 0.02),
            vec2(cx + s * 0.5, cy - s * 0.02),
            vec2(cx, cy + s * 0.55),
            red,
        );
        self.circle(cx, cy + s * 0.18, s * 0.3, red);
        let seed = alpha(rgb(0xffe08a), a);
        for (dx, dy) in [(-0.22, 0.0), (0.0, -0.05), (0.22, 0.0), (-0.12, 0.2), (0.12, 0.2), (0.0, 0.38)] {
            self.circle(cx + dx * s, cy + dy * s, s * 0.035, seed);
        }
        let leaf = alpha(rgb(0x3fb35a), a);
        for k in 0..5 {
            let ang = PI + k as f32 * PI / 4.0;
            let tip = vec2(cx + ang.cos() * s * 0.38, cy - s * 0.3 + ang.sin() * s * 0.22);
            self.tri(vec2(cx - s * 0.08, cy - s * 0.3), vec2(cx + s * 0.08, cy - s * 0.3), tip, leaf);
        }
        self.rect(cx - s * 0.03, cy - s * 0.55, s * 0.06, s * 0.25, leaf);
    }

    pub fn card_face(&self, x: f32, y: f32, w: f32, h: f32, card: Card, dim: bool, hl: bool) {
        let r = w * 0.12;
        self.rrect(x + 2.0, y + 3.0, w, h, r, alpha(BLACK, 0.35));
        if hl {
            self.rrect(x - 3.0, y - 3.0, w + 6.0, h + 6.0, r + 3.0, K_GOLD);
        }
        self.rrect(x, y, w, h, r, rgb(0xfdfdfd));
        self.rrect_lines(x, y, w, h, r, 1.0, rgb(0xc9ccd6));
        let col = if card.suit.is_red() { rgb(0xd7263d) } else { rgb(0x16181f) };
        let rank = card.rank_str();
        self.text(rank, x + w * 0.24, y + h * 0.17, h * 0.25, col, Align::Center, true);
        self.text(card.suit.symbol(), x + w * 0.24, y + h * 0.39, h * 0.2, col, Align::Center, false);
        self.text(card.suit.symbol(), x + w * 0.64, y + h * 0.7, h * 0.46, col, Align::Center, false);
        if dim {
            self.rrect(x, y, w, h, r, alpha(rgb(0x0a0d18), 0.5));
        }
    }

    /// Лицо карты на всю площадь (для текстуры).
    pub fn card_face_flat(&self, x: f32, y: f32, w: f32, h: f32, card: Card) {
        let r = w * 0.1;
        self.rrect(x, y, w, h, r, rgb(0xfdfdfd));
        self.rrect_lines(x + 1.0, y + 1.0, w - 2.0, h - 2.0, r, 2.0, rgb(0xc9ccd6));
        let col = if card.suit.is_red() { rgb(0xd7263d) } else { rgb(0x16181f) };
        self.text(card.rank_str(), x + w * 0.25, y + h * 0.16, h * 0.25, col, Align::Center, true);
        self.text(card.suit.symbol(), x + w * 0.25, y + h * 0.37, h * 0.19, col, Align::Center, false);
        self.text(card.suit.symbol(), x + w * 0.62, y + h * 0.69, h * 0.5, col, Align::Center, false);
    }

    pub fn card_back_flat(&self, x: f32, y: f32, w: f32, h: f32) {
        self.card_back_inner(x, y, w, h, false);
    }

    pub fn card_back(&self, x: f32, y: f32, w: f32, h: f32) {
        self.card_back_inner(x, y, w, h, true);
    }

    fn card_back_inner(&self, x: f32, y: f32, w: f32, h: f32, shadow: bool) {
        let r = w * 0.12;
        if shadow {
            self.rrect(x + 2.0, y + 3.0, w, h, r, alpha(BLACK, 0.35));
        }
        self.rrect(x, y, w, h, r, rgb(0xfdfdfd));
        let m = w * 0.08;
        self.rrect(x + m, y + m, w - 2.0 * m, h - 2.0 * m, r * 0.7, rgb(0xa3122b));
        // ромбики
        let step = w * 0.22;
        let mut yy = y + m + step * 0.5;
        let mut row = 0;
        while yy < y + h - m - step * 0.3 {
            let mut xx = x + m + step * if row % 2 == 0 { 0.5 } else { 1.0 };
            while xx < x + w - m - step * 0.3 {
                let d = step * 0.22;
                let c = alpha(rgb(0xd03552), 0.9);
                self.tri(vec2(xx - d, yy), vec2(xx, yy - d), vec2(xx + d, yy), c);
                self.tri(vec2(xx - d, yy), vec2(xx, yy + d), vec2(xx + d, yy), c);
                xx += step;
            }
            yy += step * 0.6;
            row += 1;
        }
        self.circle(x + w / 2.0, y + h / 2.0, w * 0.26, rgb(0xfdfdfd));
        self.strawberry(x + w / 2.0, y + h / 2.0 + w * 0.02, w * 0.36, 1.0);
    }

    /// Карта с анимацией переворота: flip 0 → рубашка, 1 → лицо.
    pub fn card_flip(&self, x: f32, y: f32, w: f32, h: f32, card: Card, flip: f32, dim: bool, hl: bool) {
        if flip >= 1.0 {
            self.card_face(x, y, w, h, card, dim, hl);
            return;
        }
        let k = (flip * 2.0 - 1.0).abs().max(0.05);
        let ww = w * k;
        let xx = x + (w - ww) / 2.0;
        if flip < 0.5 {
            self.card_back(xx, y, ww, h);
        } else {
            self.card_face(xx, y, ww, h, card, dim, hl);
        }
    }
}
