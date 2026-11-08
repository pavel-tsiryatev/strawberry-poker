//! Обёртка над macroquad: виртуальное разрешение 1280×720 с масштабированием,
//! чёткий текст, скруглённые прямоугольники, карты и фишки.

use std::cell::Cell;
use std::f32::consts::PI;

use macroquad::prelude::*;
use strawberry_poker::cards::Card;

pub const W: f32 = 1280.0;
pub const H: f32 = 720.0;

pub const K_GOLD: Color = Color::new(0.98, 0.80, 0.33, 1.0);
pub const K_GOLD_DARK: Color = Color::new(0.62, 0.45, 0.13, 1.0);
pub const K_TEXT: Color = Color::new(0.95, 0.96, 1.0, 1.0);
pub const K_MUTED: Color = Color::new(0.62, 0.67, 0.80, 1.0);
pub const K_PANEL: Color = Color::new(0.07, 0.09, 0.17, 0.92);
pub const K_RED: Color = Color::new(0.86, 0.20, 0.24, 1.0);
pub const K_BLUE: Color = Color::new(0.20, 0.47, 0.90, 1.0);
pub const K_GREEN: Color = Color::new(0.16, 0.66, 0.38, 1.0);
pub const K_ORANGE: Color = Color::new(0.95, 0.55, 0.12, 1.0);

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

pub fn darken(c: Color, t: f32) -> Color {
    mix(c, Color::new(0.0, 0.0, 0.0, c.a), t)
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
    font: &'a Font,
    bold: &'a Font,
    click: Cell<bool>,
}

impl<'a> G<'a> {
    pub fn new(font: &'a Font, bold: &'a Font) -> Self {
        let sw = screen_width();
        let sh = screen_height();
        let s = (sw / W).min(sh / H);
        G {
            s,
            ox: 0.0,
            oy: 0.0,
            w: sw / s,
            h: sh / s,
            font,
            bold,
            click: Cell::new(is_mouse_button_pressed(MouseButton::Left)),
        }
    }

    /// Для рисования в текстуру: 1 единица = 1 пиксель.
    pub fn fixed(font: &'a Font, bold: &'a Font, w: f32, h: f32) -> Self {
        G { s: 1.0, ox: 0.0, oy: 0.0, w, h, font, bold, click: Cell::new(false) }
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
        draw_rectangle(self.tx(x), self.ty(y), w * self.s, h * self.s, c);
    }

    pub fn vgradient(&self, x: f32, y: f32, w: f32, h: f32, top: Color, bottom: Color) {
        let bands = 48;
        let bh = h / bands as f32;
        for i in 0..bands {
            let t = i as f32 / (bands - 1) as f32;
            self.rect(x, y + i as f32 * bh, w, bh + 0.6, mix(top, bottom, t));
        }
    }

    fn fan(&self, cx: f32, cy: f32, pts: &[Vec2], c: Color) {
        let center = self.p(cx, cy);
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            draw_triangle(center, self.p(a.x, a.y), self.p(b.x, b.y), c);
        }
    }

    fn outline(&self, pts: &[Vec2], th: f32, c: Color) {
        for i in 0..pts.len() {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            draw_line(self.tx(a.x), self.ty(a.y), self.tx(b.x), self.ty(b.y), th * self.s, c);
        }
        for p in pts {
            draw_circle(self.tx(p.x), self.ty(p.y), th * self.s * 0.5, c);
        }
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
        let sides = ((r * self.s) as u8).clamp(16, 64);
        draw_poly(self.tx(x), self.ty(y), sides, r * self.s, 0.0, c);
    }

    pub fn ring(&self, x: f32, y: f32, r: f32, th: f32, c: Color) {
        self.arc(x, y, r, th, -90.0, 360.0, c);
    }

    /// Дуга: начало в градусах (0 = вправо), длина в градусах.
    pub fn arc(&self, x: f32, y: f32, r: f32, th: f32, start: f32, len: f32, c: Color) {
        if len <= 0.0 {
            return;
        }
        draw_arc(
            self.tx(x),
            self.ty(y),
            64,
            (r - th / 2.0) * self.s,
            start,
            th * self.s,
            len,
            c,
        );
    }


    pub fn tri(&self, a: Vec2, b: Vec2, c: Vec2, col: Color) {
        draw_triangle(self.p(a.x, a.y), self.p(b.x, b.y), self.p(c.x, c.y), col);
    }

    fn font_for(&self, bold: bool) -> &Font {
        if bold { self.bold } else { self.font }
    }

    pub fn text_w(&self, t: &str, size: f32, bold: bool) -> f32 {
        let fs = (size * self.s).round().max(1.0) as u16;
        measure_text(t, Some(self.font_for(bold)), fs, 1.0).width / self.s
    }

    /// Текст, вертикально центрированный по `y`.
    pub fn text(&self, t: &str, x: f32, y: f32, size: f32, c: Color, align: Align, bold: bool) {
        let fs = (size * self.s).round().max(1.0) as u16;
        let font = self.font_for(bold);
        let w = measure_text(t, Some(font), fs, 1.0).width;
        let px = match align {
            Align::Left => self.tx(x),
            Align::Center => self.tx(x) - w / 2.0,
            Align::Right => self.tx(x) - w,
        };
        let py = self.ty(y) + fs as f32 * 0.36;
        draw_text_ex(
            t,
            px.round(),
            py.round(),
            TextParams {
                font: Some(font),
                font_size: fs,
                color: c,
                ..Default::default()
            },
        );
    }

    pub fn text_shadow(&self, t: &str, x: f32, y: f32, size: f32, c: Color, align: Align, bold: bool) {
        self.text(t, x + 1.5, y + 2.0, size, alpha(BLACK, 0.6 * c.a), align, bold);
        self.text(t, x, y, size, c, align, bold);
    }

    /// Глянцевая кнопка. Возвращает true при нажатии.
    pub fn button(&self, x: f32, y: f32, w: f32, h: f32, label: &str, base: Color, enabled: bool) -> bool {
        let hover = enabled && self.hover(x, y, w, h);
        let pressed = hover && is_mouse_button_down(MouseButton::Left);
        let c = if !enabled {
            mix(base, rgb(0x3a3f55), 0.75)
        } else if hover {
            lighten(base, 0.12)
        } else {
            base
        };
        let dy = if pressed { 2.0 } else { 0.0 };
        let r = (h * 0.32).min(16.0);
        self.rrect(x, y + 4.0, w, h, r, darken(c, 0.55));
        self.rrect(x, y + dy, w, h, r, c);
        self.rrect(x + 3.0, y + dy + 2.0, w - 6.0, h * 0.46, r * 0.8, alpha(WHITE, 0.13));
        self.rrect_lines(x, y + dy, w, h, r, 1.5, alpha(lighten(c, 0.35), 0.8));
        let tc = if enabled { WHITE } else { alpha(WHITE, 0.4) };
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
