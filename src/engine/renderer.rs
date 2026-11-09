//! GPU-рендерер поверх miniquad (только окно и GL-контекст):
//! свои шейдеры, проход теней в карту глубины, освещённая 3D-сцена, 2D-интерфейс.

use miniquad::*;

use super::canvas::{ATLAS, Batch, Canvas, UiVertex};
use super::math::*;
use super::mesh::{MeshData, Vertex};

pub const SHADOW_SIZE: u32 = 2048;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct MeshId(usize);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TexId(pub(crate) TextureId);

/// Свойства поверхности.
#[derive(Clone, Copy, Debug)]
pub struct Material {
    /// сила блика
    pub specular: f32,
    pub shininess: f32,
    /// без освещения (светится сам)
    pub unlit: bool,
    pub cast_shadow: bool,
    pub receive_shadow: bool,
    /// аддитивное смешивание (свечение неона), без записи глубины
    pub additive: bool,
}

impl Material {
    pub const MATTE: Material = Material {
        specular: 0.05,
        shininess: 8.0,
        unlit: false,
        cast_shadow: true,
        receive_shadow: true,
        additive: false,
    };
    pub const GLOSSY: Material = Material { specular: 0.35, shininess: 48.0, ..Material::MATTE };
    pub const UNLIT: Material = Material {
        specular: 0.0,
        shininess: 1.0,
        unlit: true,
        cast_shadow: false,
        receive_shadow: false,
        additive: false,
    };
    pub const GLOW: Material = Material { additive: true, ..Material::UNLIT };

    pub fn no_shadow(self) -> Material {
        Material { cast_shadow: false, ..self }
    }
}

/// Команда отрисовки 3D-объекта.
#[derive(Clone, Copy, Debug)]
pub struct DrawCmd {
    pub mesh: MeshId,
    pub model: Mat4,
    pub texture: Option<TexId>,
    /// (смещение u, смещение v, масштаб u, масштаб v) — для атласов
    pub uv_rect: [f32; 4],
    pub tint: Color,
    pub material: Material,
}

#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fovy: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera {
    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        Mat4::perspective(self.fovy, aspect, self.near, self.far) * Mat4::look_at(self.position, self.target, self.up)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PointLight {
    pub pos: Vec3,
    pub radius: f32,
    pub color: Color,
}

/// Освещение сцены: главный свет (даёт тени), фон и два точечных источника.
#[derive(Clone, Copy, Debug)]
pub struct Light {
    pub dir: Vec3,
    pub color: Color,
    pub ambient: Color,
    /// центр и полуразмеры области, которую покрывает карта теней
    pub center: Vec3,
    pub half_w: f32,
    pub half_h: f32,
    /// эллипс «пятна» главного света (полуоси по X и Z), вне его темнеет
    pub spot: Vec2,
    pub points: [PointLight; 2],
}

impl Light {
    fn view_proj(&self) -> Mat4 {
        let d = self.dir.normalize();
        let eye = self.center - d * 30.0;
        Mat4::orthographic(-self.half_w, self.half_w, -self.half_h, self.half_h, 1.0, 60.0)
            * Mat4::look_at(eye, self.center, Vec3::Z)
    }
}

struct GpuMesh {
    vb: BufferId,
    ib: BufferId,
    count: i32,
}

#[repr(C)]
struct LitUniforms {
    mvp: Mat4,
    model: Mat4,
    light_vp: Mat4,
    light_dir: [f32; 4],
    cam_pos: [f32; 4],
    material: [f32; 4],
    tint: [f32; 4],
    uv_rect: [f32; 4],
    params: [f32; 4],
    light_color: [f32; 4],
    ambient: [f32; 4],
    p0_pos: [f32; 4],
    p0_color: [f32; 4],
    p1_pos: [f32; 4],
    p1_color: [f32; 4],
}

#[repr(C)]
struct ShadowUniforms {
    light_mvp: Mat4,
}

#[repr(C)]
struct UiUniforms {
    proj: Mat4,
}

pub struct Renderer {
    pub ctx: Box<dyn RenderingBackend>,
    lit: Pipeline,
    lit_add: Pipeline,
    shadow: Pipeline,
    ui: Pipeline,
    shadow_pass: RenderPass,
    shadow_tex: TextureId,
    meshes: Vec<GpuMesh>,
    white: TextureId,
    pub atlas: TexId,
    ui_vb: BufferId,
    ui_ib: BufferId,
}

const LIT_VS: &str = r#"#version 100
attribute vec3 in_pos;
attribute vec3 in_normal;
attribute vec2 in_uv;
attribute vec4 in_color;
uniform mat4 mvp;
uniform mat4 model;
uniform mat4 light_vp;
uniform vec4 tint;
uniform vec4 uv_rect;
varying vec3 v_pos;
varying vec3 v_normal;
varying vec2 v_uv;
varying vec4 v_color;
varying vec4 v_light;
void main() {
    vec4 wp = model * vec4(in_pos, 1.0);
    v_pos = wp.xyz;
    v_normal = (model * vec4(in_normal, 0.0)).xyz;
    v_uv = in_uv * uv_rect.zw + uv_rect.xy;
    v_color = in_color * tint;
    v_light = light_vp * wp;
    gl_Position = mvp * vec4(in_pos, 1.0);
}
"#;

const LIT_FS: &str = r#"#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
varying vec3 v_pos;
varying vec3 v_normal;
varying vec2 v_uv;
varying vec4 v_color;
varying vec4 v_light;
uniform sampler2D tex;
uniform sampler2D shadow_map;
uniform vec4 light_dir;
uniform vec4 cam_pos;
uniform vec4 material; // x: блик, y: глянец, z: принимает тень, w: без освещения
uniform vec4 params;   // x: размер текселя карты теней, yz: полуоси пятна света
uniform vec4 light_color;
uniform vec4 ambient;
uniform vec4 p0_pos;   // xyz: позиция, w: радиус
uniform vec4 p0_color; // rgb: цвет, a: сила
uniform vec4 p1_pos;
uniform vec4 p1_color;

vec3 point_light(vec4 lp, vec4 lc, vec3 n, vec3 base) {
    vec3 d = lp.xyz - v_pos;
    float dist = length(d);
    float att = clamp(1.0 - dist / lp.w, 0.0, 1.0);
    att *= att;
    float k = max(dot(n, d / dist), 0.0) * 0.8 + 0.2;
    return base * lc.rgb * (lc.a * att * k);
}

float unpack_depth(vec4 c) {
    return dot(c, vec4(1.0, 1.0 / 255.0, 1.0 / 65025.0, 1.0 / 16581375.0));
}

float shadow_factor(vec3 n, vec3 l) {
    vec3 sc = v_light.xyz / v_light.w * 0.5 + 0.5;
    if (sc.x <= 0.0 || sc.x >= 1.0 || sc.y <= 0.0 || sc.y >= 1.0 || sc.z >= 1.0) {
        return 1.0;
    }
    float bias = max(0.0025 * (1.0 - dot(n, l)), 0.0008);
    float lit = 0.0;
    // PCF 3×3 — мягкие края теней
    for (int i = -1; i <= 1; i++) {
        for (int j = -1; j <= 1; j++) {
            vec2 off = vec2(float(i), float(j)) * params.x;
            float d = unpack_depth(texture2D(shadow_map, sc.xy + off));
            lit += (sc.z - bias > d) ? 0.0 : 1.0;
        }
    }
    return lit / 9.0;
}

void main() {
    vec4 base = texture2D(tex, v_uv) * v_color;
    if (material.w > 0.5) {
        gl_FragColor = base;
        return;
    }
    vec3 n = normalize(v_normal);
    if (!gl_FrontFacing) { n = -n; }
    vec3 l = normalize(-light_dir.xyz);
    vec3 v = normalize(cam_pos.xyz - v_pos);
    vec3 h = normalize(l + v);
    float diff = max(dot(n, l), 0.0);
    float spec = pow(max(dot(n, h), 0.0), material.y) * material.x;
    float sh = material.z > 0.5 ? shadow_factor(n, l) : 1.0;
    // пятно главного света: к краям комнаты темнеет
    vec2 q = v_pos.xz / params.yz;
    float spot = clamp(1.25 - 0.75 * dot(q, q), 0.0, 1.25);
    float rim = pow(1.0 - max(dot(n, v), 0.0), 3.0) * 0.10;
    vec3 col = base.rgb * ambient.rgb
             + base.rgb * light_color.rgb * diff * (0.25 + 0.75 * sh) * spot
             + light_color.rgb * spec * sh * spot
             + point_light(p0_pos, p0_color, n, base.rgb)
             + point_light(p1_pos, p1_color, n, base.rgb)
             + base.rgb * rim;
    gl_FragColor = vec4(col, base.a);
}
"#;

const SHADOW_VS: &str = r#"#version 100
attribute vec3 in_pos;
uniform mat4 light_mvp;
void main() {
    gl_Position = light_mvp * vec4(in_pos, 1.0);
}
"#;

const SHADOW_FS: &str = r#"#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
// глубина упаковывается в RGBA8: depth-текстуры есть не на всех телефонах
vec4 pack_depth(float d) {
    vec4 enc = fract(vec4(1.0, 255.0, 65025.0, 16581375.0) * d);
    enc -= enc.yzww * vec4(1.0 / 255.0, 1.0 / 255.0, 1.0 / 255.0, 0.0);
    return enc;
}
void main() {
    gl_FragColor = pack_depth(gl_FragCoord.z);
}
"#;

const UI_VS: &str = r#"#version 100
attribute vec2 in_pos;
attribute vec2 in_uv;
attribute vec4 in_color;
uniform mat4 proj;
varying lowp vec4 color;
varying vec2 uv;
void main() {
    gl_Position = proj * vec4(in_pos, 0.0, 1.0);
    color = in_color;
    uv = in_uv;
}
"#;

const UI_FS: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
varying vec2 uv;
uniform sampler2D tex;
void main() {
    gl_FragColor = color * texture2D(tex, uv);
}
"#;

fn mat_uniform(name: &str) -> UniformDesc {
    UniformDesc::new(name, UniformType::Mat4)
}
fn vec_uniform(name: &str) -> UniformDesc {
    UniformDesc::new(name, UniformType::Float4)
}

fn alpha_blend() -> (Option<BlendState>, Option<BlendState>) {
    (
        Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        )),
        // альфа кадра остаётся 1 — окно не становится полупрозрачным
        Some(BlendState::new(Equation::Add, BlendFactor::One, BlendFactor::OneMinusValue(BlendValue::SourceAlpha))),
    )
}

impl Renderer {
    pub fn new() -> Renderer {
        let mut ctx = window::new_rendering_backend();

        let vertex_attrs = [
            VertexAttribute::new("in_pos", VertexFormat::Float3),
            VertexAttribute::new("in_normal", VertexFormat::Float3),
            VertexAttribute::new("in_uv", VertexFormat::Float2),
            VertexAttribute::new("in_color", VertexFormat::Float4),
        ];
        let (blend, ablend) = alpha_blend();

        let lit_shader = ctx
            .new_shader(
                ShaderSource::Glsl { vertex: LIT_VS, fragment: LIT_FS },
                ShaderMeta {
                    images: vec!["tex".into(), "shadow_map".into()],
                    uniforms: UniformBlockLayout {
                        uniforms: vec![
                            mat_uniform("mvp"),
                            mat_uniform("model"),
                            mat_uniform("light_vp"),
                            vec_uniform("light_dir"),
                            vec_uniform("cam_pos"),
                            vec_uniform("material"),
                            vec_uniform("tint"),
                            vec_uniform("uv_rect"),
                            vec_uniform("params"),
                            vec_uniform("light_color"),
                            vec_uniform("ambient"),
                            vec_uniform("p0_pos"),
                            vec_uniform("p0_color"),
                            vec_uniform("p1_pos"),
                            vec_uniform("p1_color"),
                        ],
                    },
                },
            )
            .unwrap_or_else(|e| panic!("шейдер освещения: {}", e));
        let lit = ctx.new_pipeline(
            &[BufferLayout::default()],
            &vertex_attrs,
            lit_shader,
            PipelineParams {
                depth_test: Comparison::LessOrEqual,
                depth_write: true,
                color_blend: blend,
                alpha_blend: ablend,
                ..Default::default()
            },
        );

        let lit_add = ctx.new_pipeline(
            &[BufferLayout::default()],
            &vertex_attrs,
            lit_shader,
            PipelineParams {
                depth_test: Comparison::LessOrEqual,
                depth_write: false,
                color_blend: Some(BlendState::new(
                    Equation::Add,
                    BlendFactor::Value(BlendValue::SourceAlpha),
                    BlendFactor::One,
                )),
                alpha_blend: Some(BlendState::new(Equation::Add, BlendFactor::Zero, BlendFactor::One)),
                ..Default::default()
            },
        );

        let shadow_shader = ctx
            .new_shader(
                ShaderSource::Glsl { vertex: SHADOW_VS, fragment: SHADOW_FS },
                ShaderMeta {
                    images: vec![],
                    uniforms: UniformBlockLayout { uniforms: vec![mat_uniform("light_mvp")] },
                },
            )
            .unwrap_or_else(|e| panic!("шейдер теней: {}", e));
        let shadow = ctx.new_pipeline(
            &[BufferLayout::default()],
            &vertex_attrs,
            shadow_shader,
            PipelineParams { depth_test: Comparison::LessOrEqual, depth_write: true, ..Default::default() },
        );

        let ui_shader = ctx
            .new_shader(
                ShaderSource::Glsl { vertex: UI_VS, fragment: UI_FS },
                ShaderMeta {
                    images: vec!["tex".into()],
                    uniforms: UniformBlockLayout { uniforms: vec![mat_uniform("proj")] },
                },
            )
            .unwrap_or_else(|e| panic!("шейдер интерфейса: {}", e));
        let ui = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[
                VertexAttribute::new("in_pos", VertexFormat::Float2),
                VertexAttribute::new("in_uv", VertexFormat::Float2),
                VertexAttribute::new("in_color", VertexFormat::Float4),
            ],
            ui_shader,
            PipelineParams { color_blend: blend, alpha_blend: ablend, ..Default::default() },
        );

        // карта теней: цвет (упакованная глубина) + буфер глубины
        let shadow_tex = ctx.new_render_texture(TextureParams {
            width: SHADOW_SIZE,
            height: SHADOW_SIZE,
            format: TextureFormat::RGBA8,
            min_filter: FilterMode::Nearest,
            mag_filter: FilterMode::Nearest,
            ..Default::default()
        });
        let shadow_depth = ctx.new_render_texture(TextureParams {
            width: SHADOW_SIZE,
            height: SHADOW_SIZE,
            format: TextureFormat::Depth,
            ..Default::default()
        });
        let shadow_pass = ctx.new_render_pass(shadow_tex, Some(shadow_depth));

        let white = ctx.new_texture_from_rgba8(1, 1, &[255, 255, 255, 255]);
        let atlas = ctx.new_texture(
            TextureAccess::Static,
            TextureSource::Bytes(&vec![0u8; ATLAS * ATLAS * 4]),
            TextureParams {
                width: ATLAS as u32,
                height: ATLAS as u32,
                format: TextureFormat::RGBA8,
                min_filter: FilterMode::Linear,
                mag_filter: FilterMode::Linear,
                ..Default::default()
            },
        );
        let ui_vb = ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Stream, BufferSource::empty::<UiVertex>(60_000));
        let ui_ib = ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Stream, BufferSource::empty::<u16>(240_000));

        Renderer {
            ctx,
            lit,
            lit_add,
            shadow,
            ui,
            shadow_pass,
            shadow_tex,
            meshes: Vec::new(),
            white,
            atlas: TexId(atlas),
            ui_vb,
            ui_ib,
        }
    }

    pub fn upload_mesh(&mut self, m: &MeshData) -> MeshId {
        let vb = self.ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable, BufferSource::slice(&m.vertices));
        let ib = self.ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(&m.indices));
        self.meshes.push(GpuMesh { vb, ib, count: m.indices.len() as i32 });
        MeshId(self.meshes.len() - 1)
    }

    /// Текстура из RGBA-пикселей; `repeat` — для тайлящихся материалов.
    pub fn texture(&mut self, w: u32, h: u32, rgba: &[u8], repeat: bool, mipmaps: bool) -> TexId {
        let t = self.ctx.new_texture(
            TextureAccess::Static,
            TextureSource::Bytes(rgba),
            TextureParams {
                width: w,
                height: h,
                format: TextureFormat::RGBA8,
                wrap: if repeat { TextureWrap::Repeat } else { TextureWrap::Clamp },
                min_filter: FilterMode::Linear,
                mag_filter: FilterMode::Linear,
                mipmap_filter: if mipmaps { MipmapFilterMode::Linear } else { MipmapFilterMode::None },
                allocate_mipmaps: mipmaps,
                ..Default::default()
            },
        );
        if mipmaps {
            self.ctx.texture_generate_mipmaps(t);
        }
        TexId(t)
    }

    /// Отрисовать 2D-пакет в новую текстуру (например, атлас карт).
    pub fn bake(&mut self, canvas: &mut Canvas, layer: usize, w: u32, h: u32) -> TexId {
        let tex = self.ctx.new_render_texture(TextureParams {
            width: w,
            height: h,
            format: TextureFormat::RGBA8,
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::Linear,
            allocate_mipmaps: true,
            ..Default::default()
        });
        let pass = self.ctx.new_render_pass(tex, None);
        self.upload_atlas(canvas);
        self.ctx.begin_pass(Some(pass), PassAction::clear_color(0.0, 0.0, 0.0, 0.0));
        // y=0 кладём в первую строку текстуры (v=0)
        let proj = Mat4::orthographic(0.0, w as f32, 0.0, h as f32, -1.0, 1.0);
        self.draw_batch(&canvas.layers[layer], proj);
        self.ctx.end_render_pass();
        self.ctx.texture_generate_mipmaps(tex);
        TexId(tex)
    }

    fn upload_atlas(&mut self, canvas: &mut Canvas) {
        for u in canvas.uploads.drain(..) {
            self.ctx.texture_update_part(self.atlas.0, u.x, u.y, u.w, u.h, &u.rgba);
        }
    }

    fn draw_batch(&mut self, batch: &Batch, proj: Mat4) {
        if batch.is_empty() {
            return;
        }
        self.ctx.apply_pipeline(&self.ui);
        for (verts, idx) in &batch.chunks {
            if idx.is_empty() {
                continue;
            }
            self.ctx.buffer_update(self.ui_vb, BufferSource::slice(verts));
            self.ctx.buffer_update(self.ui_ib, BufferSource::slice(idx));
            self.ctx.apply_bindings(&Bindings {
                vertex_buffers: vec![self.ui_vb],
                index_buffer: self.ui_ib,
                images: vec![self.atlas.0],
            });
            self.ctx.apply_uniforms(UniformsSource::table(&UiUniforms { proj }));
            self.ctx.draw(0, idx.len() as i32, 1);
        }
    }

    /// Полный кадр: тени → фон (2D) → 3D-сцена → интерфейс (2D).
    pub fn frame(
        &mut self,
        canvas: &mut Canvas,
        clear: Color,
        camera: Option<&Camera>,
        light: &Light,
        cmds: &[DrawCmd],
    ) {
        let (w, h) = window::screen_size();
        self.upload_atlas(canvas);
        let light_vp = light.view_proj();

        // 1. карта теней
        if camera.is_some() {
            self.ctx.begin_pass(
                Some(self.shadow_pass),
                PassAction::Clear { color: Some((1.0, 1.0, 1.0, 1.0)), depth: Some(1.0), stencil: None },
            );
            self.ctx.apply_pipeline(&self.shadow);
            for c in cmds.iter().filter(|c| c.material.cast_shadow) {
                let m = &self.meshes[c.mesh.0];
                self.ctx.apply_bindings(&Bindings { vertex_buffers: vec![m.vb], index_buffer: m.ib, images: vec![] });
                self.ctx.apply_uniforms(UniformsSource::table(&ShadowUniforms { light_mvp: light_vp * c.model }));
                self.ctx.draw(0, m.count, 1);
            }
            self.ctx.end_render_pass();
        }

        // 2. экран
        self.ctx.begin_default_pass(PassAction::Clear {
            color: Some((clear.r, clear.g, clear.b, 1.0)),
            depth: Some(1.0),
            stencil: None,
        });
        let ui_proj = Mat4::orthographic(0.0, w, h, 0.0, -1.0, 1.0);
        if let Some(bg) = canvas.layers.first() {
            self.draw_batch(bg, ui_proj);
        }

        if let Some(cam) = camera {
            let vp = cam.view_proj(w / h.max(1.0));
            let ldir = light.dir.normalize();
            let pl = |p: &PointLight| ([p.pos.x, p.pos.y, p.pos.z, p.radius.max(0.01)], p.color.to_array());
            let (p0p, p0c) = pl(&light.points[0]);
            let (p1p, p1c) = pl(&light.points[1]);
            let mut additive = None;
            for c in cmds {
                if additive != Some(c.material.additive) {
                    additive = Some(c.material.additive);
                    self.ctx.apply_pipeline(if c.material.additive { &self.lit_add } else { &self.lit });
                }
                let m = &self.meshes[c.mesh.0];
                let tex = c.texture.map(|t| t.0).unwrap_or(self.white);
                self.ctx.apply_bindings(&Bindings {
                    vertex_buffers: vec![m.vb],
                    index_buffer: m.ib,
                    images: vec![tex, self.shadow_tex],
                });
                let mat = c.material;
                self.ctx.apply_uniforms(UniformsSource::table(&LitUniforms {
                    mvp: vp * c.model,
                    model: c.model,
                    light_vp,
                    light_dir: [ldir.x, ldir.y, ldir.z, 0.0],
                    cam_pos: [cam.position.x, cam.position.y, cam.position.z, 1.0],
                    material: [
                        mat.specular,
                        mat.shininess,
                        if mat.receive_shadow { 1.0 } else { 0.0 },
                        if mat.unlit { 1.0 } else { 0.0 },
                    ],
                    tint: c.tint.to_array(),
                    uv_rect: c.uv_rect,
                    params: [1.0 / SHADOW_SIZE as f32, light.spot.x, light.spot.y, 0.0],
                    light_color: light.color.to_array(),
                    ambient: light.ambient.to_array(),
                    p0_pos: p0p,
                    p0_color: p0c,
                    p1_pos: p1p,
                    p1_color: p1c,
                }));
                self.ctx.draw(0, m.count, 1);
            }
        }

        for layer in canvas.layers.iter().skip(1) {
            self.draw_batch(layer, ui_proj);
        }
        self.ctx.end_render_pass();
        self.ctx.commit_frame();
    }

    /// Снимок экрана в PPM (для отладки на машинах без дисплея).
    pub fn screenshot(&mut self, path: &str) {
        let (w, h) = window::screen_size();
        let (w, h) = (w as usize, h as usize);
        let mut px = vec![0u8; w * h * 4];
        unsafe {
            miniquad::gl::glReadPixels(
                0,
                0,
                w as i32,
                h as i32,
                miniquad::gl::GL_RGBA,
                miniquad::gl::GL_UNSIGNED_BYTE,
                px.as_mut_ptr() as *mut _,
            );
        }
        let mut out = format!("P6\n{} {}\n255\n", w, h).into_bytes();
        for y in (0..h).rev() {
            for x in 0..w {
                let i = (y * w + x) * 4;
                out.extend_from_slice(&px[i..i + 3]);
            }
        }
        let _ = std::fs::write(path, out);
    }
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

// Vertex используется только как сырые байты для GPU
const _: () = assert!(std::mem::size_of::<Vertex>() == 48);
