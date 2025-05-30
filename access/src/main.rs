use macroquad::prelude::*;
// use macroquad::input::KeyCode;
use mappy::MappyState;
use retro_rs::{Buttons, Emulator};
use std::cell::RefCell;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Instant;
mod mappy_py;

const SCALE: f32 = 3.0;

#[allow(clippy::cast_possible_truncation)]
fn window_conf() -> Conf {
    Conf {
        window_title: "Mappy Access".to_owned(),
        fullscreen: false,
        window_width: 256 * SCALE as i32,
        window_height: 240 * SCALE as i32,
        ..Conf::default()
    }
}

#[allow(clippy::cast_precision_loss)]
fn replay(emu: &mut Emulator, mappy: &mut MappyState, inputs: &[[Buttons; 2]]) {
    let start = Instant::now();
    for (frames, inp) in inputs.iter().enumerate() {
        emu.run(*inp);
        mappy.process_screen(emu, *inp);
        if frames % 300 == 0 {
            println!("Scroll: {:?} : {:?}", mappy.splits, mappy.scroll);
            println!("Known tiles: {:?}", mappy.tiles.read().unwrap().gfx_count());
            println!(
                "Net: {:} for {:} inputs, avg {:}",
                start.elapsed().as_secs_f64(),
                frames,
                start.elapsed().as_secs_f64() / (frames as f64)
            );
        }
    }
}

use argh::FromArgs;
/// Mappy Access
#[derive(FromArgs)]
struct Arguments {
    /// the ROM file to load
    #[argh(positional)]
    rom: PathBuf,
    /// which replay to use, if any
    #[argh(option)]
    replay: Option<PathBuf>,
    /// path to a Python filter module
    #[argh(option)]
    filter: Option<PathBuf>,
}

#[macroquad::main(window_conf)]
async fn main() {
    #![allow(
        clippy::similar_names,
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss
    )]
    let args: Arguments = argh::from_env();

    let romfile = args.rom;
    // "mario3"
    let romname = romfile.file_stem().expect("No file name!");

    let emu = Rc::new(RefCell::new(Emulator::create(
        Path::new("cores/fceumm_libretro"),
        &romfile,
    )));
    let (start_state, mut save_buf) = {
        let mut emu = emu.borrow_mut();
        // Have to run emu for one frame before we can get the framebuffer size
        let mut start_state = vec![0; emu.save_size()];
        let mut save_buf = vec![0; emu.save_size()];
        assert!(emu.save(&mut start_state));
        assert!(emu.save(&mut save_buf));
        emu.run([Buttons::new(), Buttons::new()]);
        // So reset it afterwards
        assert!(emu.load(&start_state));
        (start_state, save_buf)
    };
    let (w, h) = emu.borrow().framebuffer_size();

    assert_eq!((w, h), (256, 240));

    let mut game_img = Image::gen_image_color(w as u16, h as u16, WHITE);
    let mut fb = vec![0_u8; w * h * 4];
    let game_tex = macroquad::texture::Texture2D::from_image(&game_img);
    let mut frame_counter: u64 = 0;
    let mut inputs: Vec<[Buttons; 2]> = Vec::with_capacity(1000);
    let mut replay_inputs: Vec<[Buttons; 2]> = vec![];
    let mut replay_index: usize = 0;
    let speeds: [usize; 10] = [0, 1, 5, 15, 30, 60, 120, 240, 300, 360];
    let mut speed: usize = 5;
    let mut accum: f32 = 0.0;
    let mappy = Rc::new(RefCell::new(MappyState::new(w, h)));
    if let Some(replayfile) = args.replay {
        mappy::read_fm2(&mut replay_inputs, &replayfile);
        replay(
            &mut emu.borrow_mut(),
            &mut mappy.borrow_mut(),
            &replay_inputs,
        );
        inputs.append(&mut replay_inputs);
    }
    let filter = args
        .filter
        .map(|fpath| mappy_py::load_module(&fpath, "access_filter"));
    let start = Instant::now();
    println!(
        "Instructions
op change playback speed (O for 0fps, P for 60fps)
wasd for directional movement
gh for select/start
j for NES \"b\" button
k for NES \"a\" button
# for load inputs #
shift-# for dump inputs #

zxcvbnm,./ for debug displays"
    );
    let mut selected_sprite = None;
    let mut selected_tile_pos = None;
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

        let shifted = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        if is_key_pressed(KeyCode::O) {
            speed = if speed == 0 || shifted { 0 } else { speed - 1 };

            println!("Speed {speed:?}");
        } else if is_key_pressed(KeyCode::P) {
            speed = if shifted {
                6
            } else {
                (speed + 1).min(speeds.len() - 1)
            };
            println!("Speed {speed:?}");
        }
        let numkey = {
            if is_key_pressed(KeyCode::Key0) {
                Some(0_u8)
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
            std::fs::create_dir_all("inputs").unwrap_or(());
            let path = Path::new("inputs/").join(format!(
                "{}_{}.fm2",
                romname.to_str().expect("rom name not a valid utf-8 string"),
                n
            ));
            if shifted {
                mappy::write_fm2(&inputs, &path);
                println!("Dumped {n}");
            } else {
                assert!(emu.borrow_mut().load(&start_state));
                mappy.borrow_mut().handle_reset();
                frame_counter = 0;
                inputs.clear();
                replay_inputs.clear();
                mappy::read_fm2(&mut replay_inputs, &path);
                replay_index = 0;
            }
        }
        if is_key_pressed(KeyCode::R) {
            std::fs::create_dir_all("state").unwrap_or(());
            let save_path = Path::new("state/").join(format!(
                "{}.state",
                romname.to_str().expect("rom name not a valid utf-8 string")
            ));
            assert!(emu.borrow().save(&mut save_buf));
            //write it out to the file
            let mut file = std::fs::File::create(save_path).expect("Couldn't create save file!");
            file.write_all(&save_buf)
                .expect("Couldn't write all save file bytes!");
        }
        if is_key_pressed(KeyCode::Y) {
            std::fs::create_dir_all("state").unwrap_or(());
            let save_path = Path::new("state/").join(format!(
                "{}.state",
                romname.to_str().expect("rom name not a valid utf-8 string")
            ));
            let mut file = std::fs::File::open(save_path).expect("Couldn't open save file!");
            file.read_exact(&mut save_buf).unwrap();
            assert!(emu.borrow_mut().load(&save_buf));
            mappy.borrow_mut().handle_reset();
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
            emu.borrow_mut().run(inputs[inputs.len() - 1]);
            if accum < 2.0 {
                // must do this here since mappy causes saves and loads, and that messes with emu's framebuffer (not updated on a load)
                emu.borrow()
                    .copy_framebuffer_rgba8888(&mut fb)
                    .expect("Couldn't copy emulator framebuffer");
            }
            // wait for sprite updates...
            mappy
                .borrow_mut()
                .process_screen(&mut emu.borrow_mut(), inputs.last().copied().unwrap());
            // then filter
            if accum < 2.0 {
                if let Some(filter_mod) = &filter {
                    use pyo3::types::PyByteArrayMethods;
                    mappy_py::with_mappy(Rc::clone(&emu), Rc::clone(&mappy), |py, mappy_py| {
                        let filter = filter_mod
                            .getattr(py, "filter")
                            .expect("Python filter module does not define `filter` function");
                        let fb_len = fb.len();
                        // two copies, maybe we could avoid this later with buffer protocol somehow.
                        let fb_py = pyo3::types::PyByteArray::new(py, &fb);
                        let fb_py_view = pyo3::types::PyMemoryView::from(&fb_py)
                            .expect("Couldn't create memory view from PyByteArray");
                        filter
                            .call1(py, (mappy_py, fb_py_view))
                            .unwrap_or_else(|e| {
                                // We can't display Python exceptions via std::fmt::Display,
                                // so print the error here manually.
                                e.print_and_set_sys_last_vars(py);
                                panic!();
                            });
                        // This second copy will also disappear if we can eliminate the first
                        unsafe {
                            // This is safe because the PyByteArray
                            // will be dropped and not reused.  Also,
                            // the bytes get copied out (but if the
                            // copy can be avoided, it would still be
                            // safe, since the buffer won't get
                            // modified by Rust while the PyByteArray
                            // is live)
                            fb.copy_from_slice(&fb_py.as_bytes()[..fb_len]);
                        }
                    });
                }
            }
            frame_counter += 1;
            if frame_counter % 300 == 0 {
                // println!("Scroll: {:?} : {:?}", mappy.splits, mappy.scroll);
                // println!("Known tiles: {:?}", mappy.tiles.gfx_count());
                println!(
                    "Net: {:} for {:} inputs, avg {:}",
                    start.elapsed().as_secs_f64(),
                    frame_counter,
                    start.elapsed().as_secs_f64() / (frame_counter as f64)
                );
            }
            accum -= 1.0;
        }
        game_img.bytes.copy_from_slice(&fb);
        game_tex.update(&game_img);
        draw_texture_ex(
            &game_tex,
            0.,
            0.,
            WHITE,
            DrawTextureParams {
                dest_size: Some(Vec2::new(w as f32 * SCALE, h as f32 * SCALE)),
                ..DrawTextureParams::default()
            },
        );
        {
            let mappy = mappy.borrow();
            if is_mouse_button_down(MouseButton::Left) && mappy.current_room.is_some() {
                let (tx, ty) = screen_f32_to_tile(mouse_position(), &mappy);
                selected_tile_pos = Some((tx, ty));
            }
            if let Some((tx, ty)) = selected_tile_pos {
                let (sx, sy) = tile_to_screen((tx, ty), &mappy);
                draw_rectangle_lines(sx, sy, 8.0 * SCALE, 8.0 * SCALE, 1.0 * SCALE, RED);
                if let Some(change) = mappy.current_room.as_ref().unwrap().get(tx, ty) {
                    let tiles = mappy.tiles.read().unwrap();
                    let change_data = tiles.get_change_by_id(change);
                    if let Some(cd) = change_data {
                        let to = cd.to;
                        let tile = tiles.get_tile_by_id(to).unwrap();
                        println!("T: {},{} -- {:?}", tx, ty, tile.perceptual_hash());
                        draw_text(
                            &format!("{},{} -- {:?}", tx, ty, tile.perceptual_hash()),
                            SCALE,
                            SCALE * 16.0,
                            SCALE * 16.0,
                            RED,
                        );
                    }
                }
            }
            if is_mouse_button_down(MouseButton::Left) {
                // selected_sprite = None;
                for track in &mappy.live_tracks {
                    let (mx, my) = mouse_position();
                    if mappy::sprites::overlapping_sprite(
                        (mx / SCALE) as usize,
                        (my / SCALE) as usize,
                        2,
                        2,
                        &[*track.current_data()],
                    ) {
                        selected_sprite = Some(track.id);
                    }
                }
            }
            if let Some(track) = selected_sprite
                .and_then(|track_id| mappy.live_tracks.iter().find(|t| t.id == track_id))
            {
                let (wx, wy) = track.current_point();
                let (base_sx, base_sy) = mappy.world_to_screen(wx, wy);
                draw_rectangle_lines(
                    base_sx as f32 * SCALE,
                    base_sy as f32 * SCALE,
                    8.0 * SCALE,
                    f32::from(track.current_data().height()) * SCALE,
                    1.0 * SCALE,
                    BLUE,
                );
                let data = track.current_data();
                let (px, py) = track.current_point();
                println!("S: {},{} -- {}", px, py, data.key());
                draw_text(
                    &format!("{},{} -- {}", wx, wy, data.key()),
                    w as f32 * SCALE - 100.0 * SCALE,
                    SCALE * 16.0,
                    SCALE * 16.0,
                    BLUE,
                );
            }
        }
        next_frame().await;
    }
    mappy.borrow_mut().finish();
    println!("{}", mappy.borrow().timers);
}

#[allow(clippy::cast_possible_truncation)]
fn screen_f32_to_tile((x, y): (f32, f32), mappy: &MappyState) -> (i32, i32) {
    let x = (x / SCALE) as i32;
    let y = (y / SCALE) as i32;
    mappy.screen_to_tile(x, y)
}
#[allow(clippy::cast_precision_loss)]
fn tile_to_screen((x, y): (i32, i32), mappy: &MappyState) -> (f32, f32) {
    let (x, y) = mappy.tile_to_screen(x, y);
    (x as f32 * SCALE, y as f32 * SCALE)
}
