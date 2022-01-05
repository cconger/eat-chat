use winit::window::Window;
use crossfont::{self, FontDesc, Style, Slant, Weight, Size, GlyphKey};
use wgpu::util::DeviceExt;
use crate::renderer::atlas::{Glyph, Atlas};

mod atlas;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    position: [f32; 2],
}

impl Vertex {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
            ],
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ProjectionUniform {
    cell_dim: [f32; 2],
    size: [f32; 2],
    offset: [f32; 2],
}

const VERTICES: &[Vertex] = &[
    // Top Left
    Vertex {
        position: [0., 1.],
    },
    // Bottom left
    Vertex {
        position: [0., 0.],
    },
    // Bottom Right
    Vertex {
        position: [1., 0.],
    },
    // Top Right
    Vertex {
        position: [1., 1.],
    },
];

// Makes two counterclockwise triangles out of the four points
const INDICES: &[u16] = &[0,2,1,2,0,3];

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    cell_coords: [f32;2],
    tex_offset: [f32;2],
    tex_size: [f32;2],
    bg_color: [f32;3],
    fg_color: [f32;4],
    position: [f32;4],
}

impl InstanceRaw {
    fn desc<'a>() -> wgpu::VertexBufferLayout<'a> {
        use std::mem;
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<InstanceRaw>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 5,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32;2]>() as wgpu::BufferAddress,
                    shader_location: 6,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32;4]>() as wgpu::BufferAddress,
                    shader_location: 7,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32;6]>() as wgpu::BufferAddress,
                    shader_location: 8,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32;9]>() as wgpu::BufferAddress,
                    shader_location: 9,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: mem::size_of::<[f32;13]>() as wgpu::BufferAddress,
                    shader_location: 10,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

pub struct Cell {
    col: u32,
    row: u32,
    bg_color: [f32;3],
    fg_color: [f32;4],
    glyph: Glyph,
}


impl Cell {
    fn to_instance(&self) -> InstanceRaw {
        InstanceRaw {
            cell_coords: [self.col as f32, self.row as f32],
            tex_offset: [self.glyph.uv_left, self.glyph.uv_top],
            tex_size: [self.glyph.uv_width, self.glyph.uv_height],
            bg_color: self.bg_color,
            fg_color: self.fg_color,
            position: [self.glyph.left, self.glyph.top, self.glyph.width, self.glyph.height],
        }
    }
}

pub struct Screen {
    offset_x: u32,
    offset_y: u32,
    cell_width: f32,
    cell_height: f32,
    cells: Vec<Cell>,

    font_key: crossfont::FontKey,
    font_size: f32,

    atlas: Atlas,

    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    render_pipeline: wgpu::RenderPipeline,
    bg_render_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    instance_buffer: wgpu::Buffer,
    projection_buffer: wgpu::Buffer,
    projection_bind_group: wgpu::BindGroup,
    diffuse_bind_group: wgpu::BindGroup,
}

impl Screen {
    pub async fn new(window: &Window) -> Self {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            }
        ).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::empty(),
                limits: wgpu::Limits::default(),
                label: None,
            },
            None,
        ).await.unwrap();

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_preferred_format(&adapter).unwrap(),
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });


        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });
        let num_indices = INDICES.len() as u32;

        // Font Rendering
        let scale_factor = window.scale_factor() as f32;
        let mut atlas = Atlas::new(scale_factor);

        let font_size = 20.0;
        let font_desc = FontDesc::new::<String>(
            "SF Mono".into(),
            Style::Description{
                slant: Slant::Normal,
                weight: Weight::Normal,
            });

        let (regular, metrics) = atlas.load_font(&font_desc, font_size);
        println!("Average Advance: {}", metrics.average_advance);
        println!("Line Height    : {}", metrics.line_height);
        println!("Descent        : {}", metrics.descent);
        println!("Underline Pos  : {}", metrics.underline_position);
        println!("Underline Thick: {}", metrics.underline_thickness);
        println!("Strikeout Pos  : {}", metrics.strikeout_position);
        println!("Strikeout Thick: {}", metrics.strikeout_thickness);

        let diffuse_texture_view = atlas.texture_view(&device);
        let diffuse_sampler = atlas.sampler(&device);

        let cell_width = metrics.average_advance;
        let cell_height = metrics.line_height;

        let middle_cell = Cell {
            col: 1,
            row: 1,
            bg_color: [0.0,0.0,0.0],
            fg_color: [1.0,0.0,0.0,1.0],
            glyph: atlas.get_glyph(&device, &queue, GlyphKey {
                character: 'b',
                font_key: regular,
                size: Size::new(20.0),
            }).unwrap(),
        };

        println!("Middle Cell: {:?}", middle_cell.to_instance());

        let mut cells = Vec::new();
        cells.push(Cell {
            col: 1,
            row: 0,
            bg_color: [0.0,0.0,0.0],
            fg_color: [1.0,1.0,1.0,1.0],
            glyph: atlas.get_glyph(&device, &queue, GlyphKey {
                character: 'u',
                font_key: regular,
                size: Size::new(20.0),
            }).unwrap(),
        });
        cells.push(Cell {
            col: 0,
            row: 1,
            bg_color: [0.0,0.0,0.0],
            fg_color: [1.0,1.0,1.0,0.5],
            glyph: atlas.get_glyph(&device, &queue, GlyphKey {
                character: 'a',
                font_key: regular,
                size: Size::new(20.0),
            }).unwrap(),
        });
        cells.push(middle_cell);
        cells.push(Cell {
            col: 2,
            row: 1,
            bg_color: [0.0,0.0,0.0],
            fg_color: [1.0,1.0,1.0,0.5],
            glyph: atlas.get_glyph(&device, &queue, GlyphKey {
                character: 'c',
                font_key: regular,
                size: Size::new(20.0),
            }).unwrap(),
        });
        cells.push(Cell {
            col: 1,
            row: 2,
            bg_color: [0.0,0.0,0.0],
            fg_color: [1.0,1.0,1.0,0.5],
            glyph: atlas.get_glyph(&device, &queue, GlyphKey {
                character: 'd',
                font_key: regular,
                size: Size::new(20.0),
            }).unwrap(),
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Instance Buffer"),
            size: 1024*1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let texture_bind_group_layout = device.create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry { binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    ],
                    label: Some("texture_bind_group_layout"),
            }
        );
        let diffuse_bind_group = device.create_bind_group(
            &wgpu::BindGroupDescriptor {
                layout: &texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&diffuse_texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&diffuse_sampler),
                    }
                ],
                label: Some("diffuse_bind_group"),
            }
        );

        let projection_uniform = ProjectionUniform {
                        cell_dim: [cell_width as f32, cell_height as f32],
                        size: [size.width as f32, size.height as f32],
                        offset: [0.0, 0.0],
                    };

        // Projection Uniform needs the metrics from the font (we should not have this as a
        // uniform)
        let projection_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Projection Uniform"),
                contents: bytemuck::cast_slice(&[projection_uniform]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            }
            );
        let projection_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Projection Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer{
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        let projection_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Projection Bind Group"),
            layout: &projection_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: projection_buffer.as_entire_binding(),
                },
            ],
        });

        println!("{:?}", projection_uniform);

        let bg_render_pipeline_layout = 
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &projection_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });

        let bg_render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("BG Render Piepline"),
            layout: Some(&bg_render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_bg",
                buffers: &[Vertex::desc(), InstanceRaw::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_bg",
                targets: &[wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::REPLACE,
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &projection_bind_group_layout,
                    &texture_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });


        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Piepline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[Vertex::desc(), InstanceRaw::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::OVER,
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                }],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        Self {
            offset_x: 0,
            offset_y: 0,
            cell_width: cell_width as f32,
            cell_height: cell_height as f32,
            size,

            font_key: regular,
            font_size,
            cells,

            surface,
            device,
            queue,
            config,
            render_pipeline,
            bg_render_pipeline,
            vertex_buffer,
            index_buffer,
            atlas,
            instance_buffer,
            num_indices,
            projection_buffer,
            projection_bind_group,
            diffuse_bind_group,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            self.queue.write_buffer(
                &self.projection_buffer,
                0,
                bytemuck::cast_slice(&[
                    ProjectionUniform {
                        cell_dim: [self.cell_width, self.cell_height],
                        size: [new_size.width as f32, new_size.height as f32],
                        offset: [self.offset_x as f32, self.offset_y as f32],
                    },
                ]),
                );
        }
    }

    pub fn update(&mut self) {
        // TODO: Check if self.instance_data() size is larger than our buffer and realloc
        self.queue.write_buffer(
            &self.instance_buffer,
            0,
            bytemuck::cast_slice(&self.instance_data()),
        );
    }

    pub fn print_string(&mut self, row: u32, col: u32, s: String) {
        for (i, c) in s.chars().enumerate() {
            self.cells.push(Cell {
                col: col + i as u32,
                row,
                bg_color: [0.0, 0.0, 0.0],
                fg_color: [1.0, 1.0, 1.0, 1.0],
                glyph: self.atlas.get_glyph(&self.device, &self.queue, GlyphKey {
                    character: c,
                    font_key: self.font_key,
                    size: Size::new(self.font_size),
                }).unwrap(),
            });
        }
    }

    fn instance_data(&self) -> Vec<InstanceRaw> {
        self.cells.iter().map(Cell::to_instance).collect::<Vec<_>>()
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let mut bg_render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("BG Render Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.1,
                            b: 0.1,
                            a: 1.0,
                        }),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            // Render the backgrounds
            bg_render_pass.set_pipeline(&self.bg_render_pipeline);
            bg_render_pass.set_bind_group(0, &self.projection_bind_group, &[]);
            bg_render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            bg_render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            bg_render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            bg_render_pass.draw_indexed(0..self.num_indices, 0, 0..self.cells.len() as _);
        }

        {
            // Draw the glyphs
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("FG Render Pass"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.projection_bind_group, &[]);
            render_pass.set_bind_group(1, &self.diffuse_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..self.num_indices, 0, 0..self.cells.len() as _);
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        output.present();
        Ok(())
    }
}
