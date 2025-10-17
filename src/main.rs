mod app;
mod audio;
mod renderer;

use app::App;
use winit::event_loop::EventLoop;

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = App::default();
    event_loop.run_app(&mut app)?;
    Ok(())
}
