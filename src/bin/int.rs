use macroquad::*;
use mappy::MappyState;
use mappy::TILE_SIZE;
use retro_rs::{Buttons, Emulator};

use std::path::Path;
use std::time::Instant;

#[derive(PartialEq, Eq, Debug)]
enum PlayState {
    Paused,
    Playing,
}

const SCALE: f32 = 3.;

fn window_conf() -> Conf {
    Conf {
        window_title: "Mappy".to_owned(),
        fullscreen: false,
        window_width: 256 * SCALE as i32,
        window_height: 240 * SCALE as i32,
        ..Conf::default()
    }
}

use argh::FromArgs;

#[derive(FromArgs, PartialEq, Debug)]
/// A command with positional arguments.
struct Args {
    #[argh(positional)]
    /// won't allow Path, REMEMBER to turn String to Path later on!!
    romfile: String,
    #[argh(option, short = 'r')]
    /// replay
    replay: Option<String>,
    #[argh(option, short = 's')]
    /// save
    save: Option<String>,
}

#[macroquad::main(window_conf)]
async fn main() {
    use std::env;
    // running cargo run --bin int help will give instructions
    let args: Args = argh::from_env();
    let romfile: &str = &args.romfile;
    dbg!(&args);
    // let rompath = String::from("roms/") + romfile;
    // let rom: &str = &rompath;
    // println!("{}", rompath);
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
    let mut play_state = PlayState::Playing;
    let mut draw_grid = false;
    let mut draw_tile_standins = false;
    let mut draw_live_tracks = false;
    let mut frame_counter: u64 = 0;
    let mut inputs: Vec<[Buttons; 2]> = Vec::with_capacity(1000);
    let mut replay_inputs: Vec<[Buttons; 2]> = vec![];
    let mut replay_index = 0;
    // let args: Vec<_> = env::args().collect();
    if let Some(input_path) = args.replay {
        mappy::read_fm2(&mut replay_inputs, &Path::new(&input_path));
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
    loop {
        let frame_start = Instant::now();
        if is_key_down(KeyCode::Escape) {
            break;
        }
        //space: pause/play

        //wasd: directional movement
        //g: select
        //h: start
        //j: b (run)
        //k: a (jump)

        if is_key_pressed(KeyCode::Space) {
            play_state = match play_state {
                PlayState::Paused => PlayState::Playing,
                PlayState::Playing => PlayState::Paused,
            };
            println!("Toggle playing to: {:?}", play_state);
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
            let path = Path::new("inputs/").join(format!("mario_{}.fm2", n));
            if shifted {
                mappy::write_fm2(&inputs, &path);
                println!("Dumped {}", n);
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
            emu.copy_framebuffer_rgba8888(&mut fb)
                .expect("Couldn't copy emulator framebuffer");
            let (pre, mid, post): (_, &[Color], _) = unsafe { fb.align_to() };
            assert!(pre.is_empty());
            assert!(post.is_empty());
            assert_eq!(mid.len(), w * h);
            game_img.update(&mid);
            update_texture(game_tex, &game_img);
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
        }
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
                    let tile = mappy.current_screen.get(
                        sr.x + (x - region.x) / TILE_SIZE as i32,
                        sr.y + (y - region.y) / TILE_SIZE as i32,
                    );
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
                        let (_, (sx0, sy0), sd0) = pair[0];
                        let x0 = sx0 + (sd0.x as i32) - mappy.scroll.0;
                        let y0 = sy0 + (sd0.y as i32) - mappy.scroll.1;
                        let (_, (sx1, sy1), sd1) = pair[1];
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
    //mappy.dump_tiles(Path::new("out/"));
}
