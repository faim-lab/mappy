use mappy::MappyState;
use minifb::{Key, KeyRepeat, Scale, Window, WindowOptions};
use retro_rs::{Buttons, Emulator};
use std::path::Path;
use std::time::{Duration, Instant};
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
    use std::env;
    let mut emu = Emulator::create(
        Path::new("../../cores/fceumm_libretro"),
        Path::new("../../roms/mario.nes"),
    );
    // Have to run emu for one frame before we can get the framebuffer size
    emu.run([Buttons::new(), Buttons::new()]);
    let (w, h) = emu.framebuffer_size();
    // So reset it afterwards
    emu.reset();

    let mut buffer: Vec<u32> = vec![0xFF00_0000; w * SCALE * h * SCALE];
    let mut window = Window::new(
        &"MPC Mario",
        w,
        h,
        WindowOptions {
            scale: Scale::X4,
            ..WindowOptions::default()
        },
    )
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
    let mut inputs: Vec<[Buttons; 2]> = Vec::with_capacity(32000);
    let mut replay_inputs: Vec<[Buttons; 2]> = vec![];
    let mut replay_index = 0;
    let args:Vec<_> = env::args().collect();
    if args.len() > 1 {
        mappy::read_fm2(&mut replay_inputs, &Path::new(&args[1]));
    }

    let mut mappy = MappyState::new(w, h);
    let start = Instant::now();
    println!(
        "Instructions
Space bar toggles play/pause
wasd for directional movement (mostly left/right, but can go down into some pipes or up vines
gh for select/start
j for run/throw fireball (when fiery)
k for jump
# for load inputs #
shift-# for dump inputs #

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
        let shifted = window.is_key_down(Key::LeftShift) || window.is_key_down(Key::RightShift);
        let numkey = {
            if window.is_key_pressed(Key::Key0, KeyRepeat::No) {
                Some(0)
            } else if window.is_key_pressed(Key::Key1, KeyRepeat::No) {
                Some(1)
            } else if window.is_key_pressed(Key::Key2, KeyRepeat::No) {
                Some(2)
            } else if window.is_key_pressed(Key::Key3, KeyRepeat::No) {
                Some(3)
            } else if window.is_key_pressed(Key::Key4, KeyRepeat::No) {
                Some(4)
            } else if window.is_key_pressed(Key::Key5, KeyRepeat::No) {
                Some(5)
            } else if window.is_key_pressed(Key::Key6, KeyRepeat::No) {
                Some(6)
            } else if window.is_key_pressed(Key::Key7, KeyRepeat::No) {
                Some(7)
            } else if window.is_key_pressed(Key::Key8, KeyRepeat::No) {
                Some(8)
            } else if window.is_key_pressed(Key::Key9, KeyRepeat::No) {
                Some(9)
            } else {
                None
            }
        };
        if let Some(n) = numkey {
            let path = Path::new("../../inputs/").join(format!("mario_{}.fm2", n));
            if shifted {
                mappy::write_fm2(&inputs, &path);
                println!("Dumped {}",n);
            } else {
                // TODO clear mappy too?
                emu.reset();
                frame_counter = 0;
                last_fps_update_counter = 0;
                inputs.clear();
                replay_inputs.clear();
                mappy::read_fm2(&mut replay_inputs, &path);
                replay_index = 0;
            }
        }
        if play_state == PlayState::Playing {
            elapsed_time += last_time.elapsed();
            total_elapsed_time += last_time.elapsed();
        }
        last_time = Instant::now();
        while elapsed_time.as_secs_f64() >= 1.0 / 60.0 {
            elapsed_time -= Duration::from_secs_f64(1.0 / 60.0);
            let buttons = if replay_index >= replay_inputs.len() {
                Buttons::new()
                    .up(window.is_key_down(Key::W))
                    .down(window.is_key_down(Key::S))
                    .left(window.is_key_down(Key::A))
                    .right(window.is_key_down(Key::D))
                    .select(window.is_key_down(Key::G))
                    .start(window.is_key_down(Key::H))
                    .b(window.is_key_down(Key::J))
                    .a(window.is_key_down(Key::K))
            } else {
                replay_index += 1;
                replay_inputs[replay_index-1][0]
            };
            inputs.push([buttons, Buttons::new()]);
            emu.run(inputs[inputs.len() - 1]);
            mappy.process_screen(&emu);
            frame_counter += 1;
            if frame_counter % 60 == 0 {
                println!("Scroll: {:?} : {:?}", mappy.splits, mappy.scroll);
                println!("Known tiles: {:?}", mappy.tiles.len());
                println!(
                    "Net: {:} for {:} inputs, avg {:}",
                    start.elapsed().as_secs_f64(),
                    frame_counter,
                    start.elapsed().as_secs_f64() / (frame_counter as f64)
                );
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
                .update_with_buffer(&buffer, w * SCALE, h * SCALE)
                .expect("Couldn't update window framebuffer!");
            draw_t = Instant::now();
        }
        window.update()
    }
    mappy.dump_tiles(Path::new("../../out/"));
}
