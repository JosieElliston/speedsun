//! GPU rendering of the puzzle with wgpu.
//!
//! The frame is composed exactly like the old CPU (`euc`) renderer: a base
//! pass of opaque stickers, then up to three overlay layers — the
//! filter-translucent stickers, the blocked-twist red flash, and the twist
//! gizmos — each rendered opaquely into an overlay texture over its own depth
//! setup (so per pixel only the layer's nearest surface shows, with no
//! fog-like buildup) and composited onto the scene with HSC2 stylized
//! transparency (https://ajfarkas.dev/blog/hsc2-transparency/).

use eframe::{egui, egui_wgpu, wgpu, wgpu::util::DeviceExt};

/// egui requires native textures to be Rgba8Unorm. That's also the right
/// choice here: the shaders emit premultiplied gamma-space colors, so all
/// blending happens in gamma space, matching the old Color32 compositing.
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A sticker-pass vertex: a clip-space position (orthographic, w = 1, z in
/// [0, 1] with smaller nearer) plus the sticker's style and the per-vertex
/// edge distances for outline drawing (see `push_fan` in puzzle_view).
/// Colors are straight-alpha; the fragment shader premultiplies.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 4],
    pub face: [f32; 4],
    pub outline: [f32; 4],
    /// x: outline width in pixels; y, z, w: pixel-space distances to the
    /// containing triangle's edges.
    pub width_edges: [f32; 4],
}

/// One frame's geometry, in composition order.
pub struct FrameInput<'a> {
    /// target size in physical pixels.
    pub size: [u32; 2],
    pub backface_culling: bool,
    /// opaque stickers.
    pub base: &'a [Vertex],
    /// filter-translucent stickers: depth-tested against the base pass and
    /// composited by their own per-pixel alpha.
    pub translucent: &'a [Vertex],
    /// pure-red copies of the blocked pieces' triangles, drawn over a fresh
    /// depth buffer (on top of everything) at `flash_strength`.
    pub flash: &'a [Vertex],
    pub flash_strength: f32,
    /// twist gizmo faces, never backface-culled, at `gizmo_strength`.
    pub gizmos: &'a [Vertex],
    pub gizmo_strength: f32,
}

/// index into `strengths` / `Targets::bind_groups` for each overlay layer.
const TRANSLUCENT: usize = 0;
const FLASH: usize = 1;
const GIZMO: usize = 2;

pub struct GpuRenderer {
    render_state: egui_wgpu::RenderState,
    sticker_cull: wgpu::RenderPipeline,
    sticker_nocull: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    /// uniform buffers holding each overlay layer's composite strength. the
    /// translucent slot stays 1.0 (its per-pixel alpha does the work); the
    /// flash and gizmo slots are rewritten every frame.
    strengths: [wgpu::Buffer; 3],
    /// the size-dependent resources, recreated when the viewport resizes.
    targets: Option<Targets>,
}

struct Targets {
    size: [u32; 2],
    scene: wgpu::TextureView,
    overlay: wgpu::TextureView,
    depth: wgpu::TextureView,
    /// composite bind groups (overlay texture + strength), one per layer.
    bind_groups: [wgpu::BindGroup; 3],
    /// the scene texture's egui id, stable across resizes.
    egui_id: egui::TextureId,
}

impl GpuRenderer {
    pub fn new(render_state: egui_wgpu::RenderState) -> Self {
        let device = &render_state.device;

        let sticker_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sticker"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/sticker.wgsl").into()),
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composite"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/composite.wgsl").into()),
        });

        let vertex_attrs = wgpu::vertex_attr_array![
            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4
        ];
        // culling is the only difference between the two sticker pipelines.
        // vertices are counterclockwise (in y-up clip space, wgpu's Ccw) as
        // seen from outside their piece, same as the picking in `bary_z`.
        let sticker_pipeline = |cull_mode: Option<wgpu::Face>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sticker"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &sticker_shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &vertex_attrs,
                    }],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &sticker_shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    // no blending: each pass overwrites, like the CPU
                    // rasterizer; transparency is done by the composites.
                    targets: &[Some(COLOR_FORMAT.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let sticker_cull = sticker_pipeline(Some(wgpu::Face::Back));
        let sticker_nocull = sticker_pipeline(None);

        let composite = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composite"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &composite_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &composite_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let strengths = [(); 3].map(|()| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("composite strength"),
                contents: bytemuck::cast_slice(&[1.0f32, 0.0, 0.0, 0.0]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
        });

        Self {
            render_state,
            sticker_cull,
            sticker_nocull,
            composite,
            strengths,
            targets: None,
        }
    }

    fn ensure_targets(&mut self, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let device = &self.render_state.device;
        let texture_view = |label, format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size[0],
                        height: size[1],
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let attach_and_bind =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let scene = texture_view("scene", COLOR_FORMAT, attach_and_bind);
        let overlay = texture_view("overlay", COLOR_FORMAT, attach_and_bind);
        let depth = texture_view(
            "depth",
            DEPTH_FORMAT,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );

        let layout = self.composite.get_bind_group_layout(0);
        let bind_groups = [TRANSLUCENT, FLASH, GIZMO].map(|slot| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("composite"),
                layout: &layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&overlay),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.strengths[slot].as_entire_binding(),
                    },
                ],
            })
        });

        // nearest filtering: the texture is drawn 1:1 at physical resolution.
        let mut renderer = self.render_state.renderer.write();
        let egui_id = match self.targets.take() {
            Some(old) => {
                renderer.update_egui_texture_from_wgpu_texture(
                    device,
                    &scene,
                    wgpu::FilterMode::Nearest,
                    old.egui_id,
                );
                old.egui_id
            }
            None => renderer.register_native_texture(device, &scene, wgpu::FilterMode::Nearest),
        };
        self.targets = Some(Targets {
            size,
            scene,
            overlay,
            depth,
            bind_groups,
            egui_id,
        });
    }

    /// render a frame into the scene texture and return its egui texture id.
    pub fn render(&mut self, input: &FrameInput) -> egui::TextureId {
        self.ensure_targets(input.size);
        let targets = self.targets.as_ref().unwrap();
        let device = &self.render_state.device;
        let queue = &self.render_state.queue;

        for (slot, strength) in [(FLASH, input.flash_strength), (GIZMO, input.gizmo_strength)] {
            queue.write_buffer(
                &self.strengths[slot],
                0,
                bytemuck::cast_slice(&[strength, 0.0, 0.0, 0.0]),
            );
        }

        let vertex_buffer = |vertices: &[Vertex]| {
            (!vertices.is_empty()).then(|| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("stickers"),
                    contents: bytemuck::cast_slice(vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                })
            })
        };
        let base_buf = vertex_buffer(input.base);
        let translucent_buf = vertex_buffer(input.translucent);
        let flash_buf = vertex_buffer(input.flash);
        let gizmo_buf = vertex_buffer(input.gizmos);

        let culled = if input.backface_culling {
            &self.sticker_cull
        } else {
            &self.sticker_nocull
        };

        let mut encoder = device.create_command_encoder(&Default::default());

        // base pass: opaque stickers. always runs, clearing the scene texture
        // and the depth buffer.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("base"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.scene,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            if let Some(buf) = &base_buf {
                pass.set_pipeline(culled);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..input.base.len() as u32, 0..1);
            }
        }

        // translucent stickers keep the base pass's depth buffer, so opaque
        // geometry in front still occludes them; flash and gizmos clear it,
        // putting them on top of everything.
        if let Some(buf) = &translucent_buf {
            let n = input.translucent.len() as u32;
            self.overlay_layer(
                &mut encoder,
                culled,
                buf,
                n,
                wgpu::LoadOp::Load,
                TRANSLUCENT,
            );
        }
        if let Some(buf) = &flash_buf {
            let n = input.flash.len() as u32;
            self.overlay_layer(
                &mut encoder,
                culled,
                buf,
                n,
                wgpu::LoadOp::Clear(1.0),
                FLASH,
            );
        }
        if let Some(buf) = &gizmo_buf {
            // never cull the gizmos: their backfaces are visible and clickable.
            let n = input.gizmos.len() as u32;
            let pipeline = &self.sticker_nocull;
            self.overlay_layer(
                &mut encoder,
                pipeline,
                buf,
                n,
                wgpu::LoadOp::Clear(1.0),
                GIZMO,
            );
        }

        queue.submit([encoder.finish()]);
        targets.egui_id
    }

    /// render one overlay layer opaquely into the overlay texture, then
    /// composite it onto the scene at the layer's strength.
    fn overlay_layer(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        buf: &wgpu::Buffer,
        n_vertices: u32,
        depth_load: wgpu::LoadOp<f32>,
        slot: usize,
    ) {
        let targets = self.targets.as_ref().expect("created by ensure_targets");
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.overlay,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: depth_load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_pipeline(pipeline);
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..n_vertices, 0..1);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &targets.scene,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });
        pass.set_pipeline(&self.composite);
        pass.set_bind_group(0, &targets.bind_groups[slot], &[]);
        pass.draw(0..3, 0..1);
    }
}
