//! 3D-сцена: советская комната ночью, деревянный стол со скатертью-ковром,
//! неоновая рамка, карты, фишки и кнопка дилера. Строится на собственном движке.

use std::cell::RefCell;
use std::f32::consts::PI;

use crate::engine::canvas::Canvas;
use crate::engine::prelude::*;
use crate::engine::texgen;
use crate::gfx::{G, rgb};
use strawberry_poker::proto::code_card;

/// Полуразмеры столешницы.
pub const TX: f32 = 5.4;
pub const TZ: f32 = 3.4;
/// Высота пола относительно столешницы.
const FLOOR: f32 = -3.2;

const SLOT_W: f32 = 200.0;
const SLOT_H: f32 = 280.0;
const ATLAS: f32 = 2048.0;
const SLOT_BACK: usize = 52;
const SLOT_DEALER: usize = 53;

pub const CHIP_R: f32 = 0.2;
pub const CHIP_H: f32 = 0.06;


pub fn chip_colors() -> [Color; 6] {
    [rgb(0x1f5fd0), rgb(0xd0303a), rgb(0x2b9b55), rgb(0x26262c), rgb(0x8b3fd1), rgb(0xe8e8ee)]
}

fn slot_rect(slot: usize) -> [f32; 4] {
    let (col, row) = ((slot % 10) as f32, (slot / 10) as f32);
    [col * SLOT_W / ATLAS, row * SLOT_H / ATLAS, SLOT_W / ATLAS, SLOT_H / ATLAS]
}

// ------------------------------------------------------------------ генераторы мешей

/// Коробка с центром в нуле и размерами `size`; uv в «метрах» × `uv_k`.
fn box_mesh(size: Vec3, uv_k: f32, color: Color) -> MeshData {
    let h = size * 0.5;
    let mut m = MeshData::new();
    let faces: [(Vec3, Vec3, Vec3); 6] = [
        (Vec3::X, Vec3::Z, Vec3::Y),
        (-Vec3::X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, Vec3::Z),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (-Vec3::Z, Vec3::X, Vec3::Y),
    ];
    for (n, a, b) in faces {
        let c = n.mul_elem(h);
        let ea = a.mul_elem(h);
        let eb = b.mul_elem(h);
        let (la, lb) = (ea.length() * 2.0 * uv_k, eb.length() * 2.0 * uv_k);
        let v0 = m.push(c - ea - eb, n, vec2(0.0, 0.0), color);
        let v1 = m.push(c + ea - eb, n, vec2(la, 0.0), color);
        let v2 = m.push(c + ea + eb, n, vec2(la, lb), color);
        let v3 = m.push(c - ea + eb, n, vec2(0.0, lb), color);
        m.quad(v0, v1, v2, v3);
    }
    m
}

/// Плоский прямоугольник в плоскости XY (нормаль +Z), uv 0..1 (v вниз).
fn plane_xy(w: f32, h: f32, color: Color) -> MeshData {
    let mut m = MeshData::new();
    let a = m.push(vec3(-w / 2.0, h / 2.0, 0.0), Vec3::Z, vec2(0.0, 0.0), color);
    let b = m.push(vec3(w / 2.0, h / 2.0, 0.0), Vec3::Z, vec2(1.0, 0.0), color);
    let c = m.push(vec3(w / 2.0, -h / 2.0, 0.0), Vec3::Z, vec2(1.0, 1.0), color);
    let d = m.push(vec3(-w / 2.0, -h / 2.0, 0.0), Vec3::Z, vec2(0.0, 1.0), color);
    m.quad(a, b, c, d);
    m
}

/// Тело вращения по профилю (радиус, высота) снизу вверх, с крышкой сверху.
fn lathe(profile: &[(f32, f32)], seg: usize, color: impl Fn(usize, usize) -> Color) -> MeshData {
    let mut m = MeshData::grid(seg, profile.len() - 1, |u, v| {
        let i = ((v * (profile.len() - 1) as f32).round() as usize).min(profile.len() - 1);
        let (r, y) = profile[i];
        let a = u * 2.0 * PI;
        // нормаль по наклону профиля
        let (r0, y0) = profile[i.saturating_sub(1)];
        let (r1, y1) = profile[(i + 1).min(profile.len() - 1)];
        let (dr, dy) = (r1 - r0, y1 - y0);
        let n = vec3(a.cos() * dy, -dr, a.sin() * dy);
        let seg_i = (u * seg as f32) as usize;
        (vec3(a.cos() * r, y, a.sin() * r), n, vec2(u, v), color(seg_i, i))
    });
    let (rt, yt) = *profile.last().unwrap();
    let top: Vec<Vec2> = (0..seg).map(|k| {
        let a = k as f32 / seg as f32 * 2.0 * PI;
        vec2(a.cos() * rt, a.sin() * rt)
    }).collect();
    m.fan_xz(&top, yt, true, color(0, profile.len()), |_| vec2(0.5, 0.5));
    m
}

/// Контур игрового поля (прямоугольник со срезанными углами) для неона.
fn neon_outline(hx: f32, hz: f32, cut: f32) -> Vec<Vec2> {
    vec![
        vec2(-hx + cut, -hz),
        vec2(hx - cut, -hz),
        vec2(hx, -hz + cut),
        vec2(hx, hz - cut),
        vec2(hx - cut, hz),
        vec2(-hx + cut, hz),
        vec2(-hx, hz - cut),
        vec2(-hx, -hz + cut),
    ]
}

/// Лента вдоль замкнутого контура на высоте y: ширина `w`, прозрачность по краям.
fn ribbon(outline: &[Vec2], y: f32, w: f32, core: Color, edge_alpha: f32) -> MeshData {
    let mut m = MeshData::new();
    let n = outline.len();
    // уплотняем контур, чтобы альфа плавно шла вдоль прямых
    let mut pts = Vec::new();
    for i in 0..n {
        let a = outline[i];
        let b = outline[(i + 1) % n];
        for k in 0..8 {
            pts.push(a.lerp(b, k as f32 / 8.0));
        }
    }
    let n = pts.len();
    for i in 0..n {
        let prev = pts[(i + n - 1) % n];
        let next = pts[(i + 1) % n];
        let d = next - prev;
        let l = d.length().max(1e-6);
        let nrm = vec2(d.y / l, -d.x / l);
        let p = pts[i];
        let edge = Color::new(core.r, core.g, core.b, edge_alpha);
        m.push(vec3(p.x - nrm.x * w, y, p.y - nrm.y * w), Vec3::Y, Vec2::ZERO, edge);
        m.push(vec3(p.x, y, p.y), Vec3::Y, Vec2::ZERO, core);
        m.push(vec3(p.x + nrm.x * w, y, p.y + nrm.y * w), Vec3::Y, Vec2::ZERO, edge);
    }
    for i in 0..n as u16 {
        let a = i * 3;
        let b = ((i + 1) % n as u16) * 3;
        m.quad(a, b, b + 1, a + 1);
        m.quad(a + 1, b + 1, b + 2, a + 2);
    }
    m
}

fn chip_mesh(base: Color) -> MeshData {
    let white = rgb(0xf2f2f2);
    let seg = 36;
    let stripe = |i: usize| (i / 2) % 3 == 0;
    let mut m = MeshData::new();
    // боковина с полосками
    for i in 0..seg {
        let a0 = i as f32 / seg as f32 * 2.0 * PI;
        let a1 = (i + 1) as f32 / seg as f32 * 2.0 * PI;
        let c = if stripe(i) { white } else { base };
        let n0 = vec3(a0.cos(), 0.0, a0.sin());
        let n1 = vec3(a1.cos(), 0.0, a1.sin());
        let v0 = m.push(n0 * CHIP_R, n0, Vec2::ZERO, c);
        let v1 = m.push(n1 * CHIP_R, n1, Vec2::ZERO, c);
        let v2 = m.push(n1 * CHIP_R + Vec3::Y * CHIP_H, n1, Vec2::ZERO, c);
        let v3 = m.push(n0 * CHIP_R + Vec3::Y * CHIP_H, n0, Vec2::ZERO, c);
        m.quad(v0, v1, v2, v3);
    }
    // верх: кольцо с полосками, тонкое белое кольцо, центр
    let inner = Color::new(base.r * 0.82, base.g * 0.82, base.b * 0.82, 1.0);
    let y = CHIP_H;
    let center = m.push(vec3(0.0, y, 0.0), Vec3::Y, Vec2::ZERO, inner);
    for i in 0..seg {
        let a0 = i as f32 / seg as f32 * 2.0 * PI;
        let a1 = (i + 1) as f32 / seg as f32 * 2.0 * PI;
        let c = if stripe(i) { white } else { base };
        let ring = [(CHIP_R * 0.6, inner), (CHIP_R * 0.66, white), (CHIP_R, c)];
        let mut prev: Option<(u16, u16)> = None;
        for (r, col) in ring {
            let p0 = m.push(vec3(a0.cos() * r, y, a0.sin() * r), Vec3::Y, Vec2::ZERO, col);
            let p1 = m.push(vec3(a1.cos() * r, y, a1.sin() * r), Vec3::Y, Vec2::ZERO, col);
            match prev {
                None => m.tri(center, p0, p1),
                Some((q0, q1)) => m.quad(q0, q1, p1, p0),
            }
            prev = Some((p0, p1));
        }
    }
    m
}

/// Карта: скруглённая пластинка. Лицо — сверху, рубашка — снизу.
fn card_meshes() -> (MeshData, MeshData) {
    let (w, h, r) = (1.0, 1.4, 0.09);
    let outline = rounded_rect(w, h, r, 5);
    let th = 0.012;
    let uv = |p: Vec2| vec2(p.x / w + 0.5, p.y / h + 0.5);
    let mut face = MeshData::new();
    face.fan_xz(&outline, th, true, WHITE, uv);
    extrude_sides(&mut face, &outline, -th, th, |_| rgb(0xe6e6ea));
    let mut back = MeshData::new();
    back.fan_xz(&outline, -th, false, WHITE, |p| vec2(0.5 - p.x / w, p.y / h + 0.5));
    (face, back)
}

// ------------------------------------------------------------------ сцена

struct Obj {
    mesh: MeshId,
    model: Mat4,
    texture: Option<TexId>,
    material: Material,
    tint: Color,
}

pub struct Scene {
    statics: Vec<Obj>,
    transparent: Vec<Obj>,
    chips: Vec<MeshId>,
    dealer: MeshId,
    dealer_top: MeshId,
    card_face: MeshId,
    card_back: MeshId,
    glow_quad: MeshId,
    pub atlas: TexId,
    cmds: RefCell<Vec<DrawCmd>>,
    pub light: Light,
}

impl Scene {
    pub fn new(r: &mut Renderer, canvas: &RefCell<Canvas>) -> Scene {
        // ---- атлас карт: рисуем 2D-слоем в текстуру
        {
            canvas.borrow_mut().begin();
            let g = G::fixed(canvas, ATLAS, ATLAS);
            for slot in 0..54usize {
                let (x, y) = ((slot % 10) as f32 * SLOT_W, (slot / 10) as f32 * SLOT_H);
                match slot {
                    SLOT_BACK => g.card_back_flat(x + 2.0, y + 2.0, SLOT_W - 4.0, SLOT_H - 4.0),
                    SLOT_DEALER => {
                        g.circle(x + 100.0, y + 140.0, 98.0, rgb(0xf4f4f4));
                        g.ring(x + 100.0, y + 140.0, 90.0, 8.0, rgb(0xb8bccb));
                        g.text("D", x + 100.0, y + 143.0, 120.0, rgb(0x1a1c25), crate::gfx::Align::Center, true);
                    }
                    _ => g.card_face_flat(x + 2.0, y + 2.0, SLOT_W - 4.0, SLOT_H - 4.0, code_card(slot as u8)),
                }
            }
        }
        let atlas = r.bake(&mut canvas.borrow_mut(), 0, ATLAS as u32, ATLAS as u32);
        canvas.borrow_mut().begin();

        // ---- процедурные текстуры
        let tex = |r: &mut Renderer, img: texgen::Image, repeat: bool| r.texture(img.w as u32, img.h as u32, &img.data, repeat, true);
        let wood = tex(r, texgen::worn_wood(512, 3, 0x5a3420), true);
        let wood_dark = tex(r, texgen::worn_wood(512, 11, 0x3b2216), true);
        let cloth = tex(r, texgen::carpet(1024, 640, 5), false);
        let rug = tex(r, texgen::carpet(512, 640, 17), false);
        let walls = tex(r, texgen::wallpaper(512, 7), true);
        let city = tex(r, texgen::night_city(512, 512, 9), false);
        let lace = tex(r, texgen::lace(256, 4), true);
        let floor = tex(r, texgen::floor(512, 8), true);
        let tv = tex(r, texgen::tv_screen(256, 192), false);
        let enamel = tex(r, texgen::enamel(256, 6), true);

        let mut statics = Vec::new();
        let mut transparent = Vec::new();
        let add = |list: &mut Vec<Obj>, r: &mut Renderer, m: MeshData, model: Mat4, texture: Option<TexId>, material: Material| {
            let mesh = r.upload_mesh(&m);
            list.push(Obj { mesh, model, texture, material, tint: WHITE });
        };
        let at = |x: f32, y: f32, z: f32| Mat4::translation(vec3(x, y, z));

        // ---- стол
        add(&mut statics, r, box_mesh(vec3(TX * 2.0, 0.24, TZ * 2.0), 0.25, WHITE), at(0.0, -0.12, 0.0), Some(wood), Material::GLOSSY);
        // царга под столешницей
        add(&mut statics, r, box_mesh(vec3(TX * 2.0 - 0.6, 0.5, TZ * 2.0 - 0.6), 0.25, WHITE), at(0.0, -0.45, 0.0), Some(wood_dark), Material::MATTE);
        for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            add(
                &mut statics,
                r,
                box_mesh(vec3(0.35, -FLOOR, 0.35), 0.25, WHITE),
                at(sx * (TX - 0.45), FLOOR / 2.0, sz * (TZ - 0.45)),
                Some(wood_dark),
                Material::MATTE,
            );
        }
        // скатерть-ковёр
        let mut cloth_m = MeshData::new();
        let (cx, cz) = (TX - 0.35, TZ - 0.3);
        cloth_m.fan_xz(
            &[vec2(-cx, -cz), vec2(cx, -cz), vec2(cx, cz), vec2(-cx, cz)],
            0.006,
            true,
            WHITE,
            |p| vec2(p.x / (2.0 * cx) + 0.5, p.y / (2.0 * cz) + 0.5),
        );
        add(&mut statics, r, cloth_m, Mat4::IDENTITY, Some(cloth), Material::MATTE);

        // неоновая рамка: яркая сердцевина + широкое свечение
        let outline = neon_outline(TX - 0.9, TZ - 0.75, 0.6);
        add(&mut transparent, r, ribbon(&outline, 0.02, 0.05, Color::new(0.85, 0.6, 1.0, 1.0), 1.0), Mat4::IDENTITY, None, Material::UNLIT);
        add(&mut transparent, r, ribbon(&outline, 0.018, 0.55, Color::new(0.55, 0.2, 1.0, 0.32), 0.0), Mat4::IDENTITY, None, Material::GLOW);

        // ---- пол и стены
        let room = (13.0, 11.0);
        let mut floor_m = MeshData::new();
        floor_m.fan_xz(
            &[vec2(-room.0, -room.1), vec2(room.0, -room.1), vec2(room.0, room.1 + 6.0), vec2(-room.0, room.1 + 6.0)],
            FLOOR,
            true,
            WHITE,
            |p| vec2(p.x * 0.12, p.y * 0.12),
        );
        add(&mut statics, r, floor_m, Mat4::IDENTITY, Some(floor), Material::MATTE.no_shadow());
        let wall_h = 12.0;
        let wall = |w: f32| {
            let mut m = plane_xy(w, wall_h, WHITE);
            for v in &mut m.vertices {
                v.uv = [v.uv[0] * w * 0.22, v.uv[1] * wall_h * 0.22];
            }
            m
        };
        add(&mut statics, r, wall(room.0 * 2.0), at(0.0, FLOOR + wall_h / 2.0, -room.1), Some(walls), Material::MATTE.no_shadow());
        add(
            &mut statics,
            r,
            wall(room.1 * 2.0 + 6.0),
            at(-room.0, FLOOR + wall_h / 2.0, 3.0) * Mat4::rotation_y(PI / 2.0),
            Some(walls),
            Material::MATTE.no_shadow(),
        );
        add(
            &mut statics,
            r,
            wall(room.1 * 2.0 + 6.0),
            at(room.0, FLOOR + wall_h / 2.0, 3.0) * Mat4::rotation_y(-PI / 2.0),
            Some(walls),
            Material::MATTE.no_shadow(),
        );

        // ---- окно с видом на ночные панельки
        let bz = -room.1 + 0.02;
        add(&mut statics, r, plane_xy(5.0, 4.6, WHITE), at(1.6, 3.4, bz), Some(city), Material::UNLIT);
        let frame = rgb(0xd8d2c4);
        for (x, y, w, h) in [(1.6, 5.75, 5.3, 0.3), (1.6, 1.05, 5.3, 0.3), (-0.95, 3.4, 0.3, 4.9), (4.15, 3.4, 0.3, 4.9), (1.6, 3.4, 0.16, 4.6), (1.6, 4.5, 5.0, 0.12)] {
            add(&mut statics, r, box_mesh(vec3(w, h, 0.25), 1.0, frame), at(x, y, bz + 0.1), None, Material::MATTE);
        }
        // подоконник и батарея
        add(&mut statics, r, box_mesh(vec3(5.8, 0.15, 0.7), 1.0, frame), at(1.6, 0.85, bz + 0.35), None, Material::MATTE);
        for k in 0..12 {
            add(
                &mut statics,
                r,
                box_mesh(vec3(0.22, 1.5, 0.3), 1.0, rgb(0xcfc8b6)),
                at(-0.5 + k as f32 * 0.38, -0.3, bz + 0.25),
                None,
                Material::GLOSSY,
            );
        }
        // цветок на подоконнике
        add(
            &mut statics,
            r,
            lathe(&[(0.18, 0.0), (0.25, 0.35), (0.27, 0.4)], 16, |_, _| rgb(0x8a4b2a)),
            at(3.4, 0.92, bz + 0.4),
            None,
            Material::MATTE,
        );
        add(
            &mut statics,
            r,
            lathe(&[(0.05, 0.0), (0.45, 0.3), (0.55, 0.7), (0.3, 1.1), (0.02, 1.2)], 12, |s, _| {
                if s % 2 == 0 { rgb(0x2f6b34) } else { rgb(0x3d8442) }
            }),
            at(3.4, 1.3, bz + 0.4),
            None,
            Material::MATTE,
        );
        // тюль по бокам окна (полупрозрачный)
        for (x, w) in [(-0.9, 2.2), (4.3, 2.0)] {
            let mut m = plane_xy(w, 6.4, WHITE);
            for v in &mut m.vertices {
                v.uv = [v.uv[0] * w * 0.6, v.uv[1] * 6.4 * 0.6];
            }
            add(&mut transparent, r, m, at(x, 3.0, bz + 0.45), Some(lace), Material::MATTE.no_shadow());
        }

        // ---- ковёр на стене
        add(&mut statics, r, plane_xy(4.0, 5.0, WHITE), at(-4.6, 3.3, bz + 0.03), Some(rug), Material::MATTE.no_shadow());
        // фотографии в рамках
        for (x, y) in [(-9.4, 6.4), (-7.9, 6.1)] {
            add(&mut statics, r, box_mesh(vec3(1.0, 1.25, 0.08), 1.0, rgb(0x2a1a10)), at(x, y, bz + 0.05), None, Material::GLOSSY);
            add(&mut statics, r, plane_xy(0.75, 1.0, rgb(0x8f8a7e)), at(x, y, bz + 0.1), None, Material::MATTE);
        }

        // ---- сервант слева
        let sx = -room.0 + 1.2;
        add(&mut statics, r, box_mesh(vec3(2.2, 3.2, 6.5), 0.3, WHITE), at(sx, FLOOR + 1.6, -5.5), Some(wood_dark), Material::GLOSSY);
        add(&mut statics, r, box_mesh(vec3(1.8, 4.4, 6.5), 0.3, WHITE), at(sx - 0.2, FLOOR + 3.2 + 2.2, -5.5), Some(wood_dark), Material::GLOSSY);
        // полки и посуда за стеклом
        for k in 0..6 {
            let z = -8.2 + k as f32 * 1.05;
            add(
                &mut statics,
                r,
                lathe(&[(0.05, 0.0), (0.18, 0.05), (0.04, 0.12), (0.04, 0.4), (0.2, 0.55), (0.24, 0.85)], 14, |_, _| {
                    Color::new(0.8, 0.85, 0.95, 1.0)
                }),
                at(sx + 1.0, FLOOR + 3.6, z) * Mat4::scale(Vec3::splat(0.7)),
                None,
                Material::GLOSSY,
            );
            add(
                &mut statics,
                r,
                lathe(&[(0.2, 0.0), (0.3, 0.3), (0.32, 0.5), (0.18, 0.75)], 14, |_, _| rgb(0xf3efe6)),
                at(sx + 1.0, FLOOR + 5.2, z + 0.4) * Mat4::scale(Vec3::splat(0.6)),
                None,
                Material::GLOSSY,
            );
        }
        let mut glass = plane_xy(6.3, 4.0, Color::new(0.6, 0.7, 0.9, 0.18));
        for v in &mut glass.vertices {
            v.normal = [1.0, 0.0, 0.0];
        }
        add(&mut transparent, r, glass, at(sx + 0.75, FLOOR + 5.4, -5.5) * Mat4::rotation_y(PI / 2.0), None, Material::GLOSSY.no_shadow());

        // ---- холодильник, телевизор, тумба с чайником справа
        let fx = room.0 - 2.0;
        add(&mut statics, r, box_mesh(vec3(2.6, 6.2, 2.4), 0.4, WHITE), at(fx, FLOOR + 3.1, -8.6), Some(enamel), Material::GLOSSY);
        add(&mut statics, r, box_mesh(vec3(0.12, 1.2, 0.15), 1.0, rgb(0xb0b4bc)), at(fx - 1.1, FLOOR + 4.4, -7.35), None, Material::GLOSSY);
        add(&mut statics, r, box_mesh(vec3(2.2, 1.7, 1.7), 1.0, rgb(0x3a2418)), at(fx, FLOOR + 7.05, -8.6), Some(wood_dark), Material::GLOSSY);
        add(&mut statics, r, plane_xy(1.5, 1.15, WHITE), at(fx - 0.15, FLOOR + 7.05, -7.74), Some(tv), Material::UNLIT);
        for a in [-0.5f32, 0.5] {
            add(
                &mut statics,
                r,
                box_mesh(vec3(0.04, 1.4, 0.04), 1.0, rgb(0x777777)),
                at(fx + a * 0.6, FLOOR + 8.4, -8.6) * Mat4::rotation_z(a * 0.8),
                None,
                Material::GLOSSY,
            );
        }
        add(&mut statics, r, box_mesh(vec3(2.4, 2.6, 2.2), 0.4, WHITE), at(fx + 0.2, FLOOR + 1.3, -4.6), Some(enamel), Material::MATTE);
        add(
            &mut statics,
            r,
            lathe(&[(0.35, 0.0), (0.42, 0.2), (0.4, 0.8), (0.25, 1.0), (0.1, 1.1)], 20, |_, _| rgb(0xa9adb6)),
            at(fx, FLOOR + 2.6, -4.4),
            None,
            Material { specular: 0.9, shininess: 64.0, ..Material::GLOSSY },
        );
        // календарь на стене
        add(&mut statics, r, plane_xy(1.6, 2.2, rgb(0xd8d0c0)), at(room.0 - 0.02, 4.0, -5.0) * Mat4::rotation_y(-PI / 2.0), None, Material::MATTE);
        add(&mut statics, r, plane_xy(1.4, 0.7, rgb(0xb3202a)), at(room.0 - 0.04, 4.6, -5.0) * Mat4::rotation_y(-PI / 2.0), None, Material::MATTE);

        // ---- стулья вокруг стола
        for (x, z, rot) in [
            (-3.6, TZ + 1.5, 0.25),
            (3.6, TZ + 1.5, -0.25),
            (-TX - 1.2, 1.6, -PI / 2.0),
            (-TX - 1.2, -1.6, -PI / 2.0),
            (0.0, -TZ - 1.4, PI),
            (TX + 1.2, -1.6, PI / 2.0),
            (TX + 1.2, 1.6, PI / 2.0),
        ] {
            let base = at(x, 0.0, z) * Mat4::rotation_y(rot);
            add(&mut statics, r, box_mesh(vec3(1.8, 0.16, 1.7), 0.4, WHITE), base * at(0.0, -1.3, 0.0), Some(wood), Material::GLOSSY);
            add(&mut statics, r, box_mesh(vec3(1.8, 2.2, 0.16), 0.4, WHITE), base * at(0.0, -0.1, 0.8), Some(wood), Material::GLOSSY);
            for (lx, lz) in [(-0.75, -0.7), (0.75, -0.7), (-0.75, 0.7), (0.75, 0.7)] {
                add(&mut statics, r, box_mesh(vec3(0.14, 1.9, 0.14), 0.4, WHITE), base * at(lx, -2.25, lz), Some(wood_dark), Material::MATTE);
            }
        }

        // ---- игровые объекты
        let chips = chip_colors().iter().map(|&c| r.upload_mesh(&chip_mesh(c))).collect();
        let dealer = r.upload_mesh(&chip_mesh(rgb(0xf4f4f4)));
        let mut top = MeshData::new();
        let circle: Vec<Vec2> = (0..32).map(|k| {
            let a = k as f32 / 32.0 * 2.0 * PI;
            vec2(a.cos() * CHIP_R * 0.95, a.sin() * CHIP_R * 0.95)
        }).collect();
        // кружок «D» из атласа: слот 200×280, центр (100, 140), радиус 98 px
        top.fan_xz(&circle, CHIP_H + 0.004, true, WHITE, |p| {
            vec2(0.5 + p.x / (CHIP_R * 0.95) * 0.49, 0.5 + p.y / (CHIP_R * 0.95) * 0.35)
        });
        let dealer_top = r.upload_mesh(&top);
        let (face, back) = card_meshes();
        let card_face = r.upload_mesh(&face);
        let card_back = r.upload_mesh(&back);
        let mut glow = MeshData::new();
        glow.fan_xz(&rounded_rect(1.16, 1.56, 0.14, 5), 0.0, true, WHITE, |_| Vec2::ZERO);
        let glow_quad = r.upload_mesh(&glow);

        Scene {
            statics,
            transparent,
            chips,
            dealer,
            dealer_top,
            card_face,
            card_back,
            glow_quad,
            atlas,
            cmds: RefCell::new(Vec::new()),
            light: Light {
                dir: vec3(0.18, -1.0, -0.3),
                color: Color::new(0.82, 0.68, 0.6, 1.0),
                ambient: Color::new(0.15, 0.11, 0.21, 1.0),
                center: vec3(0.0, 0.0, -0.5),
                half_w: 9.0,
                half_h: 7.0,
                spot: vec2(13.0, 11.0),
                points: [
                    PointLight { pos: vec3(0.0, 1.2, 0.0), radius: 8.5, color: Color::new(0.6, 0.3, 1.0, 0.45) },
                    PointLight { pos: vec3(1.6, 3.4, -9.5), radius: 9.0, color: Color::new(0.45, 0.35, 1.0, 0.8) },
                ],
            },
        }
    }

    pub fn table_camera() -> Camera {
        let (w, h) = (screen_width(), screen_height().max(1.0));
        let aspect = w / h;
        let base = 56f32.to_radians();
        // на «квадратных» экранах увеличиваем угол обзора, чтобы стол влезал
        let fovy = if aspect < 1.75 { 2.0 * ((base / 2.0).tan() * 1.75 / aspect).atan() } else { base };
        Camera {
            position: vec3(0.0, 4.9, 9.6),
            target: vec3(0.0, -0.1, -1.6),
            up: Vec3::Y,
            fovy,
            near: 0.5,
            far: 60.0,
        }
    }

    /// Начать кадр: статичная комната и стол.
    pub fn begin(&self, camera: Camera) -> Camera {
        let mut cmds = self.cmds.borrow_mut();
        cmds.clear();
        for o in &self.statics {
            cmds.push(DrawCmd {
                mesh: o.mesh,
                model: o.model,
                texture: o.texture,
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                tint: o.tint,
                material: o.material,
            });
        }
        camera
    }

    /// Команды кадра: непрозрачное, затем прозрачное (тюль, стекло, неон).
    pub fn take_cmds(&self) -> Vec<DrawCmd> {
        let mut cmds = std::mem::take(&mut *self.cmds.borrow_mut());
        let mut add: Vec<DrawCmd> = Vec::new();
        for o in &self.transparent {
            let c = DrawCmd {
                mesh: o.mesh,
                model: o.model,
                texture: o.texture,
                uv_rect: [0.0, 0.0, 1.0, 1.0],
                tint: o.tint,
                material: o.material,
            };
            if o.material.additive { add.push(c) } else { cmds.push(c) }
        }
        // свечение рисуем последним, поверх всего
        let (mut glow, rest): (Vec<DrawCmd>, Vec<DrawCmd>) = cmds.into_iter().partition(|c| c.material.additive);
        let mut out = rest;
        out.append(&mut add);
        out.append(&mut glow);
        out
    }

    fn push(&self, mesh: MeshId, model: Mat4, texture: Option<TexId>, uv_rect: [f32; 4], tint: Color, material: Material) {
        self.cmds.borrow_mut().push(DrawCmd { mesh, model, texture, uv_rect, tint, material });
    }

    /// Мировая точка → виртуальные экранные координаты HUD.
    pub fn project(cam: &Camera, g: &G, p: Vec3) -> Vec2 {
        let (w, h) = (screen_width(), screen_height());
        let clip = cam.view_proj(w / h.max(1.0)).mul_vec4(p.extend(1.0));
        let ndc = clip.truncate() / clip.w;
        let sx = (ndc.x + 1.0) / 2.0 * w;
        let sy = (1.0 - ndc.y) / 2.0 * h;
        vec2(sx / g.s, sy / g.s)
    }

    /// Карта на столе. `flip`: 0 — рубашкой вверх, 1 — лицом вверх.
    /// `scale` 1.0 = ширина 1 единица.
    pub fn draw_card(&self, pos: Vec3, yaw: f32, scale: f32, card: Option<u8>, flip: f32, lift: f32) {
        let flip = flip.clamp(0.0, 1.0);
        // переворот кватернионом: поворот вокруг продольной оси карты
        let turn = Quat::from_axis_angle(Vec3::Z, (1.0 - flip) * PI);
        let arc_lift = (flip * PI).sin() * 0.4 * scale;
        let model = Mat4::translation(pos + Vec3::Y * (0.03 + lift + arc_lift))
            * Mat4::rotation_y(yaw)
            * Mat4::scale(Vec3::splat(scale))
            * turn.to_mat4();
        let face_slot = card.map(|c| c as usize).unwrap_or(SLOT_BACK);
        self.push(self.card_face, model, Some(self.atlas), slot_rect(face_slot), WHITE, Material::GLOSSY);
        self.push(self.card_back, model, Some(self.atlas), slot_rect(SLOT_BACK), WHITE, Material::GLOSSY);
    }

    /// Неоновая подсветка под картой.
    pub fn draw_card_glow(&self, pos: Vec3, scale: f32, lift: f32) {
        let model = Mat4::translation(pos + Vec3::Y * (0.02 + lift * 0.5)) * Mat4::scale(Vec3::splat(scale));
        self.push(self.glow_quad, model, None, [0.0, 0.0, 1.0, 1.0], Color::new(0.75, 0.45, 1.0, 0.9), Material::GLOW);
    }

    /// Стопка фишек; количество и цвета зависят от суммы относительно ББ.
    pub fn draw_chips(&self, pos: Vec3, amount: u64, bb: u64) {
        if amount == 0 {
            return;
        }
        let ratio = amount as f32 / bb.max(1) as f32;
        let n = (1.0 + (ratio * 2.0).log2().max(0.0) * 1.6).round().clamp(1.0, 18.0) as usize;
        let stacks = if n > 12 { 3 } else if n > 6 { 2 } else { 1 };
        let per = n.div_ceil(stacks);
        let mut k = 0;
        for s in 0..stacks {
            let ang = s as f32 * 2.1;
            let off = if stacks > 1 { vec3(ang.cos(), 0.0, ang.sin()) * (CHIP_R * 1.15) } else { Vec3::ZERO };
            for i in 0..per {
                if k >= n {
                    break;
                }
                let color = (k / 2 + s) % self.chips.len();
                let jitter = ((k * 7919) % 13) as f32 / 13.0 - 0.5;
                let p = pos + off + vec3(jitter * 0.015, 0.006 + i as f32 * CHIP_H, jitter * 0.012);
                let model = Mat4::translation(p) * Mat4::rotation_y(k as f32);
                self.push(self.chips[color], model, None, [0.0, 0.0, 1.0, 1.0], WHITE, Material::GLOSSY);
                k += 1;
            }
        }
    }

    pub fn draw_dealer(&self, pos: Vec3) {
        let model = Mat4::translation(pos + Vec3::Y * 0.006) * Mat4::scale(vec3(1.4, 1.0, 1.4));
        self.push(self.dealer, model, None, [0.0, 0.0, 1.0, 1.0], WHITE, Material::GLOSSY);
        self.push(self.dealer_top, model, Some(self.atlas), slot_rect(SLOT_DEALER), WHITE, Material::GLOSSY);
    }
}
