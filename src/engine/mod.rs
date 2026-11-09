//! Собственный 3D-движок Strawberry Poker.
//!
//! От miniquad берутся только окно, ввод и GL-контекст. Всё остальное своё:
//! математика, меши, шейдеры с освещением и тенями, 2D-холст с атласом шрифтов,
//! процедурные текстуры.

pub mod canvas;
pub mod input;
pub mod math;
pub mod mesh;
pub mod renderer;
pub mod texgen;

pub mod prelude {
    pub use super::input::*;
    pub use super::math::*;
    pub use super::mesh::*;
    pub use super::renderer::{Camera, DrawCmd, Light, Material, MeshId, PointLight, Renderer, TexId};
}
