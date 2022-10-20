use macroquad::prelude::*;
use mappy::{MappyState, TILE_SIZE};
use retro_rs::{Buttons, Emulator};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;
mod affordance;
mod debug_decorate;
mod playback;
mod scroll;

const SCALE: f32 = 3.0;

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
    for (_frames, inp) in inputs.iter().enumerate() {
        emu.run(*inp);
        mappy.process_screen(emu, *inp);
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    use std::env;
    let args: Vec<_> = env::args().collect();

    let romfile = Path::new(args[1].as_str());
    // "mario3"
    let romname = romfile.file_stem().expect("No file name!");
    std::fs::create_dir_all("inputs").unwrap_or(());
    let mut scroll_dumper: Option<scroll::ScrollDumper> = None; /*Some(scroll::ScrollDumper::new(
                                                                    Path::new("images/"),
                                                                    romname.to_str().unwrap(),
                                                                ));*/
    let mut affordances = affordance::AffordanceTracker::new(romname.to_str().unwrap());
    let mut emu = Emulator::create(
        Path::new("../libretro-fceumm/fceumm_libretro"),
        Path::new(romfile),
    );
    // let mut emu = Emulator::create(Path::new("cores/fceumm_libretro"), Path::new(romfile));
    // Have to run emu for one frame before we can get the framebuffer size
    let mut start_state = vec![0; emu.save_size()];
    let mut save_buf = vec![0; emu.save_size()];
    emu.save(&mut start_state);
    emu.save(&mut save_buf);
    emu.run([Buttons::new(), Buttons::new()]);
    let (w, h) = emu.framebuffer_size();
    // So reset it afterwards
    emu.load(&start_state);

    let mut decos = {
        use debug_decorate::*;
        vec![
            Decorator {
                deco: Box::new(Grid {}),
                enabled: false,
                toggle: KeyCode::Z,
            },
            Decorator {
                deco: Box::new(TileStandin {}),
                enabled: false,
                toggle: KeyCode::X,
            },
            Decorator {
                deco: Box::new(LiveTracks { dims: (w, h) }),
                enabled: false,
                toggle: KeyCode::C,
            },
            Decorator {
                deco: Box::new(LiveBlobs {}),
                enabled: false,
                toggle: KeyCode::B,
            },
            Decorator {
                deco: Box::new(Avatar {}),
                enabled: false,
                toggle: KeyCode::M,
            },
            Decorator {
                deco: Box::new(Recording {}),
                enabled: true,
                toggle: KeyCode::F20,
            },
            Decorator {
                deco: Box::new(SelectedTile {
                    selected_tile_pos: None,
                }),
                enabled: false,
                toggle: KeyCode::F20,
            },
            Decorator {
                deco: Box::new(SelectedSprite {
                    selected_sprite: None,
                    dims: (w, h),
                }),
                enabled: false,
                toggle: KeyCode::F20,
            },
        ]
    };

    assert_eq!((w, h), (256, 240));

    let mut game_img = Image::gen_image_color(w as u16, h as u16, WHITE);
    let mut mod_img = Image::gen_image_color(w as u16, h as u16, WHITE);
    let mut fb = vec![0_u8; w * h * 4];
    let game_tex = macroquad::texture::Texture2D::from_image(&game_img);

    let mut playback = playback::Playback::new();

    let mut mappy = MappyState::new(w, h);
    if args.len() > 2 {
        mappy::read_fm2(&mut playback.replay_inputs, Path::new(&args[2]));
        replay(&mut emu, &mut mappy, &playback.replay_inputs);
        playback.inputs.append(&mut playback.replay_inputs);
    }
    playback.start = Instant::now();

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
        playback.update_speed();
        if is_key_pressed(KeyCode::N) {
            dump_mappy_map(romname.to_str().unwrap(), &mappy);
        }
        // if is_key_pressed(KeyCode::M) {
        //      std::fs::remove_dir_all("out/rooms").unwrap_or(());
        //      std::fs::create_dir_all("out/rooms").unwrap();
        //      mappy.dump_rooms(Path::new("out/rooms"));
        //  }

        let shifted = is_key_down(KeyCode::LeftShift) || is_key_down(KeyCode::RightShift);
        if let Some(n) = pressed_numkey() {
            let path = Path::new("inputs/").join(format!(
                "{}_{}.fm2",
                romname.to_str().expect("rom name not a valid utf-8 string"),
                n
            ));
            if shifted {
                mappy::write_fm2(&playback.inputs, &path);
                println!("Dumped {}", n);
            } else {
                // TODO clear mappy too?
                if let Some(dump) = scroll_dumper.take() {
                    dump.finish(&playback.inputs);
                }
                scroll_dumper = Some(scroll::ScrollDumper::new(
                    Path::new("images/"),
                    romname.to_str().unwrap(),
                ));
                emu.load(&start_state);
                mappy.handle_reset();
                playback.replay(&path);
            }
        }
        if is_key_pressed(KeyCode::R) {
            std::fs::create_dir_all("state").unwrap_or(());
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
            // This kind of clobbers the input record, if loads aren't part of the input sequence.
            // TODO: deal with this probably by making inputs a sequence of buttons PLUS loads
            std::fs::create_dir_all("state").unwrap_or(());
            let save_path = Path::new("state/").join(format!(
                "{}.state",
                romname.to_str().expect("rom name not a valid utf-8 string")
            ));
            let mut file = std::fs::File::open(save_path).expect("Couldn't open save file!");
            file.read_exact(&mut save_buf).unwrap();
            emu.load(&save_buf);
            mappy.handle_reset();
        }

        // f/s * s = how many frames
        playback.step(get_frame_time(), |remaining_acc, input| {
            emu.run(input);
            if remaining_acc < 2.0 {
                // must do this here since mappy causes saves and loads, and that messes with emu's framebuffer (not updated on a load)
                emu.copy_framebuffer_rgba8888(&mut fb)
                    .expect("Couldn't copy emulator framebuffer");
                game_img.bytes.copy_from_slice(&fb);
            }
            mappy.process_screen(&mut emu, input);
            if let Some(dump) = scroll_dumper.as_mut() {
                dump.update(&mappy, &mut fb, &emu);
            }
        });
        affordances.update(&mappy, &emu);
        affordances.modulate(&mappy, &emu, &game_img, &mut mod_img);
        game_tex.update(&mod_img);
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

        for deco in decos.iter_mut() {
            if is_key_pressed(deco.toggle) {
                deco.enabled = !deco.enabled;
            }
            if deco.enabled {
                deco.deco.draw(&mappy);
            }
        }

        next_frame().await;
    }
    mappy.finish();
    println!("{}", mappy.timers);
    if let Some(dump) = scroll_dumper.take() {
        dump.finish(&playback.inputs);
    }
    //mappy.dump_tiles(Path::new("out/"));
}

fn screen_f32_to_tile((x, y): (f32, f32), mappy: &MappyState) -> (i32, i32) {
    let x = (x / SCALE) as i32;
    let y = (y / SCALE) as i32;
    mappy.screen_to_tile(x, y)
}
fn tile_to_screen((x, y): (i32, i32), mappy: &MappyState) -> (f32, f32) {
    let (x, y) = mappy.tile_to_screen(x, y);
    (x as f32 * SCALE, y as f32 * SCALE)
}

fn dump_mappy_map(romname: &str, mappy: &MappyState) {
    std::fs::create_dir_all("out").unwrap_or(());
    mappy.dump_map(Path::new("out/"));
    {
        use std::process::Command;
        let image = Command::new("dot")
            .current_dir("out")
            .arg("-T")
            .arg("png")
            .arg("graph.dot")
            .output()
            .expect("graphviz failed")
            .stdout;
        std::fs::write(format!("out/{}.png", romname), &image).unwrap();
    }
}
fn pressed_numkey() -> Option<usize> {
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
}
