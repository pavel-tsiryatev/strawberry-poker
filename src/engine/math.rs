//! Линейная алгебра движка: векторы, матрицы 4×4 (column-major, как в OpenGL),
//! кватернионы и цвет. Без внешних зависимостей.

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

// ------------------------------------------------------------------ Vec2

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

pub const fn vec2(x: f32, y: f32) -> Vec2 {
    Vec2 { x, y }
}

impl Vec2 {
    pub const ZERO: Vec2 = vec2(0.0, 0.0);

    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn lerp(self, to: Vec2, t: f32) -> Vec2 {
        self + (to - self) * t
    }

    pub fn dot(self, o: Vec2) -> f32 {
        self.x * o.x + self.y * o.y
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        vec2(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        vec2(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f32) -> Vec2 {
        vec2(self.x * k, self.y * k)
    }
}

// ------------------------------------------------------------------ Vec3

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

pub const fn vec3(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3 { x, y, z }
}

impl Vec3 {
    pub const ZERO: Vec3 = vec3(0.0, 0.0, 0.0);
    pub const ONE: Vec3 = vec3(1.0, 1.0, 1.0);
    pub const X: Vec3 = vec3(1.0, 0.0, 0.0);
    pub const Y: Vec3 = vec3(0.0, 1.0, 0.0);
    pub const Z: Vec3 = vec3(0.0, 0.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Vec3 {
        vec3(x, y, z)
    }

    pub const fn splat(v: f32) -> Vec3 {
        vec3(v, v, v)
    }

    pub fn dot(self, o: Vec3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    pub fn cross(self, o: Vec3) -> Vec3 {
        vec3(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    pub fn normalize(self) -> Vec3 {
        self / self.length()
    }

    pub fn normalize_or_zero(self) -> Vec3 {
        let l = self.length();
        if l > 1e-12 { self / l } else { Vec3::ZERO }
    }

    pub fn lerp(self, to: Vec3, t: f32) -> Vec3 {
        self + (to - self) * t
    }

    pub fn mul_elem(self, o: Vec3) -> Vec3 {
        vec3(self.x * o.x, self.y * o.y, self.z * o.z)
    }

    pub fn extend(self, w: f32) -> Vec4 {
        vec4(self.x, self.y, self.z, w)
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    fn add(self, o: Vec3) -> Vec3 {
        vec3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl AddAssign for Vec3 {
    fn add_assign(&mut self, o: Vec3) {
        *self = *self + o;
    }
}
impl Sub for Vec3 {
    type Output = Vec3;
    fn sub(self, o: Vec3) -> Vec3 {
        vec3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl SubAssign for Vec3 {
    fn sub_assign(&mut self, o: Vec3) {
        *self = *self - o;
    }
}
impl Mul<f32> for Vec3 {
    type Output = Vec3;
    fn mul(self, k: f32) -> Vec3 {
        vec3(self.x * k, self.y * k, self.z * k)
    }
}
impl Mul<Vec3> for f32 {
    type Output = Vec3;
    fn mul(self, v: Vec3) -> Vec3 {
        v * self
    }
}
impl Div<f32> for Vec3 {
    type Output = Vec3;
    fn div(self, k: f32) -> Vec3 {
        vec3(self.x / k, self.y / k, self.z / k)
    }
}
impl Neg for Vec3 {
    type Output = Vec3;
    fn neg(self) -> Vec3 {
        vec3(-self.x, -self.y, -self.z)
    }
}

// ------------------------------------------------------------------ Vec4

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec4 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

pub const fn vec4(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
    Vec4 { x, y, z, w }
}

impl Vec4 {
    pub fn truncate(self) -> Vec3 {
        vec3(self.x, self.y, self.z)
    }
    pub fn to_array(self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }
}

// ------------------------------------------------------------------ Mat4

/// Матрица 4×4, хранится по столбцам: `cols[столбец][строка]`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct Mat4 {
    pub cols: [[f32; 4]; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Mat4::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4 {
        cols: [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]],
    };

    pub fn translation(t: Vec3) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.cols[3] = [t.x, t.y, t.z, 1.0];
        m
    }

    pub fn scale(s: Vec3) -> Mat4 {
        let mut m = Mat4::IDENTITY;
        m.cols[0][0] = s.x;
        m.cols[1][1] = s.y;
        m.cols[2][2] = s.z;
        m
    }

    pub fn rotation_x(a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        let mut m = Mat4::IDENTITY;
        m.cols[1] = [0.0, c, s, 0.0];
        m.cols[2] = [0.0, -s, c, 0.0];
        m
    }

    pub fn rotation_y(a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        let mut m = Mat4::IDENTITY;
        m.cols[0] = [c, 0.0, -s, 0.0];
        m.cols[2] = [s, 0.0, c, 0.0];
        m
    }

    pub fn rotation_z(a: f32) -> Mat4 {
        let (s, c) = a.sin_cos();
        let mut m = Mat4::IDENTITY;
        m.cols[0] = [c, s, 0.0, 0.0];
        m.cols[1] = [-s, c, 0.0, 0.0];
        m
    }

    /// Перспектива для OpenGL (глубина в клип-пространстве −1..1).
    pub fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
        let f = 1.0 / (fovy / 2.0).tan();
        let nf = 1.0 / (near - far);
        Mat4 {
            cols: [
                [f / aspect, 0.0, 0.0, 0.0],
                [0.0, f, 0.0, 0.0],
                [0.0, 0.0, (far + near) * nf, -1.0],
                [0.0, 0.0, 2.0 * far * near * nf, 0.0],
            ],
        }
    }

    pub fn orthographic(l: f32, r: f32, b: f32, t: f32, near: f32, far: f32) -> Mat4 {
        Mat4 {
            cols: [
                [2.0 / (r - l), 0.0, 0.0, 0.0],
                [0.0, 2.0 / (t - b), 0.0, 0.0],
                [0.0, 0.0, -2.0 / (far - near), 0.0],
                [-(r + l) / (r - l), -(t + b) / (t - b), -(far + near) / (far - near), 1.0],
            ],
        }
    }

    /// Матрица вида (правосторонняя система, камера смотрит вдоль −Z).
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Mat4 {
        let f = (target - eye).normalize();
        let s = f.cross(up).normalize();
        let u = s.cross(f);
        Mat4 {
            cols: [
                [s.x, u.x, -f.x, 0.0],
                [s.y, u.y, -f.y, 0.0],
                [s.z, u.z, -f.z, 0.0],
                [-s.dot(eye), -u.dot(eye), f.dot(eye), 1.0],
            ],
        }
    }

    pub fn mul_vec4(&self, v: Vec4) -> Vec4 {
        let c = &self.cols;
        vec4(
            c[0][0] * v.x + c[1][0] * v.y + c[2][0] * v.z + c[3][0] * v.w,
            c[0][1] * v.x + c[1][1] * v.y + c[2][1] * v.z + c[3][1] * v.w,
            c[0][2] * v.x + c[1][2] * v.y + c[2][2] * v.z + c[3][2] * v.w,
            c[0][3] * v.x + c[1][3] * v.y + c[2][3] * v.z + c[3][3] * v.w,
        )
    }

    pub fn transform_point(&self, p: Vec3) -> Vec3 {
        let v = self.mul_vec4(p.extend(1.0));
        v.truncate() / v.w
    }

    pub fn transform_vector(&self, d: Vec3) -> Vec3 {
        self.mul_vec4(d.extend(0.0)).truncate()
    }
}

impl Mul for Mat4 {
    type Output = Mat4;
    fn mul(self, o: Mat4) -> Mat4 {
        let mut r = [[0.0f32; 4]; 4];
        for (c, col) in r.iter_mut().enumerate() {
            let v = self.mul_vec4(vec4(o.cols[c][0], o.cols[c][1], o.cols[c][2], o.cols[c][3]));
            *col = v.to_array();
        }
        Mat4 { cols: r }
    }
}

// ------------------------------------------------------------------ Quat

/// Единичный кватернион поворота.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Quat = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };

    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Quat {
        let a = axis.normalize();
        let (s, c) = (angle / 2.0).sin_cos();
        Quat { x: a.x * s, y: a.y * s, z: a.z * s, w: c }
    }

    pub fn normalize(self) -> Quat {
        let l = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt();
        Quat { x: self.x / l, y: self.y / l, z: self.z / l, w: self.w / l }
    }

    /// Сферическая интерполяция.
    pub fn slerp(self, mut to: Quat, t: f32) -> Quat {
        let mut d = self.x * to.x + self.y * to.y + self.z * to.z + self.w * to.w;
        if d < 0.0 {
            d = -d;
            to = Quat { x: -to.x, y: -to.y, z: -to.z, w: -to.w };
        }
        let (k0, k1) = if d > 0.9995 {
            (1.0 - t, t)
        } else {
            let th = d.acos();
            let s = th.sin();
            (((1.0 - t) * th).sin() / s, (t * th).sin() / s)
        };
        Quat {
            x: self.x * k0 + to.x * k1,
            y: self.y * k0 + to.y * k1,
            z: self.z * k0 + to.z * k1,
            w: self.w * k0 + to.w * k1,
        }
        .normalize()
    }

    pub fn to_mat4(self) -> Mat4 {
        let Quat { x, y, z, w } = self;
        let (xx, yy, zz) = (x * x, y * y, z * z);
        let (xy, xz, yz) = (x * y, x * z, y * z);
        let (wx, wy, wz) = (w * x, w * y, w * z);
        Mat4 {
            cols: [
                [1.0 - 2.0 * (yy + zz), 2.0 * (xy + wz), 2.0 * (xz - wy), 0.0],
                [2.0 * (xy - wz), 1.0 - 2.0 * (xx + zz), 2.0 * (yz + wx), 0.0],
                [2.0 * (xz + wy), 2.0 * (yz - wx), 1.0 - 2.0 * (xx + yy), 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
}

impl Mul for Quat {
    type Output = Quat;
    fn mul(self, o: Quat) -> Quat {
        Quat {
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
        }
    }
}

// ------------------------------------------------------------------ Color

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

pub const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);
pub const BLACK: Color = Color::new(0.0, 0.0, 0.0, 1.0);

impl Color {
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Color {
        Color { r, g, b, a }
    }

    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
        Color::new(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0)
    }

    pub fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        (a - b).length() < 1e-4
    }

    #[test]
    fn rotations_and_quats_agree() {
        let p = vec3(1.0, 2.0, 3.0);
        for a in [0.3f32, 1.2, -2.0] {
            let m = Mat4::rotation_y(a);
            let q = Quat::from_axis_angle(Vec3::Y, a).to_mat4();
            assert!(close(m.transform_point(p), q.transform_point(p)));
            let m = Mat4::rotation_z(a);
            let q = Quat::from_axis_angle(Vec3::Z, a).to_mat4();
            assert!(close(m.transform_point(p), q.transform_point(p)));
            let m = Mat4::rotation_x(a);
            let q = Quat::from_axis_angle(Vec3::X, a).to_mat4();
            assert!(close(m.transform_point(p), q.transform_point(p)));
        }
        // поворот на 90° вокруг Y переводит X в −Z
        assert!(close(Mat4::rotation_y(std::f32::consts::FRAC_PI_2).transform_point(Vec3::X), -Vec3::Z));
    }

    #[test]
    fn look_at_and_perspective() {
        let view = Mat4::look_at(vec3(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        // цель оказывается на оси −Z камеры
        assert!(close(view.transform_point(Vec3::ZERO), vec3(0.0, 0.0, -5.0)));
        let proj = Mat4::perspective(1.0, 1.0, 1.0, 10.0);
        let near = proj.transform_point(vec3(0.0, 0.0, -1.0));
        let far = proj.transform_point(vec3(0.0, 0.0, -10.0));
        assert!((near.z + 1.0).abs() < 1e-4 && (far.z - 1.0).abs() < 1e-4);
    }

    #[test]
    fn matrix_product_composes() {
        let a = Mat4::translation(vec3(1.0, 0.0, 0.0));
        let b = Mat4::scale(Vec3::splat(2.0));
        // сначала масштаб, потом сдвиг
        assert!(close((a * b).transform_point(Vec3::ONE), vec3(3.0, 2.0, 2.0)));
        let q0 = Quat::IDENTITY;
        let q1 = Quat::from_axis_angle(Vec3::Y, 1.0);
        let half = q0.slerp(q1, 0.5).to_mat4();
        assert!(close(half.transform_point(Vec3::X), Mat4::rotation_y(0.5).transform_point(Vec3::X)));
    }
}
