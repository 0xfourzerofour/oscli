use std::sync::Arc;

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
    window: Option<Arc<Window>>,
    renderer: Option<WaveformRenderer<'static>>,
    media: Option<Media>,
    time_window: f32, // seconds to show
    scroll_offset: f32,
    mouse_pos: (f32, f32),
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            renderer: None,
            media: None,
            time_window: 1.0, // Start with 1 second window
            scroll_offset: 0.0,
            mouse_pos: (0.0, 0.0),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let attributes = Window::default_attributes()
            .with_title("Audio Player with Waveform")
            .with_inner_size(LogicalSize::new(800, 200));

        let window = Arc::new(event_loop.create_window(attributes).unwrap());
        let renderer = pollster::block_on(WaveformRenderer::new(&window));
        self.window = Some(window.clone());
        self.renderer = Some(renderer);
        self.media = None;

        window.request_redraw();
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

                    // Calculate zoom to show time_window seconds
                    let duration_secs = media.duration_samples as f32
                        / media.sample_rate.0 as f32
                        / media.channels as f32;
                    let window_zoom = duration_secs / self.time_window;

                    // Center the view on the playhead
                    let window_scroll = (playhead_pos - 0.5 / window_zoom)
                        .max(0.0)
                        .min(1.0 - 1.0 / window_zoom);

                    renderer
                        .render(window_zoom, window_scroll, playhead_pos)
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

                    // Current time in seconds
                    let current_time = media.position.load(std::sync::atomic::Ordering::Relaxed) as f32
                        / media.sample_rate.0 as f32
                        / media.channels as f32;

                    // Click position relative to center (-0.5 to 0.5)
                    let click_relative = (self.mouse_pos.0 / waveform_width) - 0.5;

                    // Calculate seek time based on visible window
                    let seek_time = current_time + (click_relative * self.time_window);
                    media.seek(seek_time.max(0.0) as f64).ok();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let zoom_delta = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 / 50.0, // Normalize pixel delta
                };

                if zoom_delta > 0.0 {
                    self.time_window /= 1.2; // Zoom in (show less time)
                } else if zoom_delta < 0.0 {
                    self.time_window *= 1.2; // Zoom out (show more time)
                }
                self.time_window = self.time_window.clamp(0.1, 10.0); // 0.1 to 10 seconds
            }
            WindowEvent::DroppedFile(path) => {
                if let Ok(media) = Media::try_from_path(path) {
                    if let Some(renderer) = &mut self.renderer {
                        renderer.add_peaks(&media.peaks);
                        self.media = Some(media);
                    }
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if let Some(media) = &mut self.media {
                        match event.physical_key {
                            PhysicalKey::Code(KeyCode::Space) => {
                                if let Err(e) = media.play() {
                                    eprintln!("Play error: {}", e);
                                }
                            }
                            PhysicalKey::Code(KeyCode::KeyP) => {
                                if let Err(e) = media.pause() {
                                    eprintln!("Pause error: {}", e);
                                }
                            }
                            PhysicalKey::Code(KeyCode::KeyR) => {
                                if let Err(e) = media.reset() {
                                    eprintln!("Reset error: {}", e);
                                }
                            }
                            PhysicalKey::Code(KeyCode::ArrowLeft) => {
                                // Seek backward by 1 second
                                let current_time = media.position.load(std::sync::atomic::Ordering::Relaxed) as f32
                                    / media.sample_rate.0 as f32
                                    / media.channels as f32;
                                media.seek((current_time - 1.0).max(0.0) as f64).ok();
                            }
                            PhysicalKey::Code(KeyCode::ArrowRight) => {
                                // Seek forward by 1 second
                                let current_time = media.position.load(std::sync::atomic::Ordering::Relaxed) as f32
                                    / media.sample_rate.0 as f32
                                    / media.channels as f32;
                                let duration_secs = media.duration_samples as f32
                                    / media.sample_rate.0 as f32
                                    / media.channels as f32;
                                media.seek((current_time + 1.0).min(duration_secs) as f64).ok();
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
