//! 3D-сцена: стол, карты, фишки, кнопка дилера.
//! Освещение «запекается» в цвета вершин (простая модель Ламберта + блик),
//! поэтому работает на любом GPU, включая слабые телефоны.

use std::f32::consts::PI;

use macroquad::models::{Mesh, Vertex};
use macroquad::prelude::*;
use strawberry_poker::proto::code_card;

use crate::gfx::{G, rgb};

pub const RX: f32 = 6.0;
pub const RZ: f32 = 3.15;

const CARD_TW: u32 = 200;
const CARD_TH: u32 = 280;

fn light_dir() -> Vec3 {
    vec3(-0.25, 1.0, 0.45).normalize()
}

fn view_dir() -> Vec3 {
    vec3(0.0, 0.75, 0.66).normalize()
}

/// Освещение точки: цвет * (фон + диффуз) + блик + «прожектор» над столом.
fn shade(base: Color, n: Vec3, p: Vec3, shininess: f32, spec: f32) -> [u8; 4] {
    let n = n.normalize_or_zero();
    let l = light_dir();
    let diff = n.dot(l).max(0.0);
    let h = (l + view_dir()).normalize();
    let sp = if spec > 0.0 { n.dot(h).max(0.0).powf(shininess) * spec } else { 0.0 };
    let d = (p.x / (RX * 1.3)).powi(2) + (p.z / (RZ * 1.5)).powi(2);
    let spot = (1.15 - 0.45 * d).clamp(0.45, 1.15);
    let k = (0.38 + 0.72 * diff) * spot;
    let c = |x: f32| ((x * k + sp) * 255.0).clamp(0.0, 255.0) as u8;
    [c(base.r), c(base.g), c(base.b), (base.a * 255.0) as u8]
}

fn vtx(p: Vec3, uv: Vec2, color: [u8; 4]) -> Vertex {
    Vertex { position: p, uv, color, normal: Vec4::ZERO }
}

/// Сетка из колец (u — вдоль контура, v — поперёк), замкнутая по u.
fn grid_mesh(nu: usize, nv: usize, f: impl Fn(f32, f32) -> (Vec3, [u8; 4])) -> Mesh {
    let mut vertices = Vec::with_capacity((nu + 1) * (nv + 1));
    for i in 0..=nu {
        for j in 0..=nv {
            let (p, c) = f(i as f32 / nu as f32, j as f32 / nv as f32);
            vertices.push(vtx(p, Vec2::ZERO, c));
        }
    }
    let mut indices = Vec::new();
    let row = (nv + 1) as u16;
    for i in 0..nu as u16 {
        for j in 0..nv as u16 {
            let a = i * row + j;
            let b = (i + 1) * row + j;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    Mesh { vertices, indices, texture: None }
}

fn ellipse_normal(t: f32, rx: f32, rz: f32) -> Vec3 {
    vec3(t.cos() / rx, 0.0, t.sin() / rz).normalize()
}

pub struct Scene {
    felt: Mesh,
    rail: Mesh,
    piping: Mesh,
    skirt: Mesh,
    line: Mesh,
    chips: Vec<Mesh>,
    dealer: Mesh,
    pub cards: Vec<Texture2D>,
    pub back: Texture2D,
    card_mesh_face: Mesh,
    card_mesh_back: Mesh,
}

pub const CHIP_R: f32 = 0.22;
pub const CHIP_H: f32 = 0.065;

pub fn chip_colors() -> [Color; 6] {
    [rgb(0x1f6fd8), rgb(0xd8323c), rgb(0x2b9b55), rgb(0x2a2a2e), rgb(0x8b3fd1), rgb(0xf2a81d)]
}

fn chip_mesh(base: Color) -> Mesh {
    let n = 32;
    let mut vertices = Vec::new();
    let mut indices: Vec<u16> = Vec::new();
    let white = rgb(0xf4f4f4);
    let stripe = |i: usize| (i / 2) % 4 == 0;
    // боковина
    for i in 0..n {
        let a0 = i as f32 / n as f32 * 2.0 * PI;
        let a1 = (i + 1) as f32 / n as f32 * 2.0 * PI;
        let c = if stripe(i) { white } else { base };
        let base_i = vertices.len() as u16;
        for (a, y) in [(a0, 0.0), (a1, 0.0), (a0, CHIP_H), (a1, CHIP_H)] {
            let nrm = vec3(a.cos(), 0.0, a.sin());
            let p = vec3(a.cos() * CHIP_R, y, a.sin() * CHIP_R);
            vertices.push(vtx(p, Vec2::ZERO, shade(c, nrm, Vec3::ZERO, 20.0, 0.15)));
        }
        indices.extend_from_slice(&[base_i, base_i + 1, base_i + 2, base_i + 2, base_i + 1, base_i + 3]);
    }
    // верх: внешнее кольцо с полосами, внутренний круг
    let up = Vec3::Y;
    let center = vertices.len() as u16;
    vertices.push(vtx(vec3(0.0, CHIP_H, 0.0), Vec2::ZERO, shade(base, up, Vec3::ZERO, 30.0, 0.2)));
    let inner_c = shade(Color::new(base.r * 0.85, base.g * 0.85, base.b * 0.85, 1.0), up, Vec3::ZERO, 30.0, 0.2);
    for i in 0..n {
        let a0 = i as f32 / n as f32 * 2.0 * PI;
        let a1 = (i + 1) as f32 / n as f32 * 2.0 * PI;
        let c = if stripe(i) { white } else { base };
        let oc = shade(c, up, Vec3::ZERO, 30.0, 0.2);
        let b = vertices.len() as u16;
        let r_in = CHIP_R * 0.62;
        let r_ring = CHIP_R * 0.66;
        for (a, r, col) in [
            (a0, r_in, inner_c),
            (a1, r_in, inner_c),
            (a0, r_ring, rgb_u8(white)),
            (a1, r_ring, rgb_u8(white)),
            (a0, CHIP_R, oc),
            (a1, CHIP_R, oc),
        ] {
            vertices.push(vtx(vec3(a.cos() * r, CHIP_H + 0.0005, a.sin() * r), Vec2::ZERO, col));
        }
        // центр
        indices.extend_from_slice(&[center, b, b + 1]);
        // тонкое белое кольцо
        indices.extend_from_slice(&[b, b + 2, b + 1, b + 1, b + 2, b + 3]);
        // внешнее кольцо
        indices.extend_from_slice(&[b + 2, b + 4, b + 3, b + 3, b + 4, b + 5]);
    }
    Mesh { vertices, indices, texture: None }
}

fn rgb_u8(c: Color) -> [u8; 4] {
    [(c.r * 255.0) as u8, (c.g * 255.0) as u8, (c.b * 255.0) as u8, 255]
}

fn card_quad(texture: Texture2D, back: bool) -> Mesh {
    let w = 0.5;
    let h = 0.7;
    let y = if back { -0.004 } else { 0.004 };
    let c = [255, 255, 255, 255];
    // лицо смотрит вверх; рубашка — вниз (видна после переворота)
    let (u0, u1) = if back { (1.0, 0.0) } else { (0.0, 1.0) };
    let vertices = vec![
        vtx(vec3(-w, y, -h), vec2(u0, 0.0), c),
        vtx(vec3(w, y, -h), vec2(u1, 0.0), c),
        vtx(vec3(-w, y, h), vec2(u0, 1.0), c),
        vtx(vec3(w, y, h), vec2(u1, 1.0), c),
    ];
    Mesh { vertices, indices: vec![0, 1, 2, 2, 1, 3], texture: Some(texture) }
}

/// Отрисовать что-то 2D-шное в текстуру.
fn bake(w: u32, h: u32, font: &Font, bold: &Font, draw: impl Fn(&G)) -> Texture2D {
    let rt = render_target(w, h);
    rt.texture.set_filter(FilterMode::Linear);
    set_camera(&Camera2D {
        zoom: vec2(2.0 / w as f32, 2.0 / h as f32),
        target: vec2(w as f32 / 2.0, h as f32 / 2.0),
        render_target: Some(rt.clone()),
        ..Default::default()
    });
    clear_background(Color::new(0.0, 0.0, 0.0, 0.0));
    let g = G::fixed(font, bold, w as f32, h as f32);
    draw(&g);
    set_default_camera();
    let tex = rt.texture.clone();
    // RenderTarget при удалении освобождает и текстуру — держим его живым всю игру
    std::mem::forget(rt);
    tex
}

impl Scene {
    pub fn new(font: &Font, bold: &Font) -> Scene {
        // ---- текстуры карт
        let (tw, th) = (CARD_TW as f32, CARD_TH as f32);
        let mut cards = Vec::with_capacity(52);
        for code in 0..52u8 {
            let card = code_card(code);
            cards.push(bake(CARD_TW, CARD_TH, font, bold, |g| {
                g.card_face_flat(0.0, 0.0, tw, th, card);
            }));
        }
        let back = bake(CARD_TW, CARD_TH, font, bold, |g| g.card_back_flat(0.0, 0.0, tw, th));
        let dealer_tex = bake(128, 128, font, bold, |g| {
            g.rect(0.0, 0.0, 128.0, 128.0, rgb(0xf7f7f7));
            g.ring(64.0, 64.0, 58.0, 5.0, rgb(0xb8bccb));
            g.text("D", 64.0, 66.0, 78.0, rgb(0x1a1c25), crate::gfx::Align::Center, true);
        });

        // ---- сукно
        let edge = rgb(0x0c4f2e);
        let center = rgb(0x22a462);
        let felt = grid_mesh(128, 24, |u, v| {
            let t = u * 2.0 * PI;
            let k = 1.0 - v;
            let p = vec3(RX * k * t.cos(), 0.0, RZ * k * t.sin());
            let m = (1.0 - k).powf(0.7);
            let c = crate::gfx::mix(edge, center, m);
            (p, shade(c, Vec3::Y, p, 1.0, 0.0))
        });
        // линия ставок
        let line = grid_mesh(128, 1, |u, v| {
            let t = u * 2.0 * PI;
            let k = 0.72 + v * 0.012;
            let p = vec3(RX * k * t.cos(), 0.012, RZ * k * t.sin());
            (p, [140, 230, 180, 70])
        });

        // ---- кожаный борт (тор по эллипсу)
        let (brx, brz) = (RX + 0.42, RZ + 0.42);
        let leather = rgb(0x2b1a14);
        let rail = grid_mesh(128, 20, |u, v| {
            let t = u * 2.0 * PI;
            let phi = v * 2.0 * PI;
            let ne = ellipse_normal(t, brx, brz);
            let c = vec3(brx * t.cos(), 0.16, brz * t.sin());
            let p = c + ne * phi.cos() * 0.52 + Vec3::Y * phi.sin() * 0.3;
            let n = ne * phi.cos() / 0.52 + Vec3::Y * phi.sin() / 0.3;
            (p, shade(leather, n, p * 0.5, 18.0, 0.22))
        });
        // золотой кант
        let gold = rgb(0xd9a93a);
        let piping = grid_mesh(128, 8, |u, v| {
            let t = u * 2.0 * PI;
            let phi = v * 2.0 * PI;
            let (prx, prz) = (RX - 0.06, RZ - 0.06);
            let ne = ellipse_normal(t, prx, prz);
            let c = vec3(prx * t.cos(), 0.05, prz * t.sin());
            let p = c + ne * phi.cos() * 0.06 + Vec3::Y * phi.sin() * 0.06;
            let n = ne * phi.cos() + Vec3::Y * phi.sin();
            (p, shade(gold, n, p * 0.4, 24.0, 0.5))
        });
        // деревянная юбка стола
        let wood = rgb(0x3a2116);
        let skirt = grid_mesh(128, 1, |u, v| {
            let t = u * 2.0 * PI;
            let k = 1.0 - v * 0.12;
            let p = vec3((brx + 0.3) * k * t.cos(), -0.12 - v * 1.6, (brz + 0.3) * k * t.sin());
            let n = ellipse_normal(t, brx, brz) + Vec3::Y * -0.1;
            (p, shade(wood, n, p * 0.5, 10.0, 0.1))
        });

        let chips = chip_colors().iter().map(|&c| chip_mesh(c)).collect();

        // кнопка дилера: шайба с текстурой «D» сверху
        let mut dealer = chip_mesh(rgb(0xf4f4f4));
        let n = 32;
        let r = CHIP_R * 1.18;
        let base = dealer.vertices.len() as u16;
        dealer.vertices.push(vtx(vec3(0.0, CHIP_H + 0.008, 0.0), vec2(0.5, 0.5), [255; 4]));
        for i in 0..=n {
            let a = i as f32 / n as f32 * 2.0 * PI;
            dealer.vertices.push(vtx(
                vec3(a.cos() * r, CHIP_H + 0.008, a.sin() * r),
                vec2(0.5 + a.cos() * 0.5, 0.5 + a.sin() * 0.5),
                [255; 4],
            ));
        }
        // все прочие вершины используют белый угол текстуры
        for v in dealer.vertices.iter_mut().take(base as usize) {
            v.uv = vec2(0.03, 0.03);
        }
        for i in 0..n as u16 {
            dealer.indices.extend_from_slice(&[base, base + 1 + i, base + 2 + i]);
        }
        dealer.texture = Some(dealer_tex);

        let card_mesh_face = card_quad(cards[0].clone(), false);
        let card_mesh_back = card_quad(back.clone(), true);

        Scene { felt, rail, piping, skirt, line, chips, dealer, cards, back, card_mesh_face, card_mesh_back }
    }

    pub fn camera() -> Camera3D {
        let aspect = screen_width() / screen_height().max(1.0);
        let base = 40.0f32.to_radians();
        // на «квадратных» экранах увеличиваем угол обзора, чтобы стол влезал
        let fovy = if aspect < 1.75 { 2.0 * ((base / 2.0).tan() * 1.75 / aspect).atan() } else { base };
        let fovy = fovy * 0.92;
        Camera3D {
            position: vec3(0.0, 15.0, 5.8),
            target: vec3(0.0, -0.4, 0.15),
            up: Vec3::Y,
            fovy,
            // узкий диапазон глубины: на телефонах буфер глубины 16-битный
            z_near: 2.0,
            z_far: 60.0,
            ..Default::default()
        }
    }

    /// Мировая точка → виртуальные экранные координаты HUD.
    pub fn project(cam: &Camera3D, g: &G, p: Vec3) -> Vec2 {
        let m = cam.matrix();
        let clip = m * vec4(p.x, p.y, p.z, 1.0);
        let ndc = clip.truncate() / clip.w;
        let sx = (ndc.x + 1.0) / 2.0 * screen_width();
        let sy = (1.0 - ndc.y) / 2.0 * screen_height();
        vec2(sx / g.s, sy / g.s)
    }

    pub fn draw_table(&self) {
        draw_mesh(&self.skirt);
        draw_mesh(&self.felt);
        draw_mesh(&self.line);
        draw_mesh(&self.piping);
        draw_mesh(&self.rail);
    }

    fn with_model(m: Mat4, f: impl FnOnce()) {
        unsafe {
            get_internal_gl().quad_gl.push_model_matrix(m);
        }
        f();
        unsafe {
            get_internal_gl().quad_gl.pop_model_matrix();
        }
    }

    /// Карта на столе. `flip`: 0 — рубашкой вверх, 1 — лицом вверх.
    /// `scale` — 1.0 = ширина 1 единица.
    pub fn draw_card(&self, pos: Vec3, yaw: f32, scale: f32, card: Option<u8>, flip: f32, lift: f32) {
        let flip = flip.clamp(0.0, 1.0);
        let angle = (1.0 - flip) * PI;
        let arc_lift = (flip * PI).sin() * 0.35 * scale;
        let m = Mat4::from_translation(pos + Vec3::Y * (0.035 + lift + arc_lift))
            * Mat4::from_rotation_y(yaw)
            * Mat4::from_scale(Vec3::splat(scale))
            * Mat4::from_rotation_z(angle);
        let face = card.map(|c| self.cards[c as usize].clone());
        Self::with_model(m, || {
            draw_mesh(&self.card_mesh_back);
            if let Some(tex) = face {
                let mut mesh = Mesh {
                    vertices: self.card_mesh_face.vertices.clone(),
                    indices: self.card_mesh_face.indices.clone(),
                    texture: Some(tex),
                };
                if flip < 0.5 {
                    // лицо ещё внизу — чуть опускаем, чтобы не просвечивало
                    for v in &mut mesh.vertices {
                        v.position.y = 0.0;
                    }
                }
                draw_mesh(&mesh);
            } else {
                // закрытая карта: рубашка и сверху
                let mut mesh = Mesh {
                    vertices: self.card_mesh_back.vertices.clone(),
                    indices: self.card_mesh_back.indices.clone(),
                    texture: Some(self.back.clone()),
                };
                for v in &mut mesh.vertices {
                    v.position.y = 0.004;
                }
                draw_mesh(&mesh);
            }
        });
    }

    /// Подсветка карты (золотая рамка под ней).
    pub fn draw_card_glow(&self, pos: Vec3, scale: f32, lift: f32) {
        let m = Mat4::from_translation(pos + Vec3::Y * (0.02 + lift)) * Mat4::from_scale(Vec3::splat(scale));
        let c = [255, 205, 80, 255];
        let (w, h) = (0.56, 0.76);
        let mesh = Mesh {
            vertices: vec![
                vtx(vec3(-w, 0.0, -h), Vec2::ZERO, c),
                vtx(vec3(w, 0.0, -h), Vec2::ZERO, c),
                vtx(vec3(-w, 0.0, h), Vec2::ZERO, c),
                vtx(vec3(w, 0.0, h), Vec2::ZERO, c),
            ],
            indices: vec![0, 1, 2, 2, 1, 3],
            texture: None,
        };
        Self::with_model(m, || draw_mesh(&mesh));
    }

    /// Стопка фишек; количество и цвета зависят от суммы относительно ББ.
    pub fn draw_chips(&self, pos: Vec3, amount: u64, bb: u64) {
        if amount == 0 {
            return;
        }
        let ratio = amount as f32 / bb.max(1) as f32;
        let n = (1.0 + (ratio * 2.0).log2().max(0.0) * 1.6).round().clamp(1.0, 16.0) as usize;
        let stacks = if n > 6 { 2 } else { 1 };
        let per = n.div_ceil(stacks);
        let mut k = 0;
        for s in 0..stacks {
            let off = if stacks == 2 { (s as f32 - 0.5) * CHIP_R * 2.15 } else { 0.0 };
            for i in 0..per {
                if k >= n {
                    break;
                }
                let color = (k / 2 + s) % self.chips.len();
                let jitter = ((k * 7919) % 13) as f32 / 13.0 - 0.5;
                let p = pos + vec3(off + jitter * 0.012, i as f32 * CHIP_H, jitter * 0.01);
                Self::with_model(Mat4::from_translation(p) * Mat4::from_rotation_y(k as f32), || {
                    draw_mesh(&self.chips[color]);
                });
                k += 1;
            }
        }
    }

    pub fn draw_dealer(&self, pos: Vec3) {
        Self::with_model(Mat4::from_translation(pos) * Mat4::from_scale(vec3(1.3, 1.0, 1.3)), || {
            draw_mesh(&self.dealer);
        });
    }
}
