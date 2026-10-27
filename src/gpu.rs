//! gpu.rs — wgpu renderer for the physical dice (WebGPU, explicit WebGL2 fallback). Port of renderer3d.js:
//! lit, textured, beveled meshes on a transparent fullscreen canvas over the CSS table, one cached shadow map
//! plus analytic contact occlusion, and the real table texture reflected in the dice. The table camera and
//! every tray preview render into the whole canvas through a clip-space remap of their client rect, so dice
//! and shadows cross table/tray borders. Renders only when `render` is called: no animation loop of its own.
//! The camera math below is target independent; the renderer itself is wasm only.

use glam::{Mat4, Quat, Vec3, Vec4};

/// Narrow FOV: the camera rises to keep the same framing, so edge dice show less of their sides.
const FOV: f32 = 22.0 * std::f32::consts::PI / 180.0;

/// Client rect in CSS pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

/// Table half extents in world units plus the play range along z (trays may cover part of the table).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub x: f32,
    pub z: f32,
    pub min_z: f32,
    pub max_z: f32,
}

/// A tray die (or a held die off the table) drawn into `rect`, scissored to its scroll container `clip`.
/// `rotation` None: the renderer orients it itself, its value up (a d4 resting on it), the label upright.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preview {
    pub id: u32,
    pub held: bool,
    pub rect: Rect,
    pub clip: Option<Rect>,
    pub rotation: Option<Quat>,
}

/// Table camera straight down with perspective, framing the bounds at y = 0 exactly onto `table`:
/// (camera height, half height framed at y = 0).
fn table_camera(table: Rect, bounds: Bounds) -> (f32, f32) {
    let aspect = table.width / table.height;
    let half = bounds.z.max(bounds.x / aspect);
    (half / (FOV / 2.0).tan(), half)
}

/// Client-pixel center and diameter of a table die's bounding sphere.
pub fn project(table: Rect, bounds: Bounds, pos: Vec3, radius: f32) -> (f32, f32, f32) {
    let (h, _) = table_camera(table, bounds);
    let k = table.height / (2.0 * (FOV / 2.0).tan() * (h - pos.y).max(0.1));
    (
        table.left + table.width / 2.0 + pos.x * k,
        table.top + table.height / 2.0 + pos.z * k,
        2.0 * radius * k,
    )
}

/// World point at `height` under the client pixel.
pub fn unproject(x: f32, y: f32, table: Rect, bounds: Bounds, height: f32) -> Vec3 {
    let (h, _) = table_camera(table, bounds);
    let k = table.height / (2.0 * (FOV / 2.0).tan() * h);
    let f = (h - height) / h;
    Vec3::new(
        (x - table.left - table.width / 2.0) / k * f,
        height,
        (y - table.top - table.height / 2.0) / k * f,
    )
}

/// Left-multiplies a view-projection so a camera framed onto client `rect` renders into the full-canvas
/// viewport `view`: clip x → sx·x + tx·w, y → sy·y + ty·w.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn frame(view: Rect, rect: Rect) -> Mat4 {
    let sx = rect.width / view.width;
    let tx = (2.0 * (rect.left - view.left) + rect.width) / view.width - 1.0;
    let sy = rect.height / view.height;
    let ty = 1.0 - (2.0 * (rect.top - view.top) + rect.height) / view.height;
    Mat4::from_cols(
        Vec4::new(sx, 0.0, 0.0, 0.0),
        Vec4::new(0.0, sy, 0.0, 0.0),
        Vec4::Z,
        Vec4::new(tx, ty, 0.0, 1.0),
    )
}

#[cfg(target_arch = "wasm32")]
pub use web::Renderer;

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{frame, table_camera, Bounds, Preview, Rect, FOV};
    use crate::geometry::{self, Label, Shape, ATLAS_COLS};
    use crate::model::{self, Die, MAX_DICE};
    use crate::physics::Pose;
    use glam::{Mat4, Quat, Vec3};
    use std::collections::HashMap;
    use std::f64::consts::TAU;
    use std::num::NonZeroU64;
    use std::sync::{Arc, Mutex};
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement};
    use wgpu::util::DeviceExt;

    const PREVIEW_FOV: f32 = 28.0 * std::f32::consts::PI / 180.0;
    const PREVIEW_TILT: f32 = 0.3;
    const SHADOW_SIZE: u32 = 1024;
    const MAX_OCC: usize = 12;
    const ATLAS_CACHE: usize = MAX_DICE + 4; // ≥ dice drawn per frame, so per-throw face mappings never thrash
    const GUTTER: f32 = 6.0; // CSS px a tray's scroll clip is grown by, so edges aren't chopped flush
    const FONT: &str = "800 100px system-ui, -apple-system, \"Segoe UI\", sans-serif";
    const PIPS: [&[usize]; 7] = [
        &[],
        &[4],
        &[0, 8],
        &[0, 4, 8],
        &[0, 2, 6, 8],
        &[0, 2, 4, 6, 8],
        &[0, 2, 3, 5, 6, 8],
    ];
    const MODEL_STRIDE: usize = 256; // min_uniform_buffer_offset_alignment upper bound
    const CAMERA_STRIDE: usize = 512;
    const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;
    const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
    const BIAS: Mat4 = Mat4::from_cols_array(&[
        0.5, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5, 0.5, 0.0, 1.0,
    ]);

    // Same colors as style.css: (name, die, pip); skins add roughness, metalness.
    const PLAIN: &[(&str, &str, &str)] = &[
        ("white", "#edebe5", "#171717"),
        ("blue", "#194ed8", "#fff"),
        ("yellow", "#eec20f", "#1f2430"),
        ("green", "#0aa748", "#fff"),
        ("orange", "#e96f0a", "#fff"),
        ("purple", "#7020e4", "#fff"),
        ("red", "#cb131f", "#fff"),
        ("brown", "#805633", "#fff"),
        ("turquoise", "#0cb5ac", "#002826"),
        ("pink", "#e62f8a", "#fff"),
        ("gray", "#343a46", "#fff"),
    ];
    const SKINS: &[(&str, &str, &str, f32, f32)] = &[
        ("opal", "#c0cceb", "#141414", 0.18, 0.0),
        ("wood", "#dd7d36", "#141414", 0.55, 0.0),
        ("galaxy", "#321474", "#e2ac3c", 0.2, 0.0),
        ("jade", "#267947", "#e2ac3c", 0.22, 0.0),
        ("amber", "#e67113", "#141414", 0.15, 0.0),
        ("terrazzo", "#edd7c4", "#141414", 0.38, 0.0),
        ("ruby", "#aa0d0b", "#f2f2f2", 0.12, 0.0),
        ("steel", "#9c9393", "#141414", 0.3, 0.85),
        ("prism", "#c9b8d3", "#b8860b", 0.14, 0.0),
        ("marble", "#32271b", "#e2ac3c", 0.2, 0.0),
        ("ice", "#3a99df", "#141414", 0.1, 0.0),
        ("walnut", "#b4511e", "#1e1410", 0.5, 0.0),
    ];
    // Average colors of the table textures (style.css), used if the image fails to load.
    const SURFACES: &[(&str, &str)] = &[
        ("casino", "#09491f"),
        ("oak", "#8f8071"),
        ("crafts", "#e5d8c4"),
        ("bluewood", "#3a4a57"),
        ("terracotta", "#cf6736"),
    ];

    struct Material {
        die: &'static str,
        pip: &'static str,
        rough: f32,
        metal: f32,
        skin: Option<&'static str>,
    }

    fn material(color: &str) -> Material {
        if let Some(&(skin, die, pip, rough, metal)) = color
            .strip_prefix("skin-")
            .and_then(|n| SKINS.iter().find(|s| s.0 == n))
        {
            return Material {
                die,
                pip,
                rough,
                metal,
                skin: Some(skin),
            };
        }
        let (die, pip) = PLAIN
            .iter()
            .find(|p| p.0 == color)
            .map_or(("#edebe5", "#191919"), |p| (p.1, p.2));
        Material {
            die,
            pip,
            rough: 0.3,
            metal: 0.0,
            skin: None,
        }
    }

    fn hex(s: &str) -> Option<[u8; 3]> {
        let h = s.trim();
        let h = h.strip_prefix('#').unwrap_or(h);
        let at = |i: usize, n: usize| h.get(i..i + n).and_then(|d| u8::from_str_radix(d, 16).ok());
        match h.len() {
            3 => Some([at(0, 1)? * 17, at(1, 1)? * 17, at(2, 1)? * 17]),
            6 => Some([at(0, 2)?, at(2, 2)?, at(4, 2)?]),
            _ => None,
        }
    }

    fn linear(rgb: [u8; 3]) -> [f32; 3] {
        rgb.map(|c| {
            let c = c as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        })
    }

    // 6/9-style values that read as another value upside down get an underline, like real dice.
    fn ambiguous(value: u32, sides: u32) -> bool {
        let s = value.to_string();
        let turned: Option<String> = s
            .chars()
            .rev()
            .map(|c| match c {
                '0' | '1' | '8' => Some(c),
                '6' => Some('9'),
                '9' => Some('6'),
                _ => None,
            })
            .collect();
        turned.is_some_and(|t| {
            t != s && !t.starts_with('0') && t.parse::<u32>().is_ok_and(|n| n >= 1 && n <= sides)
        })
    }

    fn js(e: JsValue) -> String {
        e.as_string()
            .or_else(|| {
                e.dyn_ref::<js_sys::Error>()
                    .map(|e| String::from(e.message()))
            })
            .unwrap_or_else(|| format!("{e:?}"))
    }

    fn canvas2d(
        width: u32,
        height: u32,
    ) -> Result<(HtmlCanvasElement, CanvasRenderingContext2d), String> {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or("No document for the dice textures")?;
        let canvas: HtmlCanvasElement = document
            .create_element("canvas")
            .map_err(js)?
            .dyn_into()
            .map_err(|_| "Not a canvas")?;
        canvas.set_width(width);
        canvas.set_height(height);
        let ctx = canvas
            .get_context("2d")
            .map_err(js)?
            .ok_or("Canvas 2D is not available")?
            .dyn_into::<CanvasRenderingContext2d>()
            .map_err(|_| "Canvas 2D is not available")?;
        Ok((canvas, ctx))
    }

    /// Uploads a canvas as an sRGB texture with a full mip chain, each level halved by Canvas2D.
    fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        canvas: &HtmlCanvasElement,
        ctx: &CanvasRenderingContext2d,
    ) -> Result<wgpu::Texture, String> {
        let (w, h) = (canvas.width().max(1), canvas.height().max(1));
        let levels = 32 - w.max(h).leading_zeros();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dice texture"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let (mut src, mut src_ctx) = (canvas.clone(), ctx.clone());
        for level in 0..levels {
            let (lw, lh) = ((w >> level).max(1), (h >> level).max(1));
            if level > 0 {
                let (next, next_ctx) = canvas2d(lw, lh)?;
                next_ctx
                    .draw_image_with_html_canvas_element_and_dw_and_dh(
                        &src, 0.0, 0.0, lw as f64, lh as f64,
                    )
                    .map_err(js)?;
                (src, src_ctx) = (next, next_ctx);
            }
            let data = src_ctx
                .get_image_data(0.0, 0.0, lw as f64, lh as f64)
                .map_err(js)?
                .data();
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &data.0,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * lw),
                    rows_per_image: Some(lh),
                },
                wgpu::Extent3d {
                    width: lw,
                    height: lh,
                    depth_or_array_layers: 1,
                },
            );
        }
        Ok(texture)
    }

    fn solid(device: &wgpu::Device, queue: &wgpu::Queue, rgb: [u8; 3]) -> wgpu::Texture {
        device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("table color"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &[rgb[0], rgb[1], rgb[2], 255],
        )
    }

    async fn loaded(img: HtmlImageElement, path: &str) -> Result<HtmlImageElement, String> {
        JsFuture::from(img.decode())
            .await
            .map_err(|_| format!("Could not load {path}"))?;
        Ok(img)
    }

    /// Skin art is optional like in the original: a failed image leaves the plain die color.
    async fn optional(img: Option<HtmlImageElement>, path: &str) -> Option<HtmlImageElement> {
        match loaded(img?, path).await {
            Ok(img) => Some(img),
            Err(e) => {
                web_sys::console::warn_1(&e.into());
                None
            }
        }
    }

    // ---- texture atlas (Canvas2D): faces with numbers, pips, story motifs and d4 corner labels ----

    /// Moves the origin to a label (cell y up → canvas y down) and turns its up toward canvas −y.
    fn place(
        ctx: &CanvasRenderingContext2d,
        x0: f64,
        y0: f64,
        c: f64,
        label: &Label,
    ) -> Result<(), String> {
        ctx.set_transform(
            1.0,
            0.0,
            0.0,
            1.0,
            x0 + label.x as f64 * c,
            y0 + (1.0 - label.y as f64) * c,
        )
        .map_err(js)?;
        ctx.rotate((label.up[0] as f64).atan2(label.up[1] as f64))
            .map_err(js)
    }

    /// Draws `text` centered at the origin (y down), fitted into a free circle of `radius` px and no taller
    /// than real dice digits; `cap` is the digit cap height in FONT.
    fn glyphs(
        ctx: &CanvasRenderingContext2d,
        text: &str,
        radius: f64,
        ink: &str,
        underline: bool,
        cap: f64,
    ) -> Result<(), String> {
        ctx.set_font(FONT);
        ctx.set_text_align("center");
        ctx.set_text_baseline("alphabetic");
        let width = ctx.measure_text(text).map_err(js)?.width();
        let s = (0.85 * radius / (width / 2.0).hypot(cap * 0.62)).min(1.1 * radius / cap);
        ctx.scale(s, s).map_err(js)?;
        let y = cap / 2.0;
        ctx.set_fill_style_str("rgba(0,0,0,.28)"); // engraving: dark rim on the upper left
        ctx.fill_text(text, -2.5, y - 2.5).map_err(js)?;
        ctx.set_fill_style_str(ink);
        ctx.fill_text(text, 0.0, y).map_err(js)?;
        if underline {
            ctx.fill_rect(-width * 0.3, y + cap * 0.14, width * 0.6, cap * 0.1);
        }
        Ok(())
    }

    /// The center of the skin's blank face (the baked rounded rim stays outside).
    fn draw_crop(
        ctx: &CanvasRenderingContext2d,
        img: &HtmlImageElement,
        x: f64,
        y: f64,
        size: f64,
    ) -> Result<(), String> {
        let (nw, nh) = (img.natural_width() as f64, img.natural_height() as f64);
        let s = nw.min(nh) * 0.72;
        ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            img,
            (nw - s) / 2.0,
            (nh - s) / 2.0,
            s,
            s,
            x,
            y,
            size,
            size,
        )
        .map_err(js)
    }

    fn draw_pips(
        ctx: &CanvasRenderingContext2d,
        value: u32,
        x0: f64,
        y0: f64,
        c: f64,
        pip: &str,
        img: Option<&HtmlImageElement>,
    ) -> Result<(), String> {
        for &g in PIPS.get(value as usize).copied().unwrap_or(&[]) {
            let (gx, gy) = ((g % 3) as f64, (g / 3) as f64);
            let (x, y) = (x0 + (0.25 + 0.25 * gx) * c, y0 + (0.25 + 0.25 * gy) * c);
            ctx.save();
            ctx.begin_path();
            if let Some(img) = img {
                // Copy the painted pip at the same grid spot of pips.webp (24/50/76 %, see style.css).
                let (r, iw, ih) = (
                    0.118 * c,
                    img.natural_width() as f64,
                    img.natural_height() as f64,
                );
                let sr = 0.115 * iw;
                let (sx, sy) = (iw * (0.24 + 0.26 * gx), ih * (0.24 + 0.26 * gy));
                ctx.arc(x, y, r, 0.0, TAU).map_err(js)?;
                ctx.clip();
                ctx.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    img,
                    sx - sr,
                    sy - sr,
                    sr * 2.0,
                    sr * 2.0,
                    x - r,
                    y - r,
                    r * 2.0,
                    r * 2.0,
                )
                .map_err(js)?;
            } else {
                let r = 0.095 * c;
                ctx.arc(x, y, r, 0.0, TAU).map_err(js)?;
                ctx.set_fill_style_str(pip);
                ctx.fill();
                let shade = ctx.create_linear_gradient(x - r, y - r, x + r, y + r); // drilled: shaded upper wall
                shade.add_color_stop(0.0, "rgba(0,0,0,.4)").map_err(js)?;
                shade.add_color_stop(0.55, "rgba(0,0,0,0)").map_err(js)?;
                shade
                    .add_color_stop(1.0, "rgba(255,255,255,.14)")
                    .map_err(js)?;
                ctx.set_fill_style_canvas_gradient(&shade);
                ctx.fill();
            }
            ctx.restore();
        }
        Ok(())
    }

    /// Rotation of a resting tray die: printed `value` up (a d4 resting on it), its label upright on screen.
    fn rest_rotation(shape: &Shape, values: &[u32], value: u32) -> Quat {
        let shown = |v: u32| values.get(v as usize - 1).copied().unwrap_or(v);
        let Some(face) = shape
            .faces
            .iter()
            .find(|f| shown(f.value) == value)
            .or(shape.faces.first())
        else {
            return Quat::IDENTITY;
        };
        let tilt = Quat::from_rotation_arc(
            Vec3::from(face.normal),
            if shape.bottom_read {
                Vec3::NEG_Y
            } else {
                Vec3::Y
            },
        );
        if shape.bottom_read || face.poly.len() < 2 {
            return tilt;
        }
        // The label's up as dice3d lays it out: a d6 toward its first edge, others toward the farthest corner.
        let poly = || face.poly.iter().map(|&p| Vec3::from(p));
        let c = poly().sum::<Vec3>() / face.poly.len() as f32;
        let toward = if shape.sides == 6 {
            (Vec3::from(face.poly[0]) + Vec3::from(face.poly[1])) / 2.0
        } else {
            poly().fold(c, |best, p| {
                if p.distance(c) > best.distance(c) + 1e-6 {
                    p
                } else {
                    best
                }
            })
        };
        let up = tilt * (toward - c);
        if up.x.abs() + up.z.abs() < 1e-6 {
            return tilt;
        }
        // A turn about y moves direction angle atan2(x, z) by the turn angle; screen up is −z (angle π).
        Quat::from_rotation_y(std::f32::consts::PI - up.x.atan2(up.z)) * tilt
    }

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Globals {
        shadow_mat: [[f32; 4]; 4],
        light_vp: [[f32; 4]; 4],
        key_dir: [f32; 4],
        key_color: [f32; 4],
        fill_dir: [f32; 4],
        fill_color: [f32; 4],
        sky: [f32; 4],
        flags: [f32; 4],
    }

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Camera {
        view_proj: [[f32; 4]; 4],
        eye: [f32; 4],
        env_map: [f32; 4],
        ground: [f32; 4],
        strength: [f32; 4],
        occ: [[f32; 4]; MAX_OCC],
    }

    #[repr(C)]
    #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct Model {
        model: [[f32; 4]; 4],
        mat: [f32; 4],
        highlight: [f32; 4],
    }

    /// GPU geometry of one die type (a pure function of the side count) plus hull planes for picking.
    struct Mesh {
        sides: u32,
        vertices: wgpu::Buffer,
        indices: wgpu::Buffer,
        count: u32,
        planes: Vec<[f32; 4]>,
        occ: f32,
        shape: Shape,
    }

    struct Atlas {
        key: String,
        bind: wgpu::BindGroup,
        rough: f32,
        metal: f32,
    }

    /// A synced die: its mesh, atlas and resting tray orientation.
    struct Item {
        id: u32,
        mesh: usize,
        atlas: wgpu::BindGroup,
        rough: f32,
        metal: f32,
        rest: Quat,
    }

    pub struct Renderer {
        canvas: HtmlCanvasElement,
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
        backend: String,
        error: Arc<Mutex<Option<String>>>,
        samples: u32,
        color: Option<wgpu::TextureView>,
        depth: wgpu::TextureView,
        die_pipeline: wgpu::RenderPipeline,
        ground_pipeline: wgpu::RenderPipeline,
        shadow_pipeline: wgpu::RenderPipeline,
        scene_layout: wgpu::BindGroupLayout,
        atlas_layout: wgpu::BindGroupLayout,
        globals_buf: wgpu::Buffer,
        camera_buf: wgpu::Buffer,
        model_buf: wgpu::Buffer,
        scene_bind: wgpu::BindGroup,
        light_bind: wgpu::BindGroup,
        camera_bind: wgpu::BindGroup,
        model_bind: wgpu::BindGroup,
        shadow_view: wgpu::TextureView,
        env_view: wgpu::TextureView,
        env_sampler: wgpu::Sampler,
        shadow_sampler: wgpu::Sampler,
        atlas_sampler: wgpu::Sampler,
        env_key: String,
        env_size: f32,
        globals: Globals,
        light_bounds: Option<Bounds>,
        shadow_valid: bool,
        shadow_poses: Vec<(u32, Vec3, Quat)>,
        meshes: Vec<Mesh>,
        atlases: Vec<Atlas>,
        items: Vec<Item>,
        images: HashMap<String, HtmlImageElement>,
        atlas_canvas: HtmlCanvasElement,
        atlas_ctx: CanvasRenderingContext2d,
        cap_height: f64,
        selected: Vec<u32>,
        accent: [f32; 3],
        view: Rect,
        // Per-frame scratch, allocated once.
        model_data: Vec<u8>,
        camera_data: Vec<u8>,
        table_draws: Vec<(usize, Vec3, Quat)>,
        preview_draws: Vec<(usize, [u32; 4])>,
    }

    async fn open(
        canvas: &HtmlCanvasElement,
        backends: wgpu::Backends,
    ) -> Result<
        (
            wgpu::Surface<'static>,
            wgpu::Adapter,
            wgpu::Device,
            wgpu::Queue,
        ),
        String,
    > {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends,
            ..Default::default()
        });
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .map_err(|e| e.to_string())?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("dice"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok((surface, adapter, device, queue))
    }

    impl Renderer {
        /// WebGPU when the browser has an adapter, otherwise (or if WebGPU setup fails) WebGL2.
        /// `?gpu=webgl` in the page URL forces WebGL2 for testing the fallback.
        pub async fn new(canvas: HtmlCanvasElement) -> Result<Self, String> {
            let mut canvas = canvas;
            let force_gl = web_sys::window()
                .and_then(|w| w.location().search().ok())
                .and_then(|q| web_sys::UrlSearchParams::new_with_str(&q).ok())
                .and_then(|p| p.get("gpu"))
                .is_some_and(|v| v == "webgl");
            let opened = if !force_gl && wgpu::util::is_browser_webgpu_supported().await {
                match open(&canvas, wgpu::Backends::BROWSER_WEBGPU).await {
                    Ok(opened) => Ok(opened),
                    Err(e) => {
                        web_sys::console::warn_1(
                            &format!("WebGPU failed ({e}), retrying with WebGL2").into(),
                        );
                        // A canvas holding a WebGPU context can't give out WebGL2: swap in a fresh clone.
                        let fresh: HtmlCanvasElement = canvas
                            .clone_node()
                            .map_err(js)?
                            .dyn_into()
                            .map_err(|_| "Not a canvas")?;
                        if let Some(parent) = canvas.parent_node() {
                            parent.replace_child(&fresh, &canvas).map_err(js)?;
                        }
                        canvas = fresh;
                        open(&canvas, wgpu::Backends::GL).await
                    }
                }
            } else {
                open(&canvas, wgpu::Backends::GL).await
            };
            let (surface, adapter, device, queue) =
                opened.map_err(|e| format!("The dice need WebGPU or WebGL2: {e}"))?;

            let error = Arc::new(Mutex::new(None::<String>));
            let sink = error.clone();
            device.on_uncaptured_error(Box::new(move |e: wgpu::Error| {
                if let Ok(mut slot) = sink.lock() {
                    slot.get_or_insert_with(|| format!("Dice GPU error: {e}"));
                }
            }));

            let info = adapter.get_info();
            let api = match info.backend {
                wgpu::Backend::BrowserWebGpu => "WebGPU",
                wgpu::Backend::Gl => "WebGL2",
                _ => "native",
            };
            let backend = if info.name.is_empty() {
                api.to_string()
            } else {
                format!("{api} · {}", info.name)
            };

            let caps = surface.get_capabilities(&adapter);
            let format = caps
                .formats
                .iter()
                .copied()
                .find(|f| !f.is_srgb())
                .or(caps.formats.first().copied())
                .ok_or("The dice canvas has no supported format")?;
            // wgpu's browser backend reports only Opaque, but accepts PreMultiplied.
            // The fullscreen canvas must remain transparent over the table texture.
            let alpha_mode = if info.backend == wgpu::Backend::BrowserWebGpu
                || caps
                    .alpha_modes
                    .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
            {
                wgpu::CompositeAlphaMode::PreMultiplied
            } else {
                caps.alpha_modes
                    .first()
                    .copied()
                    .unwrap_or(wgpu::CompositeAlphaMode::Auto)
            };
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: canvas.width().max(1),
                height: canvas.height().max(1),
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode,
                view_formats: vec![],
            };
            let msaa = |f: wgpu::TextureFormat| {
                adapter
                    .get_texture_format_features(f)
                    .flags
                    .sample_count_supported(4)
            };
            let samples = if msaa(format) && msaa(DEPTH) { 4 } else { 1 };
            let aniso = adapter
                .get_downlevel_capabilities()
                .flags
                .contains(wgpu::DownlevelFlags::ANISOTROPIC_FILTERING);

            // ---- layouts ----
            let uniform = |binding: u32, dynamic: bool, size: usize| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: dynamic,
                    min_binding_size: NonZeroU64::new(size as u64),
                },
                count: None,
            };
            let texture =
                |binding: u32, sample_type: wgpu::TextureSampleType| wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                };
            let sampler =
                |binding: u32, kind: wgpu::SamplerBindingType| wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(kind),
                    count: None,
                };
            let float = wgpu::TextureSampleType::Float { filterable: true };
            let bind_layout = |label: &str, entries: &[wgpu::BindGroupLayoutEntry]| {
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some(label),
                    entries,
                })
            };
            let globals_size = size_of::<Globals>();
            let camera_size = size_of::<Camera>();
            let model_size = size_of::<Model>();
            let scene_layout = bind_layout(
                "dice scene",
                &[
                    uniform(0, false, globals_size),
                    texture(1, float),
                    sampler(2, wgpu::SamplerBindingType::Filtering),
                    texture(3, wgpu::TextureSampleType::Depth),
                    sampler(4, wgpu::SamplerBindingType::Comparison),
                ],
            );
            let light_layout = bind_layout("dice light", &[uniform(0, false, globals_size)]);
            let camera_layout = bind_layout("dice camera", &[uniform(0, true, camera_size)]);
            let model_layout = bind_layout("dice model", &[uniform(0, true, model_size)]);
            let atlas_layout = bind_layout(
                "dice atlas",
                &[
                    texture(0, float),
                    sampler(1, wgpu::SamplerBindingType::Filtering),
                ],
            );

            // ---- pipelines ----
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("dice"),
                source: wgpu::ShaderSource::Wgsl(include_str!("dice.wgsl").into()),
            });
            let pipeline_layout = |groups: &[&wgpu::BindGroupLayout]| {
                device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: groups,
                    push_constant_ranges: &[],
                })
            };
            let attributes =
                wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2];
            let vertex = wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes,
            };
            let shadow_vertex = wgpu::VertexBufferLayout {
                array_stride: 32,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &attributes[..1],
            };
            let multisample = wgpu::MultisampleState {
                count: samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            };
            let depth = |write: bool, compare: wgpu::CompareFunction| {
                Some(wgpu::DepthStencilState {
                    format: DEPTH,
                    depth_write_enabled: write,
                    depth_compare: compare,
                    stencil: Default::default(),
                    bias: Default::default(),
                })
            };
            let die_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("dice"),
                layout: Some(&pipeline_layout(&[
                    &scene_layout,
                    &camera_layout,
                    &model_layout,
                    &atlas_layout,
                ])),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_die"),
                    compilation_options: Default::default(),
                    buffers: &[vertex],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: depth(true, wgpu::CompareFunction::Less),
                multisample,
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_die"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            });
            let ground_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("dice ground"),
                layout: Some(&pipeline_layout(&[&scene_layout, &camera_layout])),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_ground"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: depth(false, wgpu::CompareFunction::Always),
                multisample,
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_ground"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            });
            let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("dice shadow"),
                layout: Some(&pipeline_layout(&[
                    &light_layout,
                    &camera_layout,
                    &model_layout,
                ])),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_shadow"),
                    compilation_options: Default::default(),
                    buffers: &[shadow_vertex],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: SHADOW_FORMAT,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState {
                        constant: 4,
                        slope_scale: 2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: wgpu::MultisampleState::default(),
                fragment: None,
                multiview: None,
                cache: None,
            });

            // ---- buffers, textures, bind groups ----
            let buffer = |label: &str, size: usize| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: size as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            };
            let globals_buf = buffer("dice globals", globals_size);
            let camera_buf = buffer("dice cameras", CAMERA_STRIDE * (MAX_DICE + 1));
            let model_buf = buffer("dice models", MODEL_STRIDE * 2 * MAX_DICE);
            let dynamic = |layout: &wgpu::BindGroupLayout, buf: &wgpu::Buffer, size: usize| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: buf,
                            offset: 0,
                            size: NonZeroU64::new(size as u64),
                        }),
                    }],
                })
            };
            let camera_bind = dynamic(&camera_layout, &camera_buf, camera_size);
            let model_bind = dynamic(&model_layout, &model_buf, model_size);
            let light_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("dice light"),
                layout: &light_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals_buf.as_entire_binding(),
                }],
            });
            let clamp = wgpu::AddressMode::ClampToEdge;
            let linear_desc = wgpu::SamplerDescriptor {
                address_mode_u: clamp,
                address_mode_v: clamp,
                address_mode_w: clamp,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            };
            let env_sampler = device.create_sampler(&linear_desc);
            let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                anisotropy_clamp: if aniso { 8 } else { 1 },
                ..linear_desc.clone()
            });
            let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                mipmap_filter: wgpu::FilterMode::Nearest,
                compare: Some(wgpu::CompareFunction::LessEqual),
                ..linear_desc
            });
            let shadow_view = device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("dice shadow map"),
                    size: wgpu::Extent3d {
                        width: SHADOW_SIZE,
                        height: SHADOW_SIZE,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: SHADOW_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default());
            let env_key = "#12141a".to_string();
            let env_view = solid(&device, &queue, hex(&env_key).unwrap_or_default())
                .create_view(&Default::default());

            let w4 = |v: [f32; 3], w: f32| [v[0], v[1], v[2], w];
            let key = Vec3::new(-0.42, 1.0, -0.62).normalize(); // from the upper left, like the CSS drop shadows
            let fill = Vec3::new(0.7, 0.45, 0.6).normalize();
            let globals = Globals {
                shadow_mat: Mat4::IDENTITY.to_cols_array_2d(),
                light_vp: Mat4::IDENTITY.to_cols_array_2d(),
                key_dir: w4(key.into(), 0.0),
                key_color: [2.8, 2.66, 2.46, 0.0],
                fill_dir: w4(fill.into(), 0.0),
                fill_color: w4(linear([140, 158, 190]).map(|c| c * 0.45), 0.0),
                sky: w4(linear([140, 150, 168]), 0.0),
                flags: [if format.is_srgb() { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
            };
            queue.write_buffer(&globals_buf, 0, bytemuck::bytes_of(&globals));
            let (atlas_canvas, atlas_ctx) = canvas2d(1, 1)?;
            let placeholder = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: DEPTH,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });

            let mut renderer = Self {
                canvas,
                scene_bind: light_bind.clone(), // replaced below once the struct exists
                device,
                queue,
                surface,
                config,
                backend,
                error,
                samples,
                color: None,
                depth: placeholder.create_view(&Default::default()),
                die_pipeline,
                ground_pipeline,
                shadow_pipeline,
                scene_layout,
                atlas_layout,
                globals_buf,
                camera_buf,
                model_buf,
                light_bind,
                camera_bind,
                model_bind,
                shadow_view,
                env_view,
                env_sampler,
                shadow_sampler,
                atlas_sampler,
                env_key,
                env_size: 1.0,
                globals,
                light_bounds: None,
                shadow_valid: false,
                shadow_poses: Vec::with_capacity(MAX_DICE),
                meshes: Vec::new(),
                atlases: Vec::with_capacity(ATLAS_CACHE + 1),
                items: Vec::with_capacity(MAX_DICE),
                images: HashMap::new(),
                atlas_canvas,
                atlas_ctx,
                cap_height: 0.0,
                selected: Vec::with_capacity(MAX_DICE),
                accent: linear([255, 176, 46]),
                view: Rect::default(),
                model_data: vec![0; MODEL_STRIDE * 2 * MAX_DICE],
                camera_data: vec![0; CAMERA_STRIDE * (MAX_DICE + 1)],
                table_draws: Vec::with_capacity(MAX_DICE),
                preview_draws: Vec::with_capacity(MAX_DICE),
            };
            renderer.scene_bind = renderer.scene_bind();
            renderer.configure();
            renderer.check()?;
            Ok(renderer)
        }

        /// "WebGPU · adapter" or "WebGL2 · renderer", for diagnostics.
        pub fn backend(&self) -> &str {
            &self.backend
        }

        /// The first GPU error reported since the last check.
        fn check(&self) -> Result<(), String> {
            match self.error.lock().ok().and_then(|mut e| e.take()) {
                Some(e) => Err(e),
                None => Ok(()),
            }
        }

        fn scene_bind(&self) -> wgpu::BindGroup {
            self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("dice scene"),
                layout: &self.scene_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.globals_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.env_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.env_sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&self.shadow_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&self.shadow_sampler),
                    },
                ],
            })
        }

        /// Applies the surface size and recreates the size-dependent render targets.
        fn configure(&mut self) {
            self.canvas.set_width(self.config.width);
            self.canvas.set_height(self.config.height);
            self.surface.configure(&self.device, &self.config);
            let size = wgpu::Extent3d {
                width: self.config.width,
                height: self.config.height,
                depth_or_array_layers: 1,
            };
            let target = |format, label| {
                self.device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size,
                        mip_level_count: 1,
                        sample_count: self.samples,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&Default::default())
            };
            self.depth = target(DEPTH, "dice depth");
            self.color = (self.samples > 1).then(|| target(self.config.format, "dice msaa"));
        }

        /// Canvas client rect (CSS px) and device pixel ratio; resizes the drawing buffer when it changed.
        pub fn set_view(&mut self, canvas_rect: Rect, dpr: f32) {
            self.view = canvas_rect;
            let max = self.device.limits().max_texture_dimension_2d;
            let w = ((canvas_rect.width * dpr).round() as u32).clamp(1, max);
            let h = ((canvas_rect.height * dpr).round() as u32).clamp(1, max);
            if w != self.config.width || h != self.config.height {
                self.config.width = w;
                self.config.height = h;
                self.configure();
            }
        }

        /// Dice the selection highlights in the accent color.
        pub fn set_selected(&mut self, ids: &[u32]) {
            self.selected.clear();
            self.selected.extend_from_slice(ids);
        }

        /// The table as the dice see it: the real background texture, or the theme's plain color. Also picks
        /// up the CSS --accent for highlights, so call it after the surface/theme attributes changed.
        pub async fn set_surface(&mut self, surface: &str, light: bool) -> Result<(), String> {
            let accent = web_sys::window()
                .and_then(|w| {
                    w.get_computed_style(&w.document()?.document_element()?)
                        .ok()
                        .flatten()
                })
                .and_then(|style| style.get_property_value("--accent").ok())
                .and_then(|v| hex(&v));
            if let Some(accent) = accent {
                self.accent = linear(accent);
            }
            let average = SURFACES.iter().find(|s| s.0 == surface).map(|s| s.1);
            let key = match average {
                Some(_) => surface.to_string(),
                None => (if light { "#eef0f4" } else { "#12141a" }).to_string(),
            };
            if key == self.env_key {
                return Ok(());
            }
            let mut texture = None;
            if let Some(average) = average {
                let path = format!("backgrounds/{surface}.webp");
                let img = self.request(&path);
                match loaded(img, &path).await {
                    Ok(img) => {
                        let (w, h) = (img.natural_width().max(1), img.natural_height().max(1));
                        let (canvas, ctx) = canvas2d(w, h)?;
                        ctx.draw_image_with_html_image_element(&img, 0.0, 0.0)
                            .map_err(js)?;
                        texture =
                            Some((upload(&self.device, &self.queue, &canvas, &ctx)?, w as f32));
                    }
                    Err(e) => {
                        web_sys::console::warn_1(&e.into());
                        texture = Some((
                            solid(&self.device, &self.queue, hex(average).unwrap_or_default()),
                            1.0,
                        ));
                    }
                }
            }
            let (texture, size) = texture.unwrap_or_else(|| {
                (
                    solid(&self.device, &self.queue, hex(&key).unwrap_or_default()),
                    1.0,
                )
            });
            self.env_view = texture.create_view(&Default::default());
            self.env_size = size;
            self.env_key = key;
            self.scene_bind = self.scene_bind();
            self.check()
        }

        /// Starts loading an asset (cached per path); `loaded` awaits it.
        fn request(&mut self, path: &str) -> HtmlImageElement {
            self.images
                .entry(path.to_string())
                .or_insert_with(|| {
                    let img = HtmlImageElement::new().expect("img element");
                    img.set_src(path);
                    img
                })
                .clone()
        }

        fn mesh(&mut self, sides: u32) -> usize {
            if let Some(i) = self.meshes.iter().position(|m| m.sides == sides) {
                return i;
            }
            let shape = geometry::build_die(sides);
            let mut data: Vec<f32> = Vec::with_capacity(shape.positions.len() * 8);
            for ((p, n), uv) in shape.positions.iter().zip(&shape.normals).zip(&shape.uvs) {
                data.extend_from_slice(p);
                data.extend_from_slice(n);
                data.extend_from_slice(uv);
            }
            let init = |label, contents: &[u8], usage| {
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(label),
                        contents,
                        usage,
                    })
            };
            let vertices = init(
                "die vertices",
                bytemuck::cast_slice(&data),
                wgpu::BufferUsages::VERTEX,
            );
            let indices = init(
                "die indices",
                bytemuck::cast_slice(&shape.indices),
                wgpu::BufferUsages::INDEX,
            );
            // Hull planes for exact picking: collision core grown by the bevel margin.
            let planes = shape
                .faces
                .iter()
                .map(|f| {
                    let n = Vec3::from(f.normal);
                    let d = shape
                        .vertices
                        .iter()
                        .map(|&v| n.dot(Vec3::from(v)))
                        .fold(f32::NEG_INFINITY, f32::max);
                    [n.x, n.y, n.z, d + shape.margin]
                })
                .collect();
            self.meshes.push(Mesh {
                sides,
                vertices,
                indices,
                count: shape.indices.len() as u32,
                planes,
                occ: (shape.inradius + shape.radius) / 2.0,
                shape,
            });
            self.meshes.len() - 1
        }

        /// Builds geometry and face atlases for every die (table and trays) once per material and face
        /// mapping; `mappings[id]` lists the printed number per geometric face value (values[k − 1]).
        pub async fn sync(
            &mut self,
            dice: &[Die],
            mappings: &[(u32, Vec<u32>)],
        ) -> Result<(), String> {
            self.check()?;
            self.items.clear();
            self.shadow_valid = false;
            for die in dice {
                let mesh = self.mesh(die.sides);
                let values: &[u32] = mappings
                    .iter()
                    .find(|m| m.0 == die.id)
                    .map_or(&[][..], |m| m.1.as_slice());
                // A story id on a d6 overrides color/skin/numbered with the fixed motif material.
                let story = die
                    .story
                    .as_deref()
                    .filter(|s| die.sides == 6 && model::is_story(s));
                let numbered = die.sides == 6 && die.numbered;
                let order = values
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                let key = match story {
                    Some(s) => format!("story|{s}|{order}"),
                    None => format!("{}|{}|{}|{}", die.sides, die.color, numbered, order),
                };
                let atlas = match self.atlases.iter().position(|a| a.key == key) {
                    Some(i) => self.atlases.remove(i), // re-pushed below: least recently used goes first
                    None => {
                        self.build_atlas(mesh, &die.color, numbered, story, values, key)
                            .await?
                    }
                };
                self.items.push(Item {
                    id: die.id,
                    mesh,
                    atlas: atlas.bind.clone(),
                    rough: atlas.rough,
                    metal: atlas.metal,
                    rest: rest_rotation(&self.meshes[mesh].shape, values, die.value),
                });
                self.atlases.push(atlas);
                if self.atlases.len() > ATLAS_CACHE {
                    self.atlases.remove(0);
                }
            }
            self.check()
        }

        /// One square cell per face (uniform scale, flat y up) plus the edge cell = die color.
        async fn build_atlas(
            &mut self,
            mesh: usize,
            color: &str,
            numbered: bool,
            story: Option<&str>,
            values: &[u32],
            key: String,
        ) -> Result<Atlas, String> {
            let sides = self.meshes[mesh].sides;
            let mat = material(if story.is_some() { "white" } else { color }); // story dice: white, the SVGs carry their ink
            let uses_pips = sides == 6 && !numbered && story.is_none();
            // Request every image first so they load in parallel.
            let mut motifs = Vec::new();
            if let Some(story) = story {
                let mut pending = Vec::with_capacity(6);
                for v in 1..=6 {
                    let (id, _) = model::story_face(story, v)
                        .ok_or_else(|| format!("Unknown story die {story}"))?;
                    let path = format!("icons/story/{id}.svg");
                    pending.push((self.request(&path), path));
                }
                for (img, path) in pending {
                    motifs.push(loaded(img, &path).await?);
                }
            }
            let (face_path, pips_path) = match mat.skin {
                Some(skin) => (
                    format!("skins/{skin}/face.webp"),
                    format!("skins/{skin}/pips.webp"),
                ),
                None => (String::new(), String::new()),
            };
            let face_req = mat.skin.map(|_| self.request(&face_path));
            let pips_req = (mat.skin.is_some() && uses_pips).then(|| self.request(&pips_path));
            let face_img = optional(face_req, &face_path).await;
            let pips_img = optional(pips_req, &pips_path).await;

            let ctx = &self.atlas_ctx;
            if self.cap_height == 0.0 {
                ctx.set_font(FONT);
                let ascent = ctx
                    .measure_text("0")
                    .map_err(js)?
                    .actual_bounding_box_ascent();
                self.cap_height = if ascent > 0.0 { ascent } else { 72.0 };
            }
            let cap = self.cap_height;
            let shape = &self.meshes[mesh].shape;
            let shown = |v: u32| values.get(v as usize - 1).copied().unwrap_or(v);
            let c = if shape.faces.len() > 24 { 128.0 } else { 256.0 };
            self.atlas_canvas.set_width(ATLAS_COLS * c as u32);
            self.atlas_canvas.set_height(shape.rows * c as u32); // resizing also resets the context state
            ctx.set_fill_style_str(mat.die);
            ctx.fill_rect(
                0.0,
                0.0,
                (ATLAS_COLS * c as u32) as f64,
                (shape.rows * c as u32) as f64,
            );
            for face in &shape.faces {
                let (x0, y0) = (
                    (face.cell % ATLAS_COLS) as f64 * c,
                    (face.cell / ATLAS_COLS) as f64 * c,
                );
                ctx.save();
                ctx.begin_path();
                ctx.rect(x0, y0, c, c);
                ctx.clip();
                if let Some(img) = &face_img {
                    draw_crop(ctx, img, x0, y0, c)?;
                }
                if story.is_some() {
                    // The motif per shown value, drawn upright like a d6 digit (16 % inset per side).
                    if let (Some(img), Some(label)) = (
                        motifs.get(shown(face.value) as usize - 1),
                        face.labels.first(),
                    ) {
                        place(ctx, x0, y0, c, label)?;
                        ctx.draw_image_with_html_image_element_and_dw_and_dh(
                            img,
                            -0.34 * c,
                            -0.34 * c,
                            0.68 * c,
                            0.68 * c,
                        )
                        .map_err(js)?;
                    }
                } else if uses_pips {
                    draw_pips(
                        ctx,
                        shown(face.value),
                        x0,
                        y0,
                        c,
                        mat.pip,
                        pips_img.as_ref(),
                    )?;
                } else {
                    // d4: each corner shows the value of the face opposite it, upright toward it.
                    for label in &face.labels {
                        let value = shown(label.value);
                        place(ctx, x0, y0, c, label)?;
                        glyphs(
                            ctx,
                            &value.to_string(),
                            label.size as f64 * c,
                            mat.pip,
                            ambiguous(value, sides),
                            cap,
                        )?;
                    }
                }
                ctx.restore();
            }
            let view = upload(&self.device, &self.queue, &self.atlas_canvas, ctx)?
                .create_view(&Default::default());
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("dice atlas"),
                layout: &self.atlas_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.atlas_sampler),
                    },
                ],
            });
            Ok(Atlas {
                key,
                bind,
                rough: mat.rough,
                metal: mat.metal,
            })
        }

        /// Nearest table die whose hull the view ray through the client pixel hits.
        pub fn raycast(
            &self,
            x: f32,
            y: f32,
            table: Rect,
            bounds: Bounds,
            poses: &[Pose],
        ) -> Option<u32> {
            let (h, _) = table_camera(table, bounds);
            let g = super::unproject(x, y, table, bounds, 0.0);
            let dir = Vec3::new(g.x, -h, g.z).normalize();
            let mut best = None;
            let mut best_t = f32::INFINITY;
            for pose in poses {
                let Some(item) = self.items.iter().find(|i| i.id == pose.id) else {
                    continue;
                };
                // Ray into the body frame.
                let inv = pose.rotation.inverse();
                let (o, d) = (inv * (Vec3::new(0.0, h, 0.0) - pose.position), inv * dir);
                let (mut t0, mut t1) = (0.0f32, f32::INFINITY);
                for p in &self.meshes[item.mesh].planes {
                    let n = Vec3::new(p[0], p[1], p[2]);
                    let (denom, num) = (n.dot(d), p[3] - n.dot(o));
                    if denom.abs() < 1e-9 {
                        if num < 0.0 {
                            t0 = f32::INFINITY;
                            break;
                        }
                        continue;
                    }
                    let t = num / denom;
                    if denom < 0.0 {
                        t0 = t0.max(t)
                    } else {
                        t1 = t1.min(t)
                    }
                }
                if t0 <= t1 && t0 < best_t {
                    best_t = t0;
                    best = Some(pose.id);
                }
            }
            best
        }

        /// Ortho light frustum around the table box (dice fly up to y ≈ 4).
        fn update_light(&mut self, b: Bounds) {
            let view = Mat4::look_at_rh(
                Vec3::from_slice(&self.globals.key_dir[..3]) * 20.0,
                Vec3::ZERO,
                Vec3::NEG_Z,
            );
            let (mut lo, mut hi) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
            for x in [-b.x - 1.0, b.x + 1.0] {
                for y in [0.0, 4.0] {
                    for z in [b.min_z - 1.0, b.max_z + 1.0] {
                        let v = view.transform_point3(Vec3::new(x, y, z));
                        lo = lo.min(v);
                        hi = hi.max(v);
                    }
                }
            }
            // Far plane margin: the ground behind a die flying at y = 4 lies ≤ 5 units further along the light.
            let light =
                Mat4::orthographic_rh(lo.x, hi.x, lo.y, hi.y, -hi.z - 1.0, -lo.z + 3.0) * view;
            self.globals.light_vp = light.to_cols_array_2d();
            self.globals.shadow_mat = (BIAS * light).to_cols_array_2d();
            self.queue
                .write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&self.globals));
            self.light_bounds = Some(b);
            self.shadow_valid = false;
        }

        /// Table texture coordinates for world xz, given the client point under the world origin and the
        /// scale (px per unit): CSS draws the table with `center / cover` over the viewport.
        fn env_map(&self, cx: f32, cy: f32, k: f32) -> [f32; 4] {
            let v = self.view;
            let s = v.width.max(v.height);
            [
                k / s,
                (cx - v.left - (v.width - s) / 2.0) / s,
                (cy - v.top - (v.height - s) / 2.0) / s,
                self.env_size,
            ]
        }

        /// Scissor (x, y, w, h in buffer px) to the scroll clip grown by GUTTER ∩ canvas; None if empty.
        fn scissor(&self, clip: Option<Rect>) -> Option<[u32; 4]> {
            let (w, h) = (self.config.width as f32, self.config.height as f32);
            let Some(c) = clip else {
                return Some([0, 0, w as u32, h as u32]);
            };
            let (sx, sy, v) = (w / self.view.width, h / self.view.height, self.view);
            let x0 = ((c.left - GUTTER - v.left) * sx).floor().max(0.0);
            let x1 = ((c.left + c.width + GUTTER - v.left) * sx).ceil().min(w);
            let y0 = ((c.top - GUTTER - v.top) * sy).floor().max(0.0);
            let y1 = ((c.top + c.height + GUTTER - v.top) * sy).ceil().min(h);
            (x1 > x0 && y1 > y0).then(|| [x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
        }

        fn highlight(&self, id: u32) -> [f32; 4] {
            if self.selected.contains(&id) {
                [self.accent[0], self.accent[1], self.accent[2], 0.6]
            } else {
                [0.0; 4]
            }
        }

        fn write_model(&mut self, slot: usize, item: usize, rotation: Quat, position: Vec3) {
            let it = &self.items[item];
            let model = Model {
                model: Mat4::from_rotation_translation(rotation, position).to_cols_array_2d(),
                mat: [it.rough, it.metal, 0.0, 0.0],
                highlight: self.highlight(it.id),
            };
            let at = slot * MODEL_STRIDE;
            self.model_data[at..at + size_of::<Model>()]
                .copy_from_slice(bytemuck::bytes_of(&model));
        }

        fn write_camera(&mut self, slot: usize, camera: &Camera) {
            let at = slot * CAMERA_STRIDE;
            self.camera_data[at..at + size_of::<Camera>()]
                .copy_from_slice(bytemuck::bytes_of(camera));
        }

        /// Draws the whole frame: table dice framed onto `table` (skipped if it is empty), then each tray preview.
        pub fn render(
            &mut self,
            table: Rect,
            bounds: Bounds,
            poses: &[Pose],
            previews: &[Preview],
        ) -> Result<(), String> {
            self.check()?;
            if self.view.width <= 0.0 || self.view.height <= 0.0 {
                return Ok(());
            }
            let frame_tex = match self.surface.get_current_texture() {
                Ok(f) => f,
                Err(wgpu::SurfaceError::Timeout) => return Ok(()),
                Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                    self.configure();
                    self.surface
                        .get_current_texture()
                        .map_err(|e| format!("Dice canvas unavailable: {e}"))?
                }
                Err(e) => return Err(format!("Dice canvas unavailable: {e}")),
            };
            let view = self.view;
            let has_table = table.width > 0.0 && table.height > 0.0;

            // ---- table: model slots 0.., camera slot 0 ----
            self.table_draws.clear();
            if has_table {
                if self.light_bounds != Some(bounds) {
                    self.update_light(bounds);
                }
                for pose in poses {
                    if self.table_draws.len() == MAX_DICE {
                        break;
                    }
                    if let Some(i) = self.items.iter().position(|it| it.id == pose.id) {
                        self.table_draws.push((i, pose.position, pose.rotation));
                    }
                }
                let mut occ = [[0.0; 4]; MAX_OCC];
                for k in 0..self.table_draws.len() {
                    let (i, pos, rot) = self.table_draws[k];
                    self.write_model(k, i, rot, pos);
                    occ[k] = [pos.x, pos.y, pos.z, self.meshes[self.items[i].mesh].occ];
                }
                let (h, half) = table_camera(table, bounds);
                let proj = Mat4::perspective_rh(
                    FOV,
                    table.width / table.height,
                    (h - 8.0).max(0.1),
                    h + 1.0,
                );
                let look = Mat4::look_at_rh(Vec3::new(0.0, h, 0.0), Vec3::ZERO, Vec3::NEG_Z);
                // The ground quad covers the whole canvas (plus a margin), not just the physical bounds.
                let k = table.height / (2.0 * half);
                let (cx, cy) = (
                    table.left + table.width / 2.0 - view.left,
                    table.top + table.height / 2.0 - view.top,
                );
                let camera = Camera {
                    view_proj: (frame(view, table) * proj * look).to_cols_array_2d(),
                    eye: [0.0, h, 0.0, 1.0],
                    env_map: self.env_map(cx + view.left, cy + view.top, k),
                    ground: [
                        (cx.max(view.width - cx) + 8.0) / k,
                        (cy.max(view.height - cy) + 8.0) / k,
                        0.0,
                        self.table_draws.len() as f32,
                    ],
                    strength: [0.5, 0.5, 0.0, 0.0],
                    occ,
                };
                self.write_camera(0, &camera);
            }

            // ---- previews: model slots MAX_DICE.., camera slots 1.. ----
            self.preview_draws.clear();
            for p in previews {
                if self.preview_draws.len() == MAX_DICE
                    || p.rect.width <= 0.0
                    || p.rect.height <= 0.0
                {
                    continue;
                }
                let (Some(i), Some(scissor)) = (
                    self.items.iter().position(|it| it.id == p.id),
                    self.scissor(p.clip),
                ) else {
                    continue;
                };
                let n = self.preview_draws.len();
                let mesh = &self.meshes[self.items[i].mesh];
                let (shape, rot) = (&mesh.shape, p.rotation.unwrap_or(self.items[i].rest));
                // The die rests on its lowest point (lifted while held), seen slightly from the front.
                let r = shape.radius;
                let low = shape
                    .vertices
                    .iter()
                    .map(|&v| (rot * Vec3::from(v)).y)
                    .fold(f32::INFINITY, f32::min)
                    - shape.margin;
                let y = -low + if p.held { r * 0.6 } else { 0.0 };
                let dist = r * 1.1 / (PREVIEW_FOV / 2.0).sin();
                let eye = Vec3::new(
                    0.0,
                    y + dist * PREVIEW_TILT.cos(),
                    dist * PREVIEW_TILT.sin(),
                );
                let proj = Mat4::perspective_rh(
                    PREVIEW_FOV,
                    p.rect.width / p.rect.height,
                    dist * 0.5,
                    dist * 2.0 + r * 4.0,
                );
                let look = Mat4::look_at_rh(eye, Vec3::new(0.0, y, 0.0), Vec3::NEG_Z);
                let mut occ = [[0.0; 4]; MAX_OCC];
                occ[0] = [0.0, y, 0.0, mesh.occ];
                let k = p.rect.height / (2.0 * dist * (PREVIEW_FOV / 2.0).tan());
                let camera = Camera {
                    view_proj: (frame(view, p.rect) * proj * look).to_cols_array_2d(),
                    eye: [eye.x, eye.y, eye.z, 0.0],
                    env_map: self.env_map(
                        p.rect.left + p.rect.width / 2.0,
                        p.rect.top + p.rect.height / 2.0,
                        k,
                    ),
                    ground: [r * 3.5, r * 3.5, 1.0, 1.0],
                    strength: [0.45, 0.5, 0.0, 0.0],
                    occ,
                };
                self.write_camera(n + 1, &camera);
                self.write_model(MAX_DICE + n, i, rot, Vec3::new(0.0, y, 0.0));
                self.preview_draws.push((i, scissor));
            }
            self.queue
                .write_buffer(&self.model_buf, 0, &self.model_data);
            self.queue
                .write_buffer(&self.camera_buf, 0, &self.camera_data);

            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("dice"),
                });

            // ---- shadow map of the table dice from the key light; redrawn only when a die moved ----
            let unchanged = self.shadow_valid
                && self.shadow_poses.len() == self.table_draws.len()
                && self
                    .shadow_poses
                    .iter()
                    .zip(&self.table_draws)
                    .all(|(a, b)| a.0 == self.items[b.0].id && a.1 == b.1 && a.2 == b.2);
            if has_table && !unchanged {
                self.shadow_poses.clear();
                self.shadow_poses.extend(
                    self.table_draws
                        .iter()
                        .map(|&(i, pos, rot)| (self.items[i].id, pos, rot)),
                );
                self.shadow_valid = true;
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("dice shadow"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.shadow_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.shadow_pipeline);
                pass.set_bind_group(0, &self.light_bind, &[]);
                pass.set_bind_group(1, &self.camera_bind, &[0]);
                for (k, &(i, ..)) in self.table_draws.iter().enumerate() {
                    let mesh = &self.meshes[self.items[i].mesh];
                    pass.set_bind_group(2, &self.model_bind, &[(k * MODEL_STRIDE) as u32]);
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
            }

            {
                let target = frame_tex.texture.create_view(&Default::default());
                let (attachment, resolve, store) = match &self.color {
                    Some(msaa) => (msaa, Some(&target), wgpu::StoreOp::Store),
                    None => (&target, None, wgpu::StoreOp::Store),
                };
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("dice"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: attachment,
                        depth_slice: None,
                        resolve_target: resolve,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Discard,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                let (w, h) = (self.config.width, self.config.height);
                let draw = |pass: &mut wgpu::RenderPass<'_>, item: usize, model_slot: usize| {
                    let it = &self.items[item];
                    let mesh = &self.meshes[it.mesh];
                    pass.set_bind_group(2, &self.model_bind, &[(model_slot * MODEL_STRIDE) as u32]);
                    pass.set_bind_group(3, &it.atlas, &[]);
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                };
                // Draw shadows before dice so a preview shadow cannot darken table dice.
                pass.set_pipeline(&self.ground_pipeline);
                pass.set_bind_group(0, &self.scene_bind, &[]);
                for (k, &(_, [x, y, sw, sh])) in self.preview_draws.iter().enumerate() {
                    pass.set_scissor_rect(x, y, sw, sh);
                    pass.set_bind_group(1, &self.camera_bind, &[((k + 1) * CAMERA_STRIDE) as u32]);
                    pass.draw(0..4, 0..1);
                }
                pass.set_scissor_rect(0, 0, w, h);
                if has_table {
                    pass.set_bind_group(1, &self.camera_bind, &[0]);
                    pass.draw(0..4, 0..1);
                    pass.set_pipeline(&self.die_pipeline);
                    for (k, &(i, ..)) in self.table_draws.iter().enumerate() {
                        draw(&mut pass, i, k);
                    }
                }
                drop(pass);
                // Separate passes preserve table color on WebGL2 and isolate each preview's depth.
                for (k, &(i, [x, y, sw, sh])) in self.preview_draws.iter().enumerate() {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("dice preview"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: attachment,
                            depth_slice: None,
                            resolve_target: resolve,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store,
                            },
                        })],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                            view: &self.depth,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: wgpu::StoreOp::Discard,
                            }),
                            stencil_ops: None,
                        }),
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_pipeline(&self.die_pipeline);
                    pass.set_scissor_rect(x, y, sw, sh);
                    pass.set_bind_group(0, &self.scene_bind, &[]);
                    pass.set_bind_group(1, &self.camera_bind, &[((k + 1) * CAMERA_STRIDE) as u32]);
                    draw(&mut pass, i, MAX_DICE + k);
                }
            }
            self.queue.submit([encoder.finish()]);
            frame_tex.present();
            self.check()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_and_unproject_agree_on_the_table() {
        let table = Rect {
            left: 10.0,
            top: 40.0,
            width: 390.0,
            height: 600.0,
        };
        let bounds = Bounds {
            x: 2.7,
            z: 4.2,
            min_z: -4.2,
            max_z: 4.2,
        };
        let p = unproject(123.0, 456.0, table, bounds, 0.0);
        let (x, y, _) = project(table, bounds, p, 0.5);
        assert!((x - 123.0).abs() < 1e-3 && (y - 456.0).abs() < 1e-3);
        // The table bounds land on the rect edges.
        let (_, top, _) = project(table, bounds, Vec3::new(0.0, 0.0, -bounds.z), 0.5);
        assert!((top - table.top).abs() < 1e-3);
    }
}
