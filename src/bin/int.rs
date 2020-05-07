use std::collections::HashSet;
use mappy::MappyState;
use sdl2::rect::Rect;
use sdl2::pixels::Color;
use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Scancode};
use sdl2;
use retro_rs::{Emulator,Buttons};
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(PartialEq, Eq, Debug)]
enum PlayState {
    Paused,
    Playing,
}

const SCALE:u32=3;

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

    let sdl_context = sdl2::init().unwrap();
    let video_subsystem = sdl_context.video().unwrap();
    let (ww,wh) = (w as u32*SCALE,h as u32*SCALE);
    let window = video_subsystem.window("Mappy", ww, wh)
        .build()
        .unwrap();

    let mut canvas = window.into_canvas().build().unwrap();

    canvas.set_draw_color(Color::RGB(0, 0, 0));
    canvas.clear();
    canvas.present();

    let mut event_pump = sdl_context.event_pump().unwrap();

    let tex_creator = canvas.texture_creator();

    let mut game_tex = tex_creator.create_texture(
        sdl2::pixels::PixelFormatEnum::ARGB8888,
        sdl2::render::TextureAccess::Streaming,
        w as u32,
        h as u32
    ).expect("Couldn't make game render texture");


    let mut play_state = PlayState::Playing;
    let mut draw_grid = false;
    let mut frame_counter: u64 = 0;
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
wasd for directional movement
gh for select/start
j for run/throw fireball (when fiery)
k for jump
# for load inputs #
shift-# for dump inputs #

zxcvbnm,./ for debug displays"
    );
    let mut fb = vec![0_u32;w*h];
    let mut last_pressed:HashSet<_> = event_pump.keyboard_state().pressed_scancodes().collect();
    'running: loop {
        let frame_start = Instant::now();
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit {..} |
                Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    break 'running
                },
                _ => {}
            }
        }
        let now_pressed:HashSet<_> = event_pump.keyboard_state().pressed_scancodes().collect();
        let just_pressed:HashSet<_> = now_pressed.difference(&last_pressed).collect();

        //space: pause/play

        //wasd: directional movement
        //g: select
        //h: start
        //j: b (run)
        //k: a (jump)

        if just_pressed.contains(&Scancode::Space) {
            play_state = match play_state {
                PlayState::Paused => PlayState::Playing,
                PlayState::Playing => PlayState::Paused,
            };
            println!("Toggle playing to: {:?}", play_state);
        }
        if just_pressed.contains(&Scancode::Z) {
            draw_grid = !draw_grid;
        }
        let shifted = now_pressed.contains(&Scancode::LShift) || now_pressed.contains(&Scancode::RShift);
        let numkey = {
            if just_pressed.contains(&Scancode::Num0) {
                Some(0)
            } else if just_pressed.contains(&Scancode::Num1) {
                Some(1)
            } else if just_pressed.contains(&Scancode::Num2) {
                Some(2)
            } else if just_pressed.contains(&Scancode::Num3) {
                Some(3)
            } else if just_pressed.contains(&Scancode::Num4) {
                Some(4)
            } else if just_pressed.contains(&Scancode::Num5) {
                Some(5)
            } else if just_pressed.contains(&Scancode::Num6) {
                Some(6)
            } else if just_pressed.contains(&Scancode::Num7) {
                Some(7)
            } else if just_pressed.contains(&Scancode::Num8) {
                Some(8)
            } else if just_pressed.contains(&Scancode::Num9) {
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
                inputs.clear();
                replay_inputs.clear();
                mappy::read_fm2(&mut replay_inputs, &path);
                replay_index = 0;
            }
        }
        if play_state == PlayState::Playing {
            let buttons = if replay_index >= replay_inputs.len() {
                Buttons::new()
                    .up(now_pressed.contains(&Scancode::W))
                    .down(now_pressed.contains(&Scancode::S))
                    .left(now_pressed.contains(&Scancode::A))
                    .right(now_pressed.contains(&Scancode::D))
                    .select(now_pressed.contains(&Scancode::G))
                    .start(now_pressed.contains(&Scancode::H))
                    .b(now_pressed.contains(&Scancode::J))
                    .a(now_pressed.contains(&Scancode::K))
            } else {
                replay_index += 1;
                replay_inputs[replay_index-1][0]
            };
            inputs.push([buttons, Buttons::new()]);
            emu.run(inputs[inputs.len() - 1]);
            emu.copy_framebuffer_argb32(&mut fb).expect("Couldn't copy emulator framebuffer");
            game_tex.update(Rect::new(0,0,w as u32,h as u32),
                            unsafe { &fb.align_to().1 },
                            4*w).expect("Couldn't copy emulator fb to texture");
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
            canvas.copy_ex(&game_tex,
                           Rect::new(0,0,w as u32,h as u32),
                           Rect::new(0,0,ww,wh),
                           0.0,
                           None,
                           false,
                           false).expect("Couldn't blit game tex");

            // draw mappy split
            if draw_grid {
                canvas.set_draw_color(Color::RGB(255,0,0));
                for x in ((mappy.grid_align.0 as usize)..w).step_by(8) {
                    for y in ((mappy.splits[0].0+mappy.grid_align.1)..mappy.splits[0].1).step_by(8) {
                        canvas.draw_rect(Rect::new((x as u32*SCALE) as i32,
                                                   (y as u32*SCALE) as i32,
8*SCALE,
                                               8*SCALE)).unwrap();
                    }
                }
            }
        }
        last_pressed = now_pressed;
        canvas.present();
        let one_sixtieth = Duration::new(0, 1_000_000_000u32 / 60);
        if one_sixtieth > frame_start.elapsed() {
            ::std::thread::sleep(one_sixtieth - frame_start.elapsed());
        }
    }
    mappy.dump_tiles(Path::new("../../out/"));
}
