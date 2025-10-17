use crate::audio::Peak;
use anyhow::Result;
use std::sync::Arc;
use wgpu::{
    include_wgsl, util::DeviceExt, BindGroup, Buffer, Device, Queue, RenderPipeline, Surface,
    SurfaceConfiguration,
};
use winit::window::Window;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    zoom: f32,
    scroll_offset: f32,
    playhead_pos: f32,
    _padding: f32,
}

pub struct WaveformRenderer<'a> {
    surface: Surface<'a>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    render_pipeline: RenderPipeline,
    vertex_buffer: Buffer,
    playhead_line_buffer: Buffer,
    playhead_triangle_buffer: Buffer,
    uniform_buffer: Buffer,
    bind_group: BindGroup,
    vertex_count: u32,
}

impl<'a> WaveformRenderer<'a> {
    pub async fn new(window: &Arc<Window>) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                label: None,
                ..Default::default()
            })
            .await
            .unwrap();

        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width, size.height)
            .unwrap();
        surface.configure(&device, &config);

        let shader = device.create_shader_module(include_wgsl!("shaders/vertex_shader.wgsl"));

        let uniforms = Uniforms {
            zoom: 1.0,
            scroll_offset: 0.0,
            playhead_pos: 0.0,
            _padding: 0.0,
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bind Group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Render Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            cache: None,
            label: Some("Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let vertices = vec![
            Vertex {
                position: [0.0, -1.0],
            },
            Vertex {
                position: [0.0, 1.0],
            },
        ];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Playhead line (vertical thin rectangle) - using screen coordinates (clip space)
        // Use special x coordinate (-10.0) to mark these as playhead vertices
        let line_width = 0.004;
        let playhead_line_vertices = vec![
            Vertex { position: [-10.0 - line_width, -1.0] },
            Vertex { position: [-10.0 + line_width, -1.0] },
            Vertex { position: [-10.0 - line_width, 1.0] },
            Vertex { position: [-10.0 + line_width, -1.0] },
            Vertex { position: [-10.0 + line_width, 1.0] },
            Vertex { position: [-10.0 - line_width, 1.0] },
        ];
        let playhead_line_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Playhead Line Buffer"),
            contents: bytemuck::cast_slice(&playhead_line_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Playhead triangle (upside-down at top) - using special x coordinate
        let triangle_size = 0.03;
        let playhead_triangle_vertices = vec![
            Vertex { position: [-10.0 - triangle_size, 1.0] },      // Top left
            Vertex { position: [-10.0 + triangle_size, 1.0] },      // Top right
            Vertex { position: [-10.0, 1.0 - triangle_size * 1.5] }, // Bottom center
        ];
        let playhead_triangle_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Playhead Triangle Buffer"),
            contents: bytemuck::cast_slice(&playhead_triangle_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            surface,
            device,
            queue,
            config,
            render_pipeline,
            vertex_buffer,
            playhead_line_buffer,
            playhead_triangle_buffer,
            uniform_buffer,
            bind_group,
            vertex_count: 2,
        }
    }

    pub fn add_peaks(&mut self, peaks: &[Peak]) {
        // Separate left (top) and right (bottom) channels
        let mut vertices: Vec<Vertex> = Vec::with_capacity(peaks.len() * 12);

        for (i, peak) in peaks.iter().enumerate() {
            let x1 = i as f32 / peaks.len() as f32;
            let x2 = (i + 1) as f32 / peaks.len() as f32;

            // Left channel amplitude (top half, 0.0 to 1.0)
            let amp_left = (peak.max_left - peak.min_left) / 2.0;

            // Left channel (top half)
            vertices.push(Vertex { position: [x1, 0.0] });
            vertices.push(Vertex { position: [x1, amp_left] });
            vertices.push(Vertex { position: [x2, 0.0] });

            vertices.push(Vertex { position: [x1, amp_left] });
            vertices.push(Vertex { position: [x2, amp_left] });
            vertices.push(Vertex { position: [x2, 0.0] });

            // Right channel amplitude (bottom half, 0.0 to -1.0)
            let amp_right = (peak.max_right - peak.min_right) / 2.0;

            // Right channel (bottom half)
            vertices.push(Vertex { position: [x1, 0.0] });
            vertices.push(Vertex { position: [x2, 0.0] });
            vertices.push(Vertex { position: [x1, -amp_right] });

            vertices.push(Vertex { position: [x2, 0.0] });
            vertices.push(Vertex { position: [x2, -amp_right] });
            vertices.push(Vertex { position: [x1, -amp_right] });
        }
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        self.vertex_buffer = vertex_buffer;
        self.vertex_count = vertices.len() as u32;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn render(&mut self, zoom: f32, scroll_offset: f32, playhead_pos: f32) -> Result<()> {
        let uniforms = Uniforms {
            zoom,
            scroll_offset,
            playhead_pos,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    depth_slice: None,
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.05,
                            g: 0.05,
                            b: 0.08,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);

            // Draw waveform
            render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            render_pass.draw(0..self.vertex_count, 0..1);

            // Draw playhead line
            render_pass.set_vertex_buffer(0, self.playhead_line_buffer.slice(..));
            render_pass.draw(0..6, 0..1);

            // Draw playhead triangle
            render_pass.set_vertex_buffer(0, self.playhead_triangle_buffer.slice(..));
            render_pass.draw(0..3, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(())
    }
}
