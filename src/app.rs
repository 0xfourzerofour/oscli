use crate::{audio::Media, renderer::WaveformRenderer};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

pub struct App {
    window: Option<Window>,
    renderer: Option<WaveformRenderer>,
    media: Option<Media>,
    zoom: f32,
    scroll_offset: f32,
    mouse_pos: (f32, f32),
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            renderer: None,
            media: None,
            zoom: 1.0,
            scroll_offset: 0.0,
            mouse_pos: (0.0, 0.0),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes()
            .with_title("Audio Player with Waveform")
            .with_inner_size(LogicalSize::new(800, 600));
        let window = event_loop.create_window(attributes).unwrap();
        let media = Media::try_from_path("example.mp3").unwrap();
        let renderer = pollster::block_on(WaveformRenderer::new(&window, &media.peaks));
        self.window = Some(window);
        self.renderer = Some(renderer);
        self.media = Some(media);
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let (Some(renderer), Some(media), Some(window)) =
                    (&mut self.renderer, &self.media, &self.window)
                {
                    let playhead_pos = media.position.load(std::sync::atomic::Ordering::Relaxed)
                        as f32
                        / media.duration_samples as f32;
                    renderer
                        .render(self.zoom, self.scroll_offset, playhead_pos)
                        .ok();
                    window.request_redraw(); // Continuous redraw for playhead
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let (Some(media), Some(window)) = (&mut self.media, &self.window) {
                    let size = window.inner_size();
                    let waveform_width = size.width as f32;
                    let seek_frac =
                        (self.mouse_pos.0 / waveform_width) / self.zoom + self.scroll_offset;
                    let seek_time = seek_frac
                        * (media.duration_samples as f32
                            / media.sample_rate.0 as f32
                            / media.channels as f32);
                    media.seek(seek_time as f64).ok();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                match delta {
                    MouseScrollDelta::LineDelta(_, y) => {
                        if y > 0.0 {
                            self.zoom *= 1.1; // Zoom in
                        } else {
                            self.zoom /= 1.1; // Zoom out
                        }
                        self.zoom = self.zoom.clamp(1.0, 10.0);
                        self.scroll_offset = self.scroll_offset.min(1.0 - 1.0 / self.zoom);
                    }
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if let Some(media) = &mut self.media {
                        match event.physical_key {
                            PhysicalKey::Code(KeyCode::Space) => media.play().expect("Should play"),
                            PhysicalKey::Code(KeyCode::KeyP) => {
                                media.pause().expect("Should pause")
                            }
                            PhysicalKey::Code(KeyCode::KeyR) => {
                                media.reset().expect("Should reset")
                            }
                            PhysicalKey::Code(KeyCode::ArrowLeft) => {
                                self.scroll_offset -= 0.1 / self.zoom;
                                self.scroll_offset = self.scroll_offset.max(0.0);
                            }
                            PhysicalKey::Code(KeyCode::ArrowRight) => {
                                self.scroll_offset += 0.1 / self.zoom;
                                self.scroll_offset = self.scroll_offset.min(1.0 - 1.0 / self.zoom);
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
