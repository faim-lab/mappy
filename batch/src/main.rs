use mappy::MappyState;
use retro_rs::{Buttons, Emulator, FramebufferToImageBuffer};
use std::path::Path;
use std::time::Instant;
fn main() {
    use std::env;
    let args: Vec<_> = env::args().collect();
    let mut emu = Emulator::create(
        Path::new("cores/fceumm_libretro"),
        Path::new(args[1].as_str()),
    );
    let mut start_state = vec![0; emu.save_size()];
    emu.save(&mut start_state);

    // Have to run emu for one frame before we can get the framebuffer size
    emu.run([Buttons::new(), Buttons::new()]);
    let (w, h) = emu.framebuffer_size();
    let mut mappy = MappyState::new(w, h);
    let start = Instant::now();
    let mut all_inputs = 0;
    for (file_i, file) in args[2..].iter().enumerate() {
        // So reset it afterwards
        emu.load(&start_state);
        mappy.handle_reset();
        let mut inputs = vec![];
        mappy::read_fm2(&mut inputs, Path::new(file.as_str()));
        all_inputs += inputs.len();
        for (_i, input_pair) in inputs.iter().enumerate() {
            emu.run(*input_pair);
            mappy.process_screen(&mut emu, *input_pair);
        }
        let fb = emu.create_imagebuffer();
        fb.unwrap().save(format!("out/out_{}.png", file_i)).unwrap();
    }

    // mappy.dump_current_room(Path::new("out/current_room.png"));

    mappy.finish();

    println!(
        "Known tiles: {:?}",
        mappy.tiles.read().unwrap().tile_stats()
    );
    println!("Emulation only: 7.62524203s for 34,514 inputs, avg 0.000220 per frame");
    println!(
        "Net: {:} for {:} inputs, avg {:} per frame",
        start.elapsed().as_secs_f64(),
        all_inputs,
        start.elapsed().as_secs_f64() / (all_inputs as f64)
    );
    println!("{}", mappy.timers);
    mappy.dump_map(Path::new("out/"));
}
