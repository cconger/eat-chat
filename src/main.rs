use std::env;
use std::time::{Instant, Duration};
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use tokio::runtime::Builder;
use ringbuf::RingBuffer;
use crate::renderer::Screen;

mod chat;
mod renderer;

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    let runtime = Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();

    let mut screen = runtime.block_on(Screen::new(&window));

    let token = match env::var("TOKEN") {
        Ok(t) => t,
        Err(e) => {
            println!("No TOKEN set, will not connect to chat: {}", e);
            "".to_string()
        }
    };
    let nick = match env::var("NICK") {
        Ok(n) => n,
        Err(e) => {
            println!("No NICK set, will not connect to chat: {}", e);
            "".to_string()
        }
    };

    // TODO: Replace this ring buffer, it doesn't actually work the way I want: overwriting input
    // as it comes in.
    let rb = RingBuffer::<String>::new(20);
    let (prod, mut cons) = rb.split();

    if token != "" && nick != "" {
        let _handle = runtime.spawn(chat::read_chat(token, nick, prod));
    }

    let mut l = 0;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(500));

        match event {
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == window.id() =>  {
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
                        screen.resize(*physical_size);
                        window.request_redraw();
                    },
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        screen.resize(**new_inner_size);
                        window.request_redraw();
                    },
                    _ => {}
                }
            },
            Event::RedrawRequested(_) => {
                println!("redraw");
                screen.update();
                match screen.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                    Err(e) => eprintln!("{:?}", e),
                }
            },
            Event::MainEventsCleared => {
                //window.request_redraw();
            },
            Event::NewEvents(StartCause::ResumeTimeReached {
                start: _t,
                requested_resume: _r,
            }) => {
                let mut any = false;
                // Drain the ring buffer
                while let Some(v) = cons.pop() {
                    screen.print_string(l, 1, v);
                    l += 1;
                    any = true;
                    //println!("Message: {}", v);
                }
                if any {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    });
}
