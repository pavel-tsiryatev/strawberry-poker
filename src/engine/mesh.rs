//! Геометрия на стороне CPU и генераторы примитивов.

use super::math::*;

/// Вершина 3D-меша: позиция, нормаль, текстурные координаты, цвет.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

#[derive(Default, Clone)]
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

impl MeshData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, p: Vec3, n: Vec3, uv: Vec2, c: Color) -> u16 {
        assert!(self.vertices.len() < u16::MAX as usize, "меш слишком большой для u16-индексов");
        self.vertices.push(Vertex {
            pos: p.to_array(),
            normal: n.normalize_or_zero().to_array(),
            uv: [uv.x, uv.y],
            color: c.to_array(),
        });
        (self.vertices.len() - 1) as u16
    }

    pub fn tri(&mut self, a: u16, b: u16, c: u16) {
        self.indices.extend_from_slice(&[a, b, c]);
    }

    pub fn quad(&mut self, a: u16, b: u16, c: u16, d: u16) {
        self.indices.extend_from_slice(&[a, b, c, a, c, d]);
    }

    /// Сетка (nu+1)×(nv+1) вершин; `f(u, v)` возвращает позицию, нормаль, uv и цвет.
    pub fn grid(nu: usize, nv: usize, f: impl Fn(f32, f32) -> (Vec3, Vec3, Vec2, Color)) -> MeshData {
        let mut m = MeshData::new();
        for i in 0..=nu {
            for j in 0..=nv {
                let (p, n, uv, c) = f(i as f32 / nu as f32, j as f32 / nv as f32);
                m.push(p, n, uv, c);
            }
        }
        let row = (nv + 1) as u16;
        for i in 0..nu as u16 {
            for j in 0..nv as u16 {
                let a = i * row + j;
                let b = (i + 1) * row + j;
                m.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
            }
        }
        m
    }

    /// Плоский многоугольник-веер в плоскости XZ на высоте `y` (нормаль ±Y).
    pub fn fan_xz(&mut self, outline: &[Vec2], y: f32, up: bool, color: Color, uv_of: impl Fn(Vec2) -> Vec2) {
        let n = if up { Vec3::Y } else { -Vec3::Y };
        let cx = outline.iter().map(|p| p.x).sum::<f32>() / outline.len() as f32;
        let cz = outline.iter().map(|p| p.y).sum::<f32>() / outline.len() as f32;
        let c = self.push(vec3(cx, y, cz), n, uv_of(vec2(cx, cz)), color);
        let first = self.vertices.len() as u16;
        for p in outline {
            self.push(vec3(p.x, y, p.y), n, uv_of(*p), color);
        }
        let k = outline.len() as u16;
        for i in 0..k {
            let a = first + i;
            let b = first + (i + 1) % k;
            if up { self.tri(c, b, a) } else { self.tri(c, a, b) }
        }
    }
}

/// Контур скруглённого прямоугольника (по часовой стрелке в XZ).
pub fn rounded_rect(w: f32, h: f32, r: f32, seg: usize) -> Vec<Vec2> {
    let corners = [
        (w / 2.0 - r, h / 2.0 - r, 0.0f32),
        (-w / 2.0 + r, h / 2.0 - r, 90.0),
        (-w / 2.0 + r, -h / 2.0 + r, 180.0),
        (w / 2.0 - r, -h / 2.0 + r, 270.0),
    ];
    let mut pts = Vec::new();
    for (cx, cy, a0) in corners {
        for i in 0..=seg {
            let a = (a0 + 90.0 * i as f32 / seg as f32).to_radians();
            pts.push(vec2(cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    pts
}

/// Боковая стенка выдавленного контура между высотами y0 и y1.
pub fn extrude_sides(m: &mut MeshData, outline: &[Vec2], y0: f32, y1: f32, color: impl Fn(usize) -> Color) {
    let k = outline.len();
    for i in 0..k {
        let a = outline[i];
        let b = outline[(i + 1) % k];
        let edge = b - a;
        let n = vec3(edge.y, 0.0, -edge.x);
        let c = color(i);
        let v0 = m.push(vec3(a.x, y0, a.y), n, Vec2::ZERO, c);
        let v1 = m.push(vec3(b.x, y0, b.y), n, Vec2::ZERO, c);
        let v2 = m.push(vec3(b.x, y1, b.y), n, Vec2::ZERO, c);
        let v3 = m.push(vec3(a.x, y1, a.y), n, Vec2::ZERO, c);
        m.quad(v0, v1, v2, v3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_counts() {
        let m = MeshData::grid(4, 3, |u, v| (vec3(u, 0.0, v), Vec3::Y, vec2(u, v), WHITE));
        assert_eq!(m.vertices.len(), 5 * 4);
        assert_eq!(m.indices.len(), 4 * 3 * 6);
        assert!(m.indices.iter().all(|&i| (i as usize) < m.vertices.len()));
    }

    #[test]
    fn rounded_rect_is_closed_and_bounded() {
        let pts = rounded_rect(2.0, 3.0, 0.2, 4);
        assert_eq!(pts.len(), 4 * 5);
        assert!(pts.iter().all(|p| p.x.abs() <= 1.0 + 1e-5 && p.y.abs() <= 1.5 + 1e-5));
    }
}
