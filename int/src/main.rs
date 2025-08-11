use macroquad::prelude::*;
use mappy::{room::Room, tile::TileDB, MappyState, TILE_SIZE};
use retro_rs::{Buttons, Emulator, FramebufferToImageBuffer};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::time::Instant;
mod affordance;
mod debug_decorate;
mod playback;
mod scroll;
use clap::Parser;
use serde::Serialize;

const SCALE: f32 = 2.0;
const OUTPUT_INTERVAL: u64 = 19;

// nested structure of JSON file
#[derive(Serialize, Default)]
struct Json {
    list: Vec<JsonEntry>,
}
#[derive(Serialize)]
struct JsonEntry {
    img_name: u64,
    scroll_position: (i32, i32),
    objects: Vec<DetectedObject>,
}
#[derive(Serialize)] // add bounding box data
struct DetectedObject {
    id: usize,
    position: (i32, i32),
    bounding_box: (i32, i32, u32, u32),
}

// Union-Find (Disjoint Set) implementation
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        UnionFind {
            parent: (0..size).collect(),
            rank: vec![0; size],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) {
        let root_x = self.find(x);
        let root_y = self.find(y);
        if root_x == root_y {
            return;
        }

        // Union by rank
        if self.rank[root_x] < self.rank[root_y] {
            self.parent[root_x] = root_y;
        } else if self.rank[root_x] > self.rank[root_y] {
            self.parent[root_y] = root_x;
        } else {
            self.parent[root_y] = root_x;
            self.rank[root_x] = self.rank[root_x].saturating_add(1);
        }
    }
}

fn is_uniform_metatile(room: &Room, tiles_db: &TileDB, x: i32, y: i32) -> bool {
    // Check if we can form a 2x2 metatile
    let neighbors = [
        room.get(x, y),
        room.get(x + 1, y),
        room.get(x, y + 1),
        room.get(x + 1, y + 1),
    ];

    // All positions must have tiles
    let [Some(a), Some(b), Some(c), Some(d)] = neighbors else {
        return false;
    };

    // All tiles must have data
    let Some(a_data) = tiles_db.get_change_by_id(a) else {
        return false;
    };
    let Some(b_data) = tiles_db.get_change_by_id(b) else {
        return false;
    };
    let Some(c_data) = tiles_db.get_change_by_id(c) else {
        return false;
    };
    let Some(d_data) = tiles_db.get_change_by_id(d) else {
        return false;
    };

    // All tiles must be the same type
    a_data.to.index() == b_data.to.index()
        && a_data.to.index() == c_data.to.index()
        && a_data.to.index() == d_data.to.index()
}

// fn is_isolated_tile(room: &Room, tiles_db: &TileDB, x: i32, y: i32) -> bool {
//     let Some(tile_id) = room.get(x, y) else {
//         return false;
//     };
//     let Some(tile_data) = tiles_db.get_change_by_id(tile_id) else {
//         return false;
//     };
//     let pattern = tile_data.to.index();

//     // Check neighbors (up, down, left, right)
//     let neighbors = [
//         room.get(x - 1, y),
//         room.get(x + 1, y),
//         room.get(x, y - 1),
//         room.get(x, y + 1),
//     ];

//     // If any neighbor has the same pattern, it's not isolated
//     !neighbors.iter().any(|n| {
//         n.and_then(|id| tiles_db.get_change_by_id(id))
//             .map(|data| data.to.index() == pattern)
//             .unwrap_or(false)
//     })
// }

#[allow(clippy::cast_possible_truncation)]
fn window_conf() -> Conf {
    Conf {
        window_title: "Mappy".to_owned(),
        fullscreen: false,
        window_width: 256 * SCALE as i32,
        window_height: 240 * SCALE as i32 + 128,
        window_resizable: false,
        ..Conf::default()
    }
}

fn replay(
    emu: &mut Emulator,
    mappy: &mut MappyState, //look up?
    inputs: &[[Buttons; 2]],
    scroll: &mut Option<&mut scroll::ScrollDumper>,
) {
    for inp in inputs {
        emu.run(*inp);
        mappy.process_screen(emu, *inp);
        if let Some(scroll) = scroll {
            scroll.update(mappy, emu);
        }
    }
}

//ADD short names AND replay file to this, -- affordance affordfile
#[derive(Parser)]
struct Cli {
    rom: std::path::PathBuf,
    affordance: Option<std::path::PathBuf>,
}

#[macroquad::main(window_conf)]
async fn main() {
    #![allow(
        clippy::too_many_lines,
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss
    )]
    use std::env;
    std::fs::create_dir_all("out").unwrap_or(());
    let args: Vec<_> = env::args().collect();

    let file_args = Cli::parse();
    //let romfile = Path::new(args[1].as_str()); //gamefile
    let romfile = file_args.rom.as_path(); //gamefile
                                           // "mario3"
    let romname = romfile.file_stem().expect("No file name!");
    std::fs::create_dir_all("inputs").unwrap_or(());
    let mut scroll_dumper: Option<scroll::ScrollDumper> = /*Some(scroll::ScrollDumper::new(
        Path::new("scroll_data/"),
        romname.to_str().unwrap(),
    ))*/ None;
    std::fs::create_dir_all("affordances").unwrap_or(());
    let mut affordances = affordance::AffordanceTracker::new(romname.to_str().unwrap());
    let afford_file = file_args.affordance.clone(); //optional affordance file

    if let Some(afford_file) = afford_file {
        affordances.load_maps(afford_file.as_path());
    }

    use chrono::Local;
    let date_str = format!("{}", Local::now().format("%Y-%m-%d-%H-%M-%S"));
    let image_folder = Path::new("images/")
        .join(Path::new(&romname))
        .join(Path::new(&date_str));
    std::fs::create_dir_all(image_folder.clone()).unwrap();
    let json_path = Path::new("images/")
        .join(Path::new(&romname))
        .join(Path::new(&(date_str.clone() + ".json")));
    let mut json = Json::default();

    // specify directory structure for DAVIS-style dataset exportation
    let dataset_images_folder = Path::new("images/datasets/")
        .join(Path::new(&romname))
        .join(Path::new(&date_str))
        .join(Path::new("images/"));
    let dataset_blob_annotations_folder = Path::new("images/datasets/")
        .join(Path::new(&romname))
        .join(Path::new(&date_str))
        .join(Path::new("annotations/blobs/"));
    let dataset_tile_annotations_folder = Path::new("images/datasets/")
        .join(Path::new(&romname))
        .join(Path::new(&date_str))
        .join(Path::new("annotations/tiles/"));
    std::fs::create_dir_all(&dataset_images_folder).unwrap();
    std::fs::create_dir_all(&dataset_blob_annotations_folder).unwrap();
    std::fs::create_dir_all(&dataset_tile_annotations_folder).unwrap();

    let mut emu = Emulator::create(Path::new("cores/fceumm_libretro"), Path::new(romfile));
    // Have to run emu for one frame before we can get the framebuffer size
    let mut start_state = vec![0; emu.save_size()];
    let mut save_buf = vec![0; emu.save_size()];
    assert!(emu.save(&mut start_state));
    assert!(emu.save(&mut save_buf));
    emu.run([Buttons::new(), Buttons::new()]);
    let (w, h) = emu.framebuffer_size();
    // So reset it afterwards
    if !emu.load(&start_state) {
        emu.reset();
    }

    //these are the visual annotations, but these are the debug annotations?
    let mut decos = {
        #[allow(clippy::wildcard_imports)]
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

    let mut frame_counter: u64 = 0;
    let mut sx = 0;
    let mut sy = 0;

    let mut playback = playback::Playback::new(); //does this just mean game play???

    let mut mappy = MappyState::new(w, h);
    if args.len() > 2 {
        mappy::read_fm2(&mut playback.replay_inputs, Path::new(&args[2]));
        replay(
            &mut emu,
            &mut mappy,
            &playback.replay_inputs,
            &mut scroll_dumper.as_mut(),
        );
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
                println!("Dumped {n}");
            } else {
                // TODO clear mappy too?
                if let Some(dump) = scroll_dumper.take() {
                    dump.finish(&playback.inputs);
                }
                scroll_dumper = None; /*Some(scroll::ScrollDumper::new(
                                          Path::new("scroll_data/"),
                                          romname.to_str().unwrap(),
                                      ));*/
                assert!(emu.load(&start_state));
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
            assert!(emu.save(&mut save_buf));
            //write it out to the file -- which files, so state, state folder doesnt currently exist?
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
            assert!(emu.load(&save_buf));
            mappy.handle_reset();
        }
        if is_key_pressed(KeyCode::F9) {
            //ADD ALSO SAVE REPLAY FILE UP TO THIS POINT

            // let timestamp = chrono::prelude::Utc::now().to_rfc3339();

            let rom: String = romfile
                .strip_prefix("roms")
                .unwrap_or(Path::new("unknownrom"))
                .display()
                .to_string();
            let filename = format!("{rom}-{date_str}.json");
            let aff_path = Path::new("affordances").join(filename);
            //let file : std::fs::File = std::fs::File::create(aff_path).unwrap();

            affordances.save(aff_path.as_path());
        }
        if is_key_pressed(KeyCode::F10) {
            // let save_path = Path::new("affordances/mario.nes-2023-11-10T17:02:52.475411+00:00.json");
            // affordances.load_maps(save_path);
        }

        //is this changing the frame rate for the ongoing play?
        // f/s * s = how many frames
        playback.step(get_frame_time(), |remaining_acc, input| {
            emu.run(input);
            // must do this before mappy processes the screen,
            // since mappy messes with the framebuffer/emulation state.
            // later, will need an early and late update?
            if let Some(dump) = scroll_dumper.as_mut() {
                dump.update(&mappy, &emu);
            }
            if remaining_acc < 2.0 {
                // must do this here since mappy causes saves and loads, and that messes with emu's framebuffer (not updated on a load)
                emu.copy_framebuffer_rgba8888(&mut fb)
                    .expect("Couldn't copy emulator framebuffer");
                game_img.bytes.copy_from_slice(&fb);
            }
            mappy.process_screen(&mut emu, input);

            frame_counter += 1;
            if frame_counter % OUTPUT_INTERVAL == 0 {
                let fb_out = emu.create_imagebuffer().unwrap();
                fb_out
                    .save(format!("{}/{}.png", image_folder.display(), frame_counter))
                    .unwrap();
                fb_out
                    .save(dataset_images_folder.join(format!("{frame_counter}.png")))
                    .unwrap();

                let mut detected_objects: Vec<DetectedObject> = vec![];
                for blob in &mappy.live_blobs {
                    let blob_pos = blob.positions.last().unwrap();
                    let curr_position = (blob_pos.1, blob_pos.2);

                    let blob_bbox = blob.bounding_boxes.last().unwrap();
                    let curr_bbox = blob_bbox.1;

                    detected_objects.push(DetectedObject {
                        id: blob.id.into(),
                        position: curr_position,
                        bounding_box: (curr_bbox.x, curr_bbox.y, curr_bbox.w, curr_bbox.h),
                    });

                    let mut mask_img = Image::gen_image_color(w as u16, h as u16, BLACK);

                    // draws one frame ahead/behind
                    for track_id in &blob.live_tracks {
                        if let Some(sd) = mappy
                            .live_track_with_id(track_id)
                            .and_then(|track| track.data_at(track.last_observation_time() - 2))
                        {
                            for (y, row) in sd.mask.iter().enumerate() {
                                for x in 0..8 {
                                    if ((row >> (7 - x)) & 0b1) == 1 {
                                        let px = u32::from(sd.x) + x;
                                        let py = u32::from(sd.y) + y as u32;
                                        if px < w as u32 && py < h as u32 {
                                            mask_img.set_pixel(px, h as u32 - py, WHITE);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Save blob mask
                    mask_img.export_png(
                        dataset_blob_annotations_folder
                            .join(format!("{}_{}.png", frame_counter, usize::from(blob.id)))
                            .to_str()
                            .unwrap(),
                    );
                }

                /*
                   for tiles:
                   screen is 2d array of tiles, can access via (x, y)
                   debug with filled white boxes
                   use info of scroll offset to know which 8x8 tile is left or right in 16x16 block

                   starting point:
                   Look for patterns of 4 adjacent 8x8 blocks --> this constitutes one 16x16 block
                   Coalesce adjacent 16x16 blocks
                */

                /*
                    Look for hflip, vflip in 8x8
                    Also look for diagonals
                 */

                // account for scrolling. check scrolling remainder by 16. if between 0 and 8, skip the first metatile or start search past that
                // alternative: check room region even or odd
                if let Some(room) = &mappy.current_room {
                    let tiles_db = mappy.tiles.read().unwrap();
                    let mut tile_counter = 0;

                    let width = room.region().w as usize;
                    let height = room.region().h as usize;
                    let metatile_width = width / 2;
                    let metatile_height = height / 2;
                    let total_metatiles = metatile_width * metatile_height;

                    let mut processed_8x8 = vec![false; width * height];
                    let mut metatile_processed = vec![false; metatile_width * metatile_height];

                    // instead of 0 to height, go from region y to region top (stay within bounds)
                    // passthrough of 16x16 and 8x8. 8x8 catches things not segmented by 16x16

                    let mut uf_meta = UnionFind::new(total_metatiles);
                    let mut metatile_patterns = vec![None; total_metatiles];

                    // First pass: Process 16x16 metatiles where possible
                    for y in 0..metatile_height {
                        for x in 0..metatile_width {
                            let idx = y * metatile_width + x;
                            let base_x = room.region().x + (x * 2) as i32;
                            let base_y = room.region().y + (y * 2) as i32;

                            if is_uniform_metatile(room, &tiles_db, base_x, base_y) {
                                let tile_ids = [
                                    room.get(base_x, base_y).unwrap(),
                                    room.get(base_x + 1, base_y).unwrap(),
                                    room.get(base_x, base_y + 1).unwrap(),
                                    room.get(base_x + 1, base_y + 1).unwrap(),
                                ];

                                let pattern =
                                    tiles_db.get_change_by_id(tile_ids[0]).unwrap().to.index();
                                metatile_patterns[idx] = Some(pattern);
                            }
                        }
                    }

                    // Group adjacent metatiles with the same pattern
                    for y in 0..metatile_height {
                        for x in 0..metatile_width {
                            let idx = y * metatile_width + x;
                            if let Some(pattern) = metatile_patterns[idx] {
                                // Check right neighbor
                                if x < metatile_width - 1 {
                                    let right_idx = idx + 1;
                                    if metatile_patterns[right_idx] == Some(pattern) {
                                        uf_meta.union(idx, right_idx);
                                    }
                                }

                                // Check bottom neighbor
                                if y < metatile_height - 1 {
                                    let bottom_idx = idx + metatile_width;
                                    if metatile_patterns[bottom_idx] == Some(pattern) {
                                        uf_meta.union(idx, bottom_idx);
                                    }
                                }
                            }
                        }
                    }

                    // Collect groups
                    let mut meta_groups: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
                    for y in 0..metatile_height {
                        for x in 0..metatile_width {
                            let idx = y * metatile_width + x;
                            if metatile_patterns[idx].is_some() {
                                let root = uf_meta.find(idx);
                                meta_groups.entry(root).or_default().push((x, y));
                            }
                        }
                    }

                    // Process metatile groups
                    for (_, positions) in meta_groups {
                        if positions.len() < 2 {
                            // Skip small groups (we'll process singles later)
                            continue;
                        }

                        let mut mask_img = Image::gen_image_color(w as u16, h as u16, BLACK);
                        let mut has_visible = false;

                        for &(x, y) in &positions {
                            let base_x = room.region().x + (x * 2) as i32;
                            let base_y = room.region().y + (y * 2) as i32;

                            let world_x = base_x * TILE_SIZE as i32;
                            let world_y = base_y * TILE_SIZE as i32;

                            let screen_x = world_x - mappy.scroll.0;
                            let screen_y = world_y - mappy.scroll.1;

                            // Only draw if at least partially visible
                            if screen_x < w as i32
                                && screen_y < h as i32
                                && screen_x + 16 >= 0
                                && screen_y + 16 >= 0
                            {
                                for py in 0..16 {
                                    for px in 0..16 {
                                        let pixel_x = screen_x + px;
                                        let pixel_y = screen_y + py;

                                        if pixel_x >= 0
                                            && pixel_x < w as i32
                                            && pixel_y >= 0
                                            && pixel_y < h as i32
                                        {
                                            mask_img.set_pixel(
                                                pixel_x as u32,
                                                h as u32 - pixel_y as u32,
                                                WHITE,
                                            );
                                            has_visible = true;
                                        }
                                    }
                                }

                                // Mark 8x8 tiles as processed
                                for dy in 0..2 {
                                    for dx in 0..2 {
                                        let tx = x * 2 + dx;
                                        let ty = y * 2 + dy;
                                        if tx < width && ty < height {
                                            processed_8x8[ty * width + tx] = true;
                                        }
                                    }
                                }
                                metatile_processed[y * metatile_width + x] = true;
                            }
                        }

                        if has_visible {
                            mask_img.export_png(
                                dataset_tile_annotations_folder
                                    .join(format!("tile_{frame_counter}_{tile_counter}.png"))
                                    .to_str()
                                    .unwrap(),
                            );
                            tile_counter += 1;
                        }
                    }
                    // Second pass: Process remaining tiles at 8x8 resolution
                    // 2. Process remaining uniform metatiles (singles)
                    for y in 0..metatile_height {
                        for x in 0..metatile_width {
                            let idx = y * metatile_width + x;
                            if metatile_processed[idx] {
                                continue;
                            }

                            if metatile_patterns[idx].is_some() {
                                let base_x = room.region().x + (x * 2) as i32;
                                let base_y = room.region().y + (y * 2) as i32;

                                let world_x = base_x * TILE_SIZE as i32;
                                let world_y = base_y * TILE_SIZE as i32;

                                let screen_x = world_x - mappy.scroll.0;
                                let screen_y = world_y - mappy.scroll.1;

                                // Only process if at least partially visible
                                if screen_x < w as i32
                                    && screen_y < h as i32
                                    && screen_x + 16 >= 0
                                    && screen_y + 16 >= 0
                                {
                                    let mut mask_img =
                                        Image::gen_image_color(w as u16, h as u16, BLACK);
                                    let mut has_visible = false;

                                    for py in 0..16 {
                                        for px in 0..16 {
                                            let pixel_x = screen_x + px;
                                            let pixel_y = screen_y + py;

                                            if pixel_x >= 0
                                                && pixel_x < w as i32
                                                && pixel_y >= 0
                                                && pixel_y < h as i32
                                            {
                                                mask_img.set_pixel(
                                                    pixel_x as u32,
                                                    h as u32 - pixel_y as u32,
                                                    WHITE,
                                                );
                                                has_visible = true;
                                            }
                                        }
                                    }

                                    if has_visible {
                                        mask_img.export_png(
                                            dataset_tile_annotations_folder
                                                .join(format!(
                                                    "tile_{frame_counter}_{tile_counter}.png"
                                                ))
                                                .to_str()
                                                .unwrap(),
                                        );
                                        tile_counter += 1;
                                    }

                                    // Mark 8x8 tiles as processed
                                    for dy in 0..2 {
                                        for dx in 0..2 {
                                            let tx = x * 2 + dx;
                                            let ty = y * 2 + dy;
                                            if tx < width && ty < height {
                                                processed_8x8[ty * width + tx] = true;
                                            }
                                        }
                                    }
                                    metatile_processed[idx] = true;
                                }
                            }
                        }
                    }

                    let mut uf_tile = UnionFind::new(width * height);
                    let mut tile_patterns = vec![None; width * height];

                    // Identify patterns for unprocessed tiles
                    for y in 0..height {
                        for x in 0..width {
                            let idx = y * width + x;
                            if processed_8x8[idx] {
                                continue;
                            }

                            let tile_x = room.region().x + x as i32;
                            let tile_y = room.region().y + y as i32;

                            if let Some(tile_id) = room.get(tile_x, tile_y) {
                                if let Some(tile_data) = tiles_db.get_change_by_id(tile_id) {
                                    tile_patterns[idx] = Some(tile_data.to.index());
                                }
                            }
                        }
                    }

                    // Group adjacent tiles with the same pattern
                    for y in 0..height {
                        for x in 0..width {
                            let idx = y * width + x;
                            if tile_patterns[idx].is_none() {
                                continue;
                            }

                            let pattern = tile_patterns[idx].unwrap();

                            // Check right neighbor
                            if x < width - 1 {
                                let right_idx = idx + 1;
                                if tile_patterns[right_idx] == Some(pattern) {
                                    uf_tile.union(idx, right_idx);
                                }
                            }

                            // Check bottom neighbor
                            if y < height - 1 {
                                let bottom_idx = idx + width;
                                if tile_patterns[bottom_idx] == Some(pattern) {
                                    uf_tile.union(idx, bottom_idx);
                                }
                            }
                        }
                    }

                    // Collect tile groups
                    let mut tile_groups: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
                    for y in 0..height {
                        for x in 0..width {
                            let idx = y * width + x;
                            if tile_patterns[idx].is_some() {
                                let root = uf_tile.find(idx);
                                tile_groups.entry(root).or_default().push((x, y));
                            }
                        }
                    }

                    // Process tile groups
                    for (_, positions) in tile_groups {
                        let mut mask_img = Image::gen_image_color(w as u16, h as u16, BLACK);
                        let mut has_visible = false;

                        for &(x, y) in &positions {
                            let tile_x = room.region().x + x as i32;
                            let tile_y = room.region().y + y as i32;

                            let world_x = tile_x * TILE_SIZE as i32;
                            let world_y = tile_y * TILE_SIZE as i32;

                            let screen_x = world_x - mappy.scroll.0;
                            let screen_y = world_y - mappy.scroll.1;

                            // Only process if at least partially visible
                            if screen_x < w as i32
                                && screen_y < h as i32
                                && screen_x + 8 >= 0
                                && screen_y + 8 >= 0
                            {
                                for py in 0..8 {
                                    for px in 0..8 {
                                        let pixel_x = screen_x + px;
                                        let pixel_y = screen_y + py;

                                        if pixel_x >= 0
                                            && pixel_x < w as i32
                                            && pixel_y >= 0
                                            && pixel_y < h as i32
                                        {
                                            mask_img.set_pixel(
                                                pixel_x as u32,
                                                h as u32 - pixel_y as u32,
                                                WHITE,
                                            );
                                            has_visible = true;
                                        }
                                    }
                                }

                                // Mark tile as processed
                                processed_8x8[y * width + x] = true;
                            }
                        }

                        if has_visible {
                            mask_img.export_png(
                                dataset_tile_annotations_folder
                                    .join(format!("tile_{frame_counter}_{tile_counter}.png"))
                                    .to_str()
                                    .unwrap(),
                            );
                            tile_counter += 1;
                        }
                    }

                    // 3. Process remaining 8x8 tiles
                    for y in 0..height {
                        for x in 0..width {
                            if processed_8x8[y * width + x] {
                                continue;
                            }

                            let tile_x = room.region().x + x as i32;
                            let tile_y = room.region().y + y as i32;

                            if let Some(tile_id) = room.get(tile_x, tile_y) {
                                if tiles_db.get_change_by_id(tile_id).is_some() {
                                    let world_x = tile_x * TILE_SIZE as i32;
                                    let world_y = tile_y * TILE_SIZE as i32;

                                    let screen_x = world_x - mappy.scroll.0;
                                    let screen_y = world_y - mappy.scroll.1;

                                    // Only process if at least partially visible
                                    if screen_x < w as i32
                                        && screen_y < h as i32
                                        && screen_x + 8 >= 0
                                        && screen_y + 8 >= 0
                                    {
                                        let mut mask_img =
                                            Image::gen_image_color(w as u16, h as u16, BLACK);
                                        let mut has_visible = false;

                                        for py in 0..8 {
                                            for px in 0..8 {
                                                let pixel_x = screen_x + px;
                                                let pixel_y = screen_y + py;

                                                if pixel_x >= 0
                                                    && pixel_x < w as i32
                                                    && pixel_y >= 0
                                                    && pixel_y < h as i32
                                                {
                                                    mask_img.set_pixel(
                                                        pixel_x as u32,
                                                        h as u32 - pixel_y as u32,
                                                        WHITE,
                                                    );
                                                    has_visible = true;
                                                }
                                            }
                                        }

                                        if has_visible {
                                            mask_img.export_png(
                                                dataset_tile_annotations_folder
                                                    .join(format!(
                                                        "tile_{frame_counter}_{tile_counter}.png"
                                                    ))
                                                    .to_str()
                                                    .unwrap(),
                                            );
                                            tile_counter += 1;
                                        }

                                        processed_8x8[y * width + x] = true;
                                    }
                                }
                            }
                        }
                    }
                }

                let json_entry = JsonEntry {
                    img_name: frame_counter,
                    scroll_position: (mappy.scroll.0 - sx, mappy.scroll.1 - sy),
                    objects: detected_objects,
                };

                json.list.push(json_entry);

                sx = mappy.scroll.0;
                sy = mappy.scroll.1;
            }
        });
        affordances.update(&mappy, &emu); //affordances updated, this adds to the game record? or just checks for inputs?

        affordances.modulate(&mappy, &emu, &game_img, &mut mod_img); //what is modulate?
        game_tex.update(&mod_img); //updating texture based on game play? or progression in recorded?
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

        for deco in &mut decos {
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

    let json_export = serde_json::to_string_pretty(&json).unwrap();
    fs::write(json_path, json_export).unwrap();
    //mappy.dump_tiles(Path::new("out/"));
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

fn dump_mappy_map(romname: &str, mappy: &MappyState) {
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
        std::fs::write(format!("out/{romname}.png"), &image).unwrap();
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
