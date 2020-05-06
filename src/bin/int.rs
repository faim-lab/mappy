use minifb::{Key, KeyRepeat, MouseMode, Window, WindowOptions, Scale};
use retro_rs::{Emulator, Buttons};
use mappy::MappyState;
use std::path::Path;
use std::time::{Instant, Duration};

const SCALE: usize = 1;

#[derive(PartialEq, Eq, Debug)]
enum PlayState {
    Paused,
    Playing,
}

fn display_into(emu: &mut Emulator, buffer: &mut [u32]) {
    let (w, _h) = emu.framebuffer_size();
    emu.for_each_pixel(|x, y, r, g, b| {
        let col = 0xFF00_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b);
        for oy in 0..SCALE {
            let target = (y * SCALE + oy) * w * SCALE + (x * SCALE);
            for ox in 0..SCALE {
                buffer[target + ox] = col;
            }
        }
    })
        .expect("Couldn't copy out of emulator framebuffer!")
}


fn main() {
    let mut emu = Emulator::create(
        Path::new("../../cores/fceumm_libretro"),
        Path::new("../../roms/mario.nes"),
    );
    // Have to run emu for one frame before we can get the framebuffer size
    emu.run([Buttons::new(), Buttons::new()]);
    let (w,h) = emu.framebuffer_size();
    // So reset it afterwards
    emu.reset();

    let mut buffer: Vec<u32> = vec![0xFF00_0000; w * SCALE * h * SCALE];
    let mut window = Window::new(&"MPC Mario", w, h, WindowOptions {
        scale:Scale::X4,
        ..WindowOptions::default()})
        .expect("Couldn't create window");
    let mut play_state = PlayState::Playing;
    let redraw_max_interval: f64 = 1.0 / 60.0;
    let mut draw_t = Instant::now();
    let mut elapsed_time = Duration::from_secs(0);
    let mut total_elapsed_time = Duration::from_secs(0);
    let mut frame_counter: u64 = 0;
    let mut last_time = Instant::now();
    let fps_window = 2.0;
    let mut last_fps_update_counter = 0;
    let mut fps_update_t = Instant::now();

    let mut mappy = MappyState::new(w,h);
    let start = Instant::now();
    println!(
        "Instructions
Space bar toggles play/pause
wasd for directional movement (mostly left/right, but can go down into some pipes or up vines
gh for select/start
j for run/throw fireball (when fiery)
k for jump

Feel free to hack this code to print out memory addresses, VRAM information, etc."
    );

    while window.is_open() && !window.is_key_down(Key::Escape) {
        //space: pause/play

        //wasd: directional movement
        //g: select
        //h: start
        //j: b (run)
        //k: a (jump)

        if window.is_key_pressed(Key::Space, KeyRepeat::No) {
            play_state = match play_state {
                PlayState::Paused => PlayState::Playing,
                PlayState::Playing => PlayState::Paused,
            };
            println!("Toggle playing to: {:?}", play_state);
        }
        let buttons = Buttons::new()
            .up(window.is_key_down(Key::W))
            .down(window.is_key_down(Key::S))
            .left(window.is_key_down(Key::A))
            .right(window.is_key_down(Key::D))
            .select(window.is_key_down(Key::G))
            .start(window.is_key_down(Key::H))
            .b(window.is_key_down(Key::J))
            .a(window.is_key_down(Key::K));
        if play_state == PlayState::Playing {
            elapsed_time += last_time.elapsed();
            total_elapsed_time += last_time.elapsed();
        }
        last_time = Instant::now();
        while elapsed_time.as_secs_f64() >= 1.0 / 60.0 {
            elapsed_time -= Duration::from_secs_f64(1.0 / 60.0);
            emu.run([buttons, Buttons::new()]);
            mappy.process_screen(&emu);
            frame_counter += 1;
            if frame_counter % 60 == 0 {
                println!("Scroll: {:?} : {:?}",mappy.splits, mappy.scroll);
                println!("Known tiles: {:?}", mappy.tiles.len());
                println!("Net: {:} for {:} inputs, avg {:}",
                         start.elapsed().as_secs_f64(),
                         frame_counter,
                         start.elapsed().as_secs_f64()/(frame_counter as f64));
            }
        }
        // every K seconds update avg fps counter
        if play_state == PlayState::Playing && fps_update_t.elapsed().as_secs_f64() >= fps_window {
            let avg_fps = (frame_counter - last_fps_update_counter) as f64
                / fps_update_t.elapsed().as_secs_f64();
            println!("Avg FPS: {}", avg_fps);
            last_fps_update_counter = frame_counter;
            fps_update_t = Instant::now();
        }
        if draw_t.elapsed().as_secs_f64() >= redraw_max_interval && frame_counter > 0 {
            display_into(&mut emu, &mut buffer);
            window
                .update_with_buffer(&buffer, w*SCALE, h*SCALE)
                .expect("Couldn't update window framebuffer!");
            draw_t = Instant::now();
        }
        window.update()
    }
    mappy.dump_tiles(Path::new("../../out/"));
}
