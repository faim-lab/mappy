use macroquad::*;
use mappy::MappyState;
use mappy::TILE_SIZE;
use retro_rs::{Buttons, Emulator, FramebufferToImageBuffer};

use std::io::{Read, Write};
use std::path::Path;
// use std::time::Instant;

const SCALE: f32 = 3.;
const OUTPUT_INTERVAL: u64 = 19;

fn window_conf() -> Conf {
    Conf {
        window_title: "Mappy".to_owned(),
        fullscreen: false,
        window_width: 256 * SCALE as i32,
        window_height: 240 * SCALE as i32,
        ..Conf::default()
    }
}

fn replay(emu: &mut Emulator, mappy: &mut MappyState, inputs: &[[Buttons; 2]]) {
    // let start = Instant::now();
    for inp in inputs.iter() {
        emu.run(*inp);
        mappy.process_screen(emu, *inp);
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    use chrono::Local;
    use std::env;
    let romfile = Path::new("roms/zelda.nes");
    // "mario3"
    let romname = romfile.file_stem().expect("No file name!");
    let date_str = format!("{}", Local::now().format("%Y-%m-%d-%H-%M-%S"));
    let image_folder = Path::new("images/")
        .join(Path::new(&romname))
        .join(Path::new(&date_str));
    std::fs::create_dir_all(image_folder.clone()).unwrap_or_else(|_err| {});
    let csv_path = Path::new("images/")
        .join(Path::new(&romname))
        .join(Path::new(&(date_str + ".csv")));
    let mut csv = std::fs::File::create(csv_path).expect("Couldn't create CSV file");
    let mut emu = Emulator::create(Path::new("cores/fceumm_libretro"), Path::new(romfile));
    // Have to run emu for one frame before we can get the framebuffer size
    emu.run([Buttons::new(), Buttons::new()]);
    let (w, h) = emu.framebuffer_size();
    // So reset it afterwards
    emu.reset();

    assert_eq!((w, h), (256, 240));

    let mut game_img = Image::gen_image_color(w as u16, h as u16, WHITE);
    let mut fb = vec![0_u8; w * h * 4];
    let game_tex = load_texture_from_image(&game_img);
    let mut draw_grid = false;
    let mut draw_tile_standins = false;
    let mut draw_live_tracks = false;
    let mut frame_counter: u64 = 0;
    let mut inputs: Vec<[Buttons; 2]> = Vec::with_capacity(1000);
    let mut replay_inputs: Vec<[Buttons; 2]> = vec![];
    let mut replay_index: usize = 0;
    let speeds: [usize; 10] = [0, 1, 5, 15, 30, 60, 120, 240, 600, 6000];
    let mut speed: usize = 5;
    let mut accum: f32 = 0.0;
    let mut save_buf: Vec<u8> = Vec::with_capacity(emu.save_size());
    let args: Vec<_> = env::args().collect();
    let mut mappy = MappyState::new(w, h);
    if args.len() > 1 {
        mappy::read_fm2(&mut replay_inputs, &Path::new(&args[1]));
        replay(&mut emu, &mut mappy, &replay_inputs);
        inputs.extend(replay_inputs.drain(..));
    }
    let mut sx = 0;
    let mut sy = 0;

    // let start = Instant::now();
    //     println!(
    //         "Instructions
    // op changes playback speed (O for 0fps, P for 60fps)
    // wasd for directional movement
    // gh for select/start
    // j for run/throw fireball (when fiery)
    // k for jump
    // # for load inputs #
    // shift-# for dump inputs #

    // zxcvbnm,./ for debug displays"
    //     );
    loop {
        // let frame_start = Instant::now();
        if is_key_down(KeyCode::Escape) {
            break;
        }
        //space: pause/play

        //wasd: directional movement
        //g: select
        //h: start
        //j: b (run)
        //k: a (jump)

        if is_key_pressed(KeyCode::O) {
            speed = if speed == 0
                || is_key_down(KeyCode::LeftShift)
                || is_key_down(KeyCode::RightShift)
            {
                0
            } else {
                speed - 1
            };

        // println!("Speed {:?}", speed);
        } else if is_key_pressed(KeyCode::P) {
            speed = if is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift) {
                6
            } else {
                (speed + 1).min(speeds.len() - 1)
            };
            // println!("Speed {:?}", speed);
        }
        if is_key_pressed(KeyCode::Z) {
            draw_grid = !draw_grid;
        }
        if is_key_pressed(KeyCode::X) {
            draw_tile_standins = !draw_tile_standins;
        }
        if is_key_pressed(KeyCode::C) {
            draw_live_tracks = !draw_live_tracks;
        }

        let shifted = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        let numkey = {
            if is_key_pressed(KeyCode::Key0) {
                Some(0)
            } else if is_key_pressed(KeyCode::Key1) {
                Some(1)
            } else if is_key_pressed(KeyCode::Key2) {
                Some(2)
            } else if is_key_pressed(KeyCode::Key3) {
                Some(3)
            } else if is_key_pressed(KeyCode::Key4) {
                Some(4)
            } else if is_key_pressed(KeyCode::Key5) {
                Some(5)
            } else if is_key_pressed(KeyCode::Key6) {
                Some(6)
            } else if is_key_pressed(KeyCode::Key7) {
                Some(7)
            } else if is_key_pressed(KeyCode::Key8) {
                Some(8)
            } else if is_key_pressed(KeyCode::Key9) {
                Some(9)
            } else {
                None
            }
        };
        if let Some(n) = numkey {
            let path = Path::new("inputs/").join(format!(
                "{}_{}.fm2",
                romname.to_str().expect("rom name not a valid utf-8 string"),
                n
            ));
            if shifted {
                mappy::write_fm2(&inputs, &path);
            // println!("Dumped {}", n);
            } else {
                // TODO clear mappy too?
                emu.reset();
                mappy.handle_reset();
                frame_counter = 0;
                inputs.clear();
                replay_inputs.clear();
                mappy::read_fm2(&mut replay_inputs, &path);
                replay_index = 0;
            }
        }
        if is_key_pressed(KeyCode::R) {
            let save_path = Path::new("state/").join(format!(
                "{}.state",
                romname.to_str().expect("rom name not a valid utf-8 string")
            ));
            emu.save(&mut save_buf);
            //write it out to the file
            let mut file = std::fs::File::create(save_path).expect("Couldn't create save file!");
            file.write_all(&save_buf)
                .expect("Couldn't write all save file bytes!");
        }
        if is_key_pressed(KeyCode::Y) {
            let save_path = Path::new("state/").join(format!(
                "{}.state",
                romname.to_str().expect("rom name not a valid utf-8 string")
            ));
            let mut file = std::fs::File::open(save_path).expect("Couldn't open save file!");
            assert_eq!(file.read_to_end(&mut save_buf).unwrap(), emu.save_size());
            emu.load(&save_buf);
            mappy.handle_reset();
        }

        // f/s * s = how many frames
        let dt = get_frame_time();
        // Add dt (s) * multiplier (frame/s) to get number of frames.
        // e.g. 60 * 1/60 = 1
        accum += speeds[speed] as f32 * dt;
        while accum >= 1.0 {
            let buttons = if replay_index >= replay_inputs.len() {
                Buttons::new()
                    .up(is_key_down(KeyCode::W))
                    .down(is_key_down(KeyCode::S))
                    .left(is_key_down(KeyCode::A))
                    .right(is_key_down(KeyCode::D))
                    .select(is_key_down(KeyCode::G))
                    .start(is_key_down(KeyCode::H))
                    .b(is_key_down(KeyCode::J))
                    .a(is_key_down(KeyCode::K))
            } else {
                replay_index += 1;
                replay_inputs[replay_index - 1][0]
            };
            inputs.push([buttons, Buttons::new()]);
            emu.run(inputs[inputs.len() - 1]);
            if accum < 2.0 {
                // must do this here since mappy causes saves and loads, and that messes with emu's framebuffer (not updated on a load)
                emu.copy_framebuffer_rgba8888(&mut fb)
                    .expect("Couldn't copy emulator framebuffer");
            }
            mappy.process_screen(&mut emu, inputs.last().copied().unwrap());
            frame_counter += 1;
            if frame_counter % OUTPUT_INTERVAL == 0 {
                let fb_out = emu.create_imagebuffer();
                fb_out
                    .unwrap()
                    .save(format!("{}/{}.png", image_folder.display(), frame_counter))
                    .unwrap();
                println!(
                    "{},{},{}",
                    frame_counter,
                    mappy.scroll.0 - sx,
                    mappy.scroll.1 - sy
                );
                csv.write_fmt(format_args!(
                    "{},{}\n",
                    mappy.scroll.0 - sx,
                    mappy.scroll.1 - sy
                ))
                .expect("Couldn't write scroll data to csv");
                sx = mappy.scroll.0;
                sy = mappy.scroll.1;
            }
            accum -= 1.0;
        }
        let (pre, mid, post): (_, &[Color], _) = unsafe { fb.align_to() };
        assert!(pre.is_empty());
        assert!(post.is_empty());
        assert_eq!(mid.len(), w * h);
        game_img.update(&mid);
        update_texture(game_tex, &game_img);
        draw_texture_ex(
            game_tex,
            0.,
            0.,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(w as f32 * SCALE, h as f32 * SCALE)),
                ..DrawTextureParams::default()
            },
        );

        // draw mappy split
        if draw_grid {
            let region = mappy.split_region();
            for x in ((region.x as u32)..(region.x as u32 + region.w)).step_by(TILE_SIZE) {
                draw_line(
                    x as f32 * SCALE,
                    SCALE * region.y as f32,
                    x as f32 * SCALE,
                    SCALE * (region.y as f32 + region.h as f32),
                    1.,
                    RED,
                );
            }
            for y in ((region.y as u32)..(region.y as u32 + region.h)).step_by(TILE_SIZE) {
                draw_line(
                    SCALE * region.x as f32,
                    y as f32 * SCALE,
                    (SCALE) * (region.x as f32 + region.w as f32),
                    y as f32 * SCALE,
                    1.,
                    RED,
                );
            }
        }
        if draw_tile_standins {
            let region = mappy.split_region();
            let sr = mappy.current_screen.region;
            for x in ((region.x)..(region.x + region.w as i32)).step_by(TILE_SIZE) {
                for y in ((region.y)..(region.y + region.h as i32)).step_by(TILE_SIZE) {
                    // Use tile hash and convert to a 24-bit color
                    let tile = mappy.current_screen[(
                        sr.x + (x - region.x) / TILE_SIZE as i32,
                        sr.y + (y - region.y) / TILE_SIZE as i32,
                    )];
                    let idx = tile.index();
                    if idx != 0 {
                        // TODO this but better
                        draw_rectangle(
                            (x as f32 * SCALE) as f32,
                            (y as f32 * SCALE) as f32,
                            TILE_SIZE as f32 * SCALE,
                            TILE_SIZE as f32 * SCALE,
                            Color::new(
                                (idx * 127 % 256) as f32 / 255.,
                                (idx * 33 % 256) as f32 / 255.,
                                (idx * 61 % 256) as f32 / 255.,
                                1.,
                            ),
                        );
                    }
                }
            }
        }
        if draw_live_tracks {
            for track in mappy.live_tracks.iter() {
                let col = Color::new(
                    ((track.positions[0].0).0 * 31 % 256) as f32 / 255.,
                    ((track.positions[0].0).0 * 127 % 256) as f32 / 255.,
                    ((track.positions[0].0).0 * 91 % 256) as f32 / 255.,
                    1.,
                );
                let startp = Vec2::new(
                    ((track.positions[0].1).0 + track.positions[0].2.x as i32 - mappy.scroll.0)
                        as f32,
                    ((track.positions[0].1).1 + track.positions[0].2.y as i32 - mappy.scroll.1)
                        as f32,
                );
                draw_rectangle(
                    SCALE * (startp.x().max(0.)).min(w as f32) - SCALE * 2.,
                    SCALE * (startp.y().max(0.)).min(h as f32) - SCALE * 2.,
                    SCALE * 4.,
                    SCALE * 4.,
                    col,
                );
                if track.positions.len() > 1 {
                    for pair in track.positions.windows(2) {
                        let mappy::At(_, (sx0, sy0), sd0) = pair[0];
                        let x0 = sx0 + (sd0.x as i32) - mappy.scroll.0;
                        let y0 = sy0 + (sd0.y as i32) - mappy.scroll.1;
                        let mappy::At(_, (sx1, sy1), sd1) = pair[1];
                        let x1 = sx1 + (sd1.x as i32) - mappy.scroll.0;
                        let y1 = sy1 + (sd1.y as i32) - mappy.scroll.1;
                        draw_line(
                            x0 as f32 * SCALE,
                            y0 as f32 * SCALE,
                            x1 as f32 * SCALE,
                            y1 as f32 * SCALE,
                            1.,
                            col,
                        );
                    }
                }
            }
        }
        next_frame().await;
        // let frame_interval = Duration::new(0, 1_000_000_000u32 / 60);
        // // let frame_interval = Duration::new(0, 1);
        // let elapsed = frame_start.elapsed();
        // if frame_interval > elapsed {
        //     ::std::thread::sleep(frame_interval - elapsed);
        // }
    }
    mappy.finish();
    //mappy.dump_tiles(Path::new("out/"));
}
