//! 2D-холст: собирает треугольники интерфейса в пакеты и ведёт атлас глифов.
//! Глифы растеризует fontdue; упаковка в атлас, кэш и раскладка текста — свои.

use std::collections::HashMap;

use super::math::*;

pub const ATLAS: usize = 2048;
const MAX_VERTS: usize = 60_000;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// Пакет треугольников, разбитый на куски по ≤ 60 000 вершин (индексы u16).
#[derive(Default)]
pub struct Batch {
    pub chunks: Vec<(Vec<UiVertex>, Vec<u16>)>,
}

impl Batch {
    pub fn clear(&mut self) {
        self.chunks.clear();
    }

    /// Возвращает кусок, в котором есть место под `n` вершин.
    fn room(&mut self, n: usize) -> &mut (Vec<UiVertex>, Vec<u16>) {
        if self.chunks.last().map(|c| c.0.len() + n > MAX_VERTS).unwrap_or(true) {
            self.chunks.push((Vec::with_capacity(4096), Vec::with_capacity(8192)));
        }
        self.chunks.last_mut().unwrap()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.iter().all(|c| c.1.is_empty())
    }
}

#[derive(Clone, Copy)]
struct Glyph {
    /// положение в атласе (пиксели)
    ax: u16,
    ay: u16,
    w: u16,
    h: u16,
    xmin: i16,
    ymin: i16,
    advance: f32,
}

/// Загрузка куска атласа на GPU (выполняет рендерер).
pub struct Upload {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub rgba: Vec<u8>,
}

pub struct Canvas {
    fonts: Vec<fontdue::Font>,
    glyphs: HashMap<(u8, char, u16), Glyph>,
    // упаковщик «полками»
    shelf_x: usize,
    shelf_y: usize,
    shelf_h: usize,
    pub uploads: Vec<Upload>,
    pub layers: Vec<Batch>,
    pub layer: usize,
}

/// uv белого квадрата в углу атласа — для залитых фигур.
const WHITE_UV: [f32; 2] = [2.0 / ATLAS as f32, 2.0 / ATLAS as f32];

impl Canvas {
    pub fn new(font_files: &[&[u8]], layers: usize) -> Canvas {
        let fonts = font_files
            .iter()
            .map(|b| fontdue::Font::from_bytes(*b, fontdue::FontSettings::default()).expect("шрифт"))
            .collect();
        let mut c = Canvas {
            fonts,
            glyphs: HashMap::new(),
            shelf_x: 0,
            shelf_y: 0,
            shelf_h: 0,
            uploads: Vec::new(),
            layers: (0..layers).map(|_| Batch::default()).collect(),
            layer: 0,
        };
        c.reset_atlas();
        c
    }

    fn reset_atlas(&mut self) {
        self.glyphs.clear();
        self.shelf_x = 8;
        self.shelf_y = 0;
        self.shelf_h = 8;
        self.uploads.push(Upload { x: 0, y: 0, w: 8, h: 8, rgba: vec![255; 8 * 8 * 4] });
    }

    pub fn begin(&mut self) {
        for l in &mut self.layers {
            l.clear();
        }
        self.layer = 0;
    }

    // ---------------------------------------------------------- геометрия

    pub fn tri(&mut self, a: Vec2, b: Vec2, c: Vec2, col: Color) {
        let color = col.to_array();
        let (verts, idx) = self.layers[self.layer].room(3);
        let base = verts.len() as u16;
        for p in [a, b, c] {
            verts.push(UiVertex { pos: [p.x, p.y], uv: WHITE_UV, color });
        }
        idx.extend_from_slice(&[base, base + 1, base + 2]);
    }

    /// Выпуклый многоугольник веером из центра.
    pub fn fan(&mut self, center: Vec2, pts: &[Vec2], col: Color) {
        let color = col.to_array();
        let (verts, idx) = self.layers[self.layer].room(pts.len() + 1);
        let base = verts.len() as u16;
        verts.push(UiVertex { pos: [center.x, center.y], uv: WHITE_UV, color });
        for p in pts {
            verts.push(UiVertex { pos: [p.x, p.y], uv: WHITE_UV, color });
        }
        let k = pts.len() as u16;
        for i in 0..k {
            idx.extend_from_slice(&[base, base + 1 + i, base + 1 + (i + 1) % k]);
        }
    }

    /// Полоса (кольцо/дуга): пары внутренних и внешних точек.
    pub fn strip(&mut self, inner: &[Vec2], outer: &[Vec2], closed: bool, col: Color) {
        let color = col.to_array();
        let n = inner.len();
        let (verts, idx) = self.layers[self.layer].room(n * 2);
        let base = verts.len() as u16;
        for i in 0..n {
            verts.push(UiVertex { pos: [inner[i].x, inner[i].y], uv: WHITE_UV, color });
            verts.push(UiVertex { pos: [outer[i].x, outer[i].y], uv: WHITE_UV, color });
        }
        let segs = if closed { n } else { n - 1 };
        for i in 0..segs as u16 {
            let a = base + 2 * i;
            let b = base + 2 * ((i + 1) % n as u16);
            idx.extend_from_slice(&[a, a + 1, b + 1, a, b + 1, b]);
        }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, col: Color) {
        self.fan(
            vec2(x + w / 2.0, y + h / 2.0),
            &[vec2(x, y), vec2(x + w, y), vec2(x + w, y + h), vec2(x, y + h)],
            col,
        );
    }

    pub fn line(&mut self, a: Vec2, b: Vec2, th: f32, col: Color) {
        let d = b - a;
        let l = d.length().max(1e-6);
        let n = vec2(-d.y / l, d.x / l) * (th / 2.0);
        self.fan((a + b) * 0.5, &[a + n, b + n, b - n, a - n], col);
    }

    // ---------------------------------------------------------- текст

    fn glyph(&mut self, font: usize, ch: char, px: u16) -> Glyph {
        let key = (font as u8, ch, px);
        if let Some(g) = self.glyphs.get(&key) {
            return *g;
        }
        let (m, bitmap) = self.fonts[font].rasterize(ch, px as f32);
        let (w, h) = (m.width, m.height);
        if self.shelf_x + w + 2 > ATLAS {
            self.shelf_x = 0;
            self.shelf_y += self.shelf_h + 2;
            self.shelf_h = 0;
        }
        if self.shelf_y + h + 2 > ATLAS {
            // атлас заполнен — начинаем заново (редко: разные размеры шрифта)
            self.reset_atlas();
            return self.glyph(font, ch, px);
        }
        let (ax, ay) = (self.shelf_x, self.shelf_y);
        self.shelf_x += w + 2;
        self.shelf_h = self.shelf_h.max(h);
        if w > 0 && h > 0 {
            let mut rgba = Vec::with_capacity(w * h * 4);
            for a in bitmap {
                rgba.extend_from_slice(&[255, 255, 255, a]);
            }
            self.uploads.push(Upload { x: ax as i32, y: ay as i32, w: w as i32, h: h as i32, rgba });
        }
        let g = Glyph {
            ax: ax as u16,
            ay: ay as u16,
            w: w as u16,
            h: h as u16,
            xmin: m.xmin as i16,
            ymin: m.ymin as i16,
            advance: m.advance_width,
        };
        self.glyphs.insert(key, g);
        g
    }

    pub fn measure(&mut self, font: usize, text: &str, px: u16) -> f32 {
        text.chars().map(|ch| self.glyph(font, ch, px).advance).sum()
    }

    /// Текст с базовой линией на `baseline`, начиная с `x`.
    pub fn text(&mut self, font: usize, text: &str, x: f32, baseline: f32, px: u16, col: Color) {
        let color = col.to_array();
        let mut pen = x;
        let inv = 1.0 / ATLAS as f32;
        for ch in text.chars() {
            let g = self.glyph(font, ch, px);
            if g.w > 0 && g.h > 0 {
                let x0 = (pen + g.xmin as f32).round();
                let y0 = (baseline - g.ymin as f32 - g.h as f32).round();
                let (x1, y1) = (x0 + g.w as f32, y0 + g.h as f32);
                let (u0, v0) = (g.ax as f32 * inv, g.ay as f32 * inv);
                let (u1, v1) = ((g.ax + g.w) as f32 * inv, (g.ay + g.h) as f32 * inv);
                let (verts, idx) = self.layers[self.layer].room(4);
                let b = verts.len() as u16;
                verts.push(UiVertex { pos: [x0, y0], uv: [u0, v0], color });
                verts.push(UiVertex { pos: [x1, y0], uv: [u1, v0], color });
                verts.push(UiVertex { pos: [x1, y1], uv: [u1, v1], color });
                verts.push(UiVertex { pos: [x0, y1], uv: [u0, v1], color });
                idx.extend_from_slice(&[b, b + 1, b + 2, b, b + 2, b + 3]);
            }
            pen += g.advance;
        }
    }
}
