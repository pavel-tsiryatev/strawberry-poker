//! Процедурные текстуры: шум, дерево, ковёр, обои, ночной город, тюль.
//! Всё генерируется на CPU при запуске — никаких файлов с картинками.

use super::math::Color;

pub struct Image {
    pub w: usize,
    pub h: usize,
    pub data: Vec<u8>,
}

impl Image {
    pub fn new(w: usize, h: usize) -> Image {
        Image { w, h, data: vec![0; w * h * 4] }
    }

    pub fn set(&mut self, x: usize, y: usize, c: [f32; 4]) {
        let i = (y * self.w + x) * 4;
        for k in 0..4 {
            self.data[i + k] = (c[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }

    /// Заполнить по функции f(u, v) → rgba, где u, v ∈ [0, 1).
    pub fn fill(w: usize, h: usize, f: impl Fn(f32, f32) -> [f32; 4]) -> Image {
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let c = f((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                img.set(x, y, c);
            }
        }
        img
    }
}

// ------------------------------------------------------------------ шум

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0x00ff_ffff) as f32 / 0x00ff_ffff as f32
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Значимый шум с периодом `period` клеток (0 — без периода).
pub fn value_noise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (smooth(x - xi as f32), smooth(y - yi as f32));
    let wrap = |v: i32| if period > 0 { v.rem_euclid(period) } else { v };
    let a = hash(wrap(xi), wrap(yi), seed);
    let b = hash(wrap(xi + 1), wrap(yi), seed);
    let c = hash(wrap(xi), wrap(yi + 1), seed);
    let d = hash(wrap(xi + 1), wrap(yi + 1), seed);
    let ab = a + (b - a) * fx;
    let cd = c + (d - c) * fx;
    ab + (cd - ab) * fy
}

/// Фрактальный шум (сумма октав), результат примерно в [0, 1].
pub fn fbm(x: f32, y: f32, octaves: u32, period: i32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..octaves {
        let p = if period > 0 { period * freq as i32 } else { 0 };
        sum += amp * value_noise(x * freq, y * freq, p, seed.wrapping_add(o * 101));
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn rgb(hex: u32) -> [f32; 3] {
    [((hex >> 16) & 255) as f32 / 255.0, ((hex >> 8) & 255) as f32 / 255.0, (hex & 255) as f32 / 255.0]
}

fn opaque(c: [f32; 3]) -> [f32; 4] {
    [c[0], c[1], c[2], 1.0]
}

fn scale(c: [f32; 3], k: f32) -> [f32; 3] {
    [c[0] * k, c[1] * k, c[2] * k]
}

// ------------------------------------------------------------------ материалы

/// Старое лакированное дерево с волокнами, царапинами и потёртостями.
pub fn worn_wood(size: usize, seed: u32, base: u32) -> Image {
    let dark = scale(rgb(base), 0.55);
    let light = rgb(base);
    let worn = mix(rgb(base), rgb(0xc9a27a), 0.55);
    Image::fill(size, size, |u, v| {
        // волокна вдоль u
        let warp = fbm(u * 4.0, v * 16.0, 4, 4, seed) * 6.0;
        let grain = ((v * 70.0 + warp * 3.0).sin() * 0.5 + 0.5).powf(1.5);
        let fine = fbm(u * 64.0, v * 256.0, 2, 64, seed + 7);
        let mut c = mix(dark, light, grain * 0.6 + fine * 0.4);
        // потёртости лака — светлые пятна
        let w = fbm(u * 6.0, v * 6.0, 4, 6, seed + 13);
        if w > 0.58 {
            c = mix(c, worn, ((w - 0.58) * 6.0).min(0.8));
        }
        // царапины
        let s = value_noise(u * 3.0, v * 90.0, 3, seed + 21);
        if s > 0.93 {
            c = mix(c, worn, 0.45);
        }
        // тёмная грязь в стыках
        let g = fbm(u * 12.0 + 3.0, v * 12.0, 3, 12, seed + 31);
        c = scale(c, 0.8 + 0.3 * g);
        opaque(c)
    })
}

/// Восточный ковёр: кайма, медальон, поле с узорами. `aspect` = ширина/высота.
pub fn carpet(w: usize, h: usize, seed: u32) -> Image {
    let burgundy = rgb(0x4a0c18);
    let red = rgb(0x6b1424);
    let navy = rgb(0x1a1c38);
    let cream = rgb(0xa8967a);
    let gold = rgb(0x8a6428);
    let black = rgb(0x1a0d10);
    let aspect = w as f32 / h as f32;
    Image::fill(w, h, |u, v| {
        // координаты от центра, x растянут по пропорциям
        let x = (u - 0.5) * 2.0 * aspect;
        let y = (v - 0.5) * 2.0;
        let ex = aspect - x.abs();
        let ey = 1.0 - y.abs();
        let edge = ex.min(ey);
        let mut c;
        if edge < 0.06 {
            c = navy;
        } else if edge < 0.08 {
            c = cream;
        } else if edge < 0.2 {
            // кайма с зигзагом и «ромбиками»
            let t = if ex < ey { y } else { x };
            let zig = ((t * 18.0).fract() - 0.5).abs() * 2.0;
            let d = ((edge - 0.08) / 0.12 - 0.5).abs() * 2.0;
            c = if d < zig * 0.8 { gold } else { burgundy };
            if (d - zig * 0.8).abs() < 0.08 {
                c = black;
            }
        } else if edge < 0.22 {
            c = cream;
        } else {
            // поле: медальон и повторяющиеся розетки
            let md = x.abs() / aspect.max(1.0) * 1.3 + y.abs() * 1.3;
            if md < 0.95 {
                let ring = (md * 9.0).fract();
                c = if md < 0.18 {
                    gold
                } else if ring < 0.18 {
                    cream
                } else if (md * 9.0) as i32 % 2 == 0 {
                    navy
                } else {
                    red
                };
                // «лепестки» медальона
                let ang = y.atan2(x);
                if ((ang * 8.0).sin() * 0.5 + 0.5) > 0.82 && md > 0.3 {
                    c = mix(c, gold, 0.6);
                }
            } else {
                let tx = (x * 3.2).rem_euclid(1.0) - 0.5;
                let ty = (y * 3.2).rem_euclid(1.0) - 0.5;
                let r = tx.abs() + ty.abs();
                c = red;
                if r < 0.22 {
                    c = navy;
                }
                if r < 0.1 {
                    c = gold;
                }
                if (r - 0.3).abs() < 0.03 {
                    c = black;
                }
                let flower = ((ty.atan2(tx) * 6.0).cos() * 0.5 + 0.5) * 0.18 + 0.08;
                if (tx * tx + ty * ty).sqrt() < flower && r > 0.22 {
                    c = mix(c, cream, 0.7);
                }
            }
        }
        // ворс и потёртость
        let pile = fbm(u * 300.0, v * 300.0 / aspect, 2, 0, seed);
        let wear = fbm(u * 5.0, v * 5.0, 4, 0, seed + 3);
        c = scale(c, 0.78 + 0.32 * pile);
        c = mix(c, rgb(0x8a6a60), ((wear - 0.6) * 1.5).clamp(0.0, 0.35));
        opaque(c)
    })
}

/// Выцветшие советские обои: полосы, мелкий цветочный узор, пятна.
pub fn wallpaper(size: usize, seed: u32) -> Image {
    let base = rgb(0x8f7a5f);
    let stripe = rgb(0x7a654c);
    let flower = rgb(0xa8936f);
    Image::fill(size, size, |u, v| {
        let mut c = base;
        let sx = (u * 8.0).fract();
        if (0.46..0.54).contains(&sx) {
            c = stripe;
        }
        // цветочки в шахматном порядке
        let (cx, cy) = ((u * 8.0).floor(), (v * 6.0).floor());
        let off = if (cx as i32 + cy as i32) % 2 == 0 { 0.25 } else { 0.75 };
        let fx = sx - off;
        let fy = (v * 6.0).fract() - 0.5;
        let r = (fx * fx * 4.0 + fy * fy).sqrt();
        let petals = ((fy.atan2(fx) * 5.0).cos() * 0.5 + 0.5) * 0.12 + 0.08;
        if r < petals {
            c = flower;
        }
        // пятна, потёки и грязь
        let stain = fbm(u * 3.0, v * 3.0, 5, 3, seed);
        c = scale(c, 0.65 + 0.5 * stain);
        let drip = fbm(u * 40.0, v * 2.0, 3, 40, seed + 5);
        c = scale(c, 0.9 + 0.12 * drip);
        opaque(c)
    })
}

/// Вид из окна ночью: небо, панельные дома, горящие окна.
pub fn night_city(w: usize, h: usize, seed: u32) -> Image {
    let sky_top = rgb(0x0b0f24);
    let sky_low = rgb(0x2a1f45);
    let wall = rgb(0x15172a);
    Image::fill(w, h, |u, v| {
        let mut c = mix(sky_top, sky_low, v);
        // три панельки на разной глубине
        let buildings = [(0.05, 0.62, 0.22, 0.85), (0.3, 0.95, 0.15, 1.0), (0.0, 0.3, 0.45, 0.6)];
        for (k, (x0, x1, top, shade)) in buildings.iter().enumerate() {
            if u >= *x0 && u <= *x1 && v >= *top {
                c = scale(wall, *shade);
                let cols = 12.0 + k as f32 * 4.0;
                let rows = 14.0;
                let lx = (u - x0) / (x1 - x0) * cols;
                let ly = (v - top) / (1.0 - top) * rows;
                let (fx, fy) = (lx.fract(), ly.fract());
                if (0.25..0.75).contains(&fx) && (0.3..0.75).contains(&fy) {
                    let r = hash(lx as i32, ly as i32, seed + k as u32);
                    c = if r > 0.62 {
                        mix(rgb(0xffc96b), rgb(0xffe2a8), hash(lx as i32, ly as i32, seed + 9))
                    } else if r > 0.58 {
                        rgb(0x6f8cff)
                    } else {
                        scale(wall, 0.6)
                    };
                }
            }
        }
        opaque(c)
    })
}

/// Тюль: светлый узор с прозрачностью.
pub fn lace(size: usize, seed: u32) -> Image {
    Image::fill(size, size, |u, v| {
        let x = (u * 10.0).fract() - 0.5;
        let y = (v * 10.0).fract() - 0.5;
        let r = (x * x + y * y).sqrt();
        let petals = (y.atan2(x) * 6.0).cos() * 0.5 + 0.5;
        let mut a: f32 = 0.35;
        if r < 0.12 + petals * 0.18 {
            a = 0.75;
        }
        if (r - 0.42).abs() < 0.03 {
            a = 0.7;
        }
        let mesh = ((u * 160.0).fract() < 0.25) as i32 as f32 + ((v * 160.0).fract() < 0.25) as i32 as f32;
        a = (a + mesh * 0.12).min(0.85);
        let dirt = fbm(u * 4.0, v * 4.0, 3, 4, seed);
        let c = mix(rgb(0xe8e2d4), rgb(0xb8ad96), dirt * 0.6);
        [c[0], c[1], c[2], a]
    })
}

/// Затёртый линолеум/паркет.
pub fn floor(size: usize, seed: u32) -> Image {
    let a = rgb(0x4a3022);
    let b = rgb(0x5c3b28);
    Image::fill(size, size, |u, v| {
        // ёлочка из дощечек
        let bx = (u * 8.0).floor();
        let by = (v * 16.0 + bx * 0.5).floor();
        let t = hash(bx as i32, by as i32, seed);
        let mut c = mix(a, b, t);
        let fy = (v * 16.0 + bx * 0.5).fract();
        let fx = (u * 8.0).fract();
        if fy < 0.04 || fx < 0.02 {
            c = scale(c, 0.55);
        }
        let grain = fbm(u * 8.0, v * 64.0, 3, 8, seed + 1);
        c = scale(c, 0.75 + 0.4 * grain);
        opaque(c)
    })
}

/// Экран старого телевизора: голубое свечение и строки развёртки.
pub fn tv_screen(w: usize, h: usize) -> Image {
    Image::fill(w, h, |u, v| {
        let x = u - 0.5;
        let y = v - 0.5;
        let vign = 1.0 - (x * x + y * y) * 1.8;
        let mut c = mix(rgb(0x9fb4d8), rgb(0x3b4f78), v);
        // силуэт ракеты
        let rx = (u - 0.55).abs();
        if v > 0.2 && v < 0.85 && rx < 0.03 + (v - 0.2) * 0.02 {
            c = rgb(0xe8eefc);
        }
        if v > 0.85 {
            c = mix(c, rgb(0xffffff), 0.3);
        }
        let scan = if ((v * h as f32) as i32) % 3 == 0 { 0.8 } else { 1.0 };
        let k = vign.max(0.2) * scan;
        opaque(scale(c, k))
    })
}

/// Эмаль старого холодильника.
pub fn enamel(size: usize, seed: u32) -> Image {
    Image::fill(size, size, |u, v| {
        let n = fbm(u * 6.0, v * 6.0, 4, 6, seed);
        let mut c = mix(rgb(0xe9e4d6), rgb(0xc7bfa8), n);
        let chip = fbm(u * 30.0, v * 30.0, 2, 30, seed + 4);
        if chip > 0.78 {
            c = rgb(0x5a5246);
        }
        opaque(c)
    })
}

pub fn color_of(c: [f32; 3]) -> Color {
    Color::new(c[0], c[1], c[2], 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_is_bounded_and_periodic() {
        for i in 0..200 {
            let x = i as f32 * 0.37;
            let n = fbm(x, x * 0.5, 4, 0, 1);
            assert!((0.0..=1.0).contains(&n));
        }
        // с периодом 4 значения на x и x+4 совпадают
        let a = value_noise(0.3, 1.7, 4, 9);
        let b = value_noise(4.3, 1.7, 4, 9);
        assert!((a - b).abs() < 1e-6);
    }

    #[test]
    fn textures_have_right_size() {
        let img = carpet(64, 40, 1);
        assert_eq!(img.data.len(), 64 * 40 * 4);
        let l = lace(32, 1);
        assert!(l.data.chunks(4).all(|p| p[3] < 255));
    }
}
