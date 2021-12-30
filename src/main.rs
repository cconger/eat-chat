use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};
use crossfont::{self, Rasterize, Rasterizer, BitmapBuffer, FontDesc, Style, Slant, Weight, Size, GlyphKey};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
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
struct ProjectionUniform {
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

const INDICES: &[u16] = &[0,2,1,2,0,3];

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct InstanceRaw {
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

struct Screen {
    offset_x: u32,
    offset_y: u32,
    width: u32,
    height: u32,
    cell_width: f32,
    cell_height: f32,
    cells: Vec<Cell>,
}

impl Screen {
    fn new(offset_x: u32, offset_y: u32, width: u32, height: u32, cell_width: f32, cell_height: f32) -> Self {
        Self{
            offset_x,
            offset_y,
            width,
            height,
            cell_width,
            cell_height,
            cells: Vec::new(),
        }
    }

    fn update(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    fn projection_uniform(&self) -> ProjectionUniform {
        ProjectionUniform {
            cell_dim: [self.cell_width, self.cell_height],
            size: [self.width as f32, self.height as f32],
            offset: [self.offset_x as f32, self.offset_y as f32],
        }
    }

    fn instance_data(&self) -> Vec<InstanceRaw> {
        self.cells.iter().map(Cell::to_instance).collect::<Vec<_>>()
    }
}

struct Cell {
    col: u32,
    row: u32,
    bg_color: [f32;3],
    fg_color: [f32;4],
    glyph: Glyph,
    top: f32,
    left: f32,
    width: f32,
    height: f32,
}


impl Cell {
    fn to_instance(&self) -> InstanceRaw {
        InstanceRaw {
            cell_coords: [self.col as f32, self.row as f32],
            tex_offset: [self.glyph.uv_left, self.glyph.uv_top],
            tex_size: [self.glyph.uv_width, self.glyph.uv_height],
            bg_color: self.bg_color,
            fg_color: self.fg_color,
            position: [self.left, self.top, self.width, self.height],
        }
    }
}

struct Glyph {
    key: GlyphKey,
    uv_top: f32,
    uv_left: f32,
    uv_width: f32,
    uv_height: f32,
}


struct State {
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
    screen: Screen,
    instance_buffer: wgpu::Buffer,
    projection_buffer: wgpu::Buffer,
    projection_bind_group: wgpu::BindGroup,
    diffuse_bind_group: wgpu::BindGroup,
}

impl State {
    async fn new(window: &Window) -> Self {
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
        let mut rasterizer = Rasterizer::new(scale_factor, true).unwrap();

        let font_desc = FontDesc::new::<String>(
            "SF Mono".into(),
            Style::Description{
                slant: Slant::Normal,
                weight: Weight::Normal,
            });
        let font_size = Size::new(20.0);
        let regular = rasterizer.load_font(&font_desc, font_size).unwrap();
        let gk = GlyphKey { font_key: regular, character: 'm', size: font_size };
        let m_glyph = rasterizer.get_glyph(gk).unwrap();
        let metrics = rasterizer.metrics(regular, font_size).unwrap();
        println!("Average Advance: {}", metrics.average_advance);
        println!("Line Height    : {}", metrics.line_height);
        println!("Descent        : {}", metrics.descent);
        println!("Underline Pos  : {}", metrics.underline_position);
        println!("Underline Thick: {}", metrics.underline_thickness);
        println!("Strikeout Pos  : {}", metrics.strikeout_position);
        println!("Strikeout Thick: {}", metrics.strikeout_thickness);

        println!("Glyph {} Width : {}", m_glyph.character, m_glyph.width);
        println!("Glyph {} Height: {}", m_glyph.character, m_glyph.height);
        println!("Glyph {} Top   : {}", m_glyph.character, m_glyph.top);
        println!("Glyph {} Left  : {}", m_glyph.character, m_glyph.left);

        // Load m_glyph as a texture
        let texture_size = wgpu::Extent3d {
            width: m_glyph.width as u32,
            height: m_glyph.height as u32,
            depth_or_array_layers: 1,
        };
        let diffuse_texture = device.create_texture(
            &wgpu::TextureDescriptor {
                // All textures are stored as 3D, we represent our 2D texture
                // by setting depth to 1.
                size: texture_size,
                mip_level_count: 1, 
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // Most images are stored using sRGB so we need to reflect that here.
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                // TEXTURE_BINDING tells wgpu that we want to use this texture in shaders
                // COPY_DST means that we want to copy data to this texture
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                label: Some("glyph_texture"),
            }
        );

        let buff = match m_glyph.buffer {
            BitmapBuffer::Rgba(v) => {
                println!("Format: RGBA");
                v
            },
            BitmapBuffer::Rgb(v) => {
                println!("Format: RGB");
                let mut new_buff = Vec::with_capacity((v.len() / 3) * 4);
                for chunk in v.chunks(3) {
                    match chunk {
                        &[r,g,b] => {
                            new_buff.push(r);
                            new_buff.push(g);
                            new_buff.push(b);
                            new_buff.push(std::cmp::max(std::cmp::max(r,g),b));
                        }
                        _ => println!("Not chunk aligned"),
                    }
                }

                new_buff
            },
        };

        //println!("Glyph {} Buffer Len: {}", m_glyph.character, buff.len());
        //println!("Glyph {} Buffer: {:?}", m_glyph.character, buff);

        queue.write_texture(
            // Tells wgpu where to copy the pixel data
            wgpu::ImageCopyTexture {
                texture: &diffuse_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            // The actual pixel data
            &buff,
            // The layout of the texture
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: std::num::NonZeroU32::new(4 * m_glyph.width as u32),
                rows_per_image: std::num::NonZeroU32::new(m_glyph.height as u32),
            },
            texture_size,
            );

        let diffuse_texture_view = diffuse_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let diffuse_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let mut screen = Screen::new(
            0,0,
            size.width,
            size.height,
            metrics.average_advance as f32,
            metrics.line_height as f32,
            );

        let middle_cell = Cell {
            col: 1,
            row: 1,
            bg_color: [0.0,0.0,0.0],
            fg_color: [1.0,0.0,0.0,1.0],
            glyph: Glyph {
                key: gk,
                uv_top: 0.0,
                uv_left: 0.0,
                uv_height: 1.0,
                uv_width: 1.0,
            },
            width: m_glyph.width as f32,
            height: m_glyph.height as f32,
            top: m_glyph.top as f32 - metrics.descent,
            left: m_glyph.left as f32,
        };

        println!("Middle Cell: {:?}", middle_cell.to_instance());

        screen.cells.push(Cell {
            col: 1,
            row: 0,
            bg_color: [0.0,0.0,0.0],
            fg_color: [1.0,1.0,1.0,1.0],
            glyph: Glyph {
                key: gk,
                uv_top: 0.0,
                uv_left: 0.0,
                uv_height: 1.0,
                uv_width: 1.0,
            },
            width: m_glyph.width as f32,
            height: m_glyph.height as f32,
            top: m_glyph.top as f32 - metrics.descent,
            left: m_glyph.left as f32,
        });
        screen.cells.push(Cell {
            col: 0,
            row: 1,
            bg_color: [0.0,0.0,0.0],
            fg_color: [1.0,1.0,1.0,0.5],
            glyph: Glyph {
                key: gk,
                uv_top: 0.0,
                uv_left: 0.0,
                uv_height: 1.0,
                uv_width: 1.0,
            },
            width: m_glyph.width as f32,
            height: m_glyph.height as f32,
            top: m_glyph.top as f32 - metrics.descent,
            left: m_glyph.left as f32,
        });
        screen.cells.push(middle_cell);
        screen.cells.push(Cell {
            col: 2,
            row: 1,
            bg_color: [0.0,0.0,0.0],
            fg_color: [1.0,1.0,1.0,0.5],
            glyph: Glyph {
                key: gk,
                uv_top: 0.0,
                uv_left: 0.0,
                uv_height: 1.0,
                uv_width: 1.0,
            },
            width: m_glyph.width as f32,
            height: m_glyph.height as f32,
            top: m_glyph.top as f32 - metrics.descent,
            left: m_glyph.left as f32,
        });
        screen.cells.push(Cell {
            col: 1,
            row: 2,
            bg_color: [0.0,0.0,0.0],
            fg_color: [1.0,1.0,1.0,0.5],
            glyph: Glyph {
                key: gk,
                uv_top: 0.0,
                uv_left: 0.0,
                uv_height: 1.0,
                uv_width: 1.0,
            },
            width: m_glyph.width as f32,
            height: m_glyph.height as f32,
            top: m_glyph.top as f32 - metrics.descent,
            left: m_glyph.left as f32,
        });

        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Instance Buffer"),
            contents: bytemuck::cast_slice(&screen.instance_data()),
            usage: wgpu::BufferUsages::VERTEX,
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
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler {
                            // This is only for TextureSampleType::Depth
                            comparison: false,
                            // This should be true if the sample_type of the texture is:
                            //     TextureSampleType::Float { filterable: true }
                            // Otherwise you'll get an error.
                            filtering: true,
                        },
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


        // Projection Uniform needs the metrics from the font (we should not have this as a
        // uniform)
        let projection_buffer = device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Projection Uniform"),
                contents: bytemuck::cast_slice(&[screen.projection_uniform()]),
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

        println!("{:?}", screen.projection_uniform());


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
                clamp_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
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
                clamp_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        });

        Self {
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
            bg_render_pipeline,
            vertex_buffer,
            index_buffer,
            screen,
            instance_buffer,
            num_indices,
            projection_buffer,
            projection_bind_group,
            diffuse_bind_group,
        }
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);

            self.screen.update(new_size.width, new_size.height);
            self.queue.write_buffer(
                &self.projection_buffer,
                0,
                bytemuck::cast_slice(&[self.screen.projection_uniform()]),
                );
        }
    }

    fn input(&mut self, _event: &WindowEvent) -> bool {
        false
    }

    fn update(&mut self) {
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
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
            bg_render_pass.set_bind_group(1, &self.diffuse_bind_group, &[]);
            bg_render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            bg_render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
            bg_render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            bg_render_pass.draw_indexed(0..self.num_indices, 0, 0..self.screen.cells.len() as _);
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
            render_pass.draw_indexed(0..self.num_indices, 0, 0..self.screen.cells.len() as _);
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        output.present();
        Ok(())
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();
    let mut state = pollster::block_on(State::new(&window));

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == window.id() =>  if !state.input(event) {
                match event {
                    WindowEvent::CloseRequested
                        | WindowEvent::KeyboardInput {
                            input: KeyboardInput {
                                state: ElementState::Pressed,
                                virtual_keycode: Some(VirtualKeyCode::Escape),
                                ..
                            },
                            ..
                        } => *control_flow = ControlFlow::Exit,
                    WindowEvent::Resized(physical_size) => {
                        state.resize(*physical_size);
                        window.request_redraw();
                    },
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        state.resize(**new_inner_size);
                        window.request_redraw();
                    },
                    _ => {}
                }
            },
            Event::RedrawRequested(_) => {
                println!("redraw");
                state.update();
                match state.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                    Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                    Err(e) => eprintln!("{:?}", e),
                }
            },
            Event::MainEventsCleared => {
                //window.request_redraw();
            },
            _ => {}
        }
    });
}
