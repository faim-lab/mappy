use std::collections::HashSet;
use mappy::MappyState;
use sdl2::rect::{Rect,Point};
use sdl2::pixels::Color;
use sdl2::event::Event;
use sdl2::keyboard::{Keycode, Scancode};
use sdl2;
use retro_rs::{Emulator,Buttons};
use std::path::Path;
use std::time::{Duration, Instant};
use mappy::TILE_SIZE;

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

    // let window2 = video_subsystem.window("Current Room", 512, 480)
    //     .build()
    //     .unwrap();

    // let mut canvas2 = window2.into_canvas().build().unwrap();

    // canvas2.set_draw_color(Color::RGB(0, 0, 0));
    // canvas2.clear();
    // canvas2.present();

    let mut event_pump = sdl_context.event_pump().unwrap();

    let tex_creator = canvas.texture_creator();
    let mut game_tex = tex_creator.create_texture(
        sdl2::pixels::PixelFormatEnum::ARGB8888,
        sdl2::render::TextureAccess::Streaming,
        w as u32,
        h as u32
    ).expect("Couldn't make game render texture");
    // let mut cur_screen_tex_size = (1,1);
    // let mut cur_screen_tex = tex_creator.create_texture(
    //     sdl2::pixels::PixelFormatEnum::ARGB32,
    //     sdl2::render::TextureAccess::Streaming,
    //     cur_screen_tex_size.0,
    //     cur_screen_tex_size.1
    // );

    let mut play_state = PlayState::Playing;
    let mut draw_grid = false;
    let mut draw_tile_standins = false;
    let mut draw_live_tracks = false;
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
        if just_pressed.contains(&Scancode::X) {
            draw_tile_standins = !draw_tile_standins;
        }
        if just_pressed.contains(&Scancode::C) {
            draw_live_tracks = !draw_live_tracks;
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
                println!("Known tiles: {:?}", mappy.tiles.gfx_count());
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
                let region = mappy.split_region();
                for x in ((region.x as u32)..(region.x as u32+region.w)).step_by(TILE_SIZE) {
                    canvas.draw_line(Point::new(x as i32*SCALE as i32,
                                                SCALE as i32*region.y),
                                     Point::new(x as i32*SCALE as i32,
                                                SCALE as i32*(region.y+region.h as i32))).unwrap();
                }
                for y in ((region.y as u32)..(region.y as u32+region.h)).step_by(TILE_SIZE) {
                    canvas.draw_line(Point::new(SCALE as i32*region.x,y as i32 * SCALE as i32),
                                     Point::new((SCALE as i32)*(region.x+region.w as i32), y as i32 * SCALE as i32)).unwrap();
                }
            }
            if draw_tile_standins {
                let region = mappy.split_region();
                let sr = mappy.current_screen.region;
                for x in ((region.x)..(region.x+region.w as i32)).step_by(TILE_SIZE) {
                    for y in ((region.y)..(region.y+region.h as i32)).step_by(TILE_SIZE) {
                        // Use tile hash and convert to a 24-bit color
                        let tile = mappy.current_screen.get(
                            sr.x+(x-region.x)/TILE_SIZE as i32,
                            sr.y+(y-region.y)/TILE_SIZE as i32);
                        let idx = tile.index();
                        if idx != 0 {
                            // TODO this but better
                            canvas.set_draw_color(
                                Color::RGB((idx*127 % 256) as u8,
                                           (idx*33 % 256) as u8,
                                           (idx*61 % 256) as u8));
                            canvas.fill_rect(Rect::new((x as u32*SCALE) as i32,
                                                       (y as u32*SCALE) as i32,
                                                       TILE_SIZE as u32*SCALE,
                                                       TILE_SIZE as u32*SCALE)).unwrap()
                        }
                    }
                }
            }
            if draw_live_tracks {
                for track in mappy.live_tracks.iter() {
                    canvas.set_draw_color(
                        Color::RGB(((track.positions[0].0).0*31 % 256) as u8,
                                   ((track.positions[0].0).0*127 % 256) as u8,
                                   ((track.positions[0].0).0*91 % 256) as u8)
                    );
                    let startp = Point::new(
                        (track.positions[0].1).0 + track.positions[0].2.x as i32 - mappy.scroll.0,
                        (track.positions[0].1).1 + track.positions[0].2.y as i32 - mappy.scroll.1
                    );
                    canvas
                        .fill_rect(
                            Rect::new((SCALE*(startp.x.max(0) as u32).max(SCALE*2)-SCALE*2) as i32,
                                      (SCALE*(startp.y.max(0) as u32).max(SCALE*2)-SCALE*2) as i32,
                                      SCALE*4, SCALE*4))
                        .expect("Couldn't draw start for track");
                    if track.positions.len() > 1 {
                        canvas.draw_lines(
                            track
                                .positions
                                .iter()
                                .filter_map(
                                    |(_,(sx,sy),sd)| {
                                        let x = sx + (sd.x as i32) - mappy.scroll.0;
                                        let y = sy + (sd.y as i32) - mappy.scroll.1;
                                        if 0 <= x && x <= (w as i32) && 0 <= y && y <= (h as i32) {
                                            Some(Point::new(x*(SCALE as i32), y*(SCALE as i32)))
                                        } else {
                                            None
                                        }
                                    })
                                .collect::<Vec<Point>>()
                                .as_slice()).expect("Couldn't draw lines for track");
                    }
                }
            }
        }
        last_pressed = now_pressed;
        canvas.present();
        let one_sixtieth = Duration::new(0, 1_000_000_000u32 / 60);
        let elapsed = frame_start.elapsed();
        if one_sixtieth > elapsed {
            ::std::thread::sleep(one_sixtieth - elapsed);
        }
    }
    mappy.dump_tiles(Path::new("../../out/"));
}
