use mappy::MappyState;
use retro_rs::{Buttons, Emulator, FramebufferToImageBuffer};
use std::path::Path;
use std::time::Instant;
fn main() {
    use std::env;
    let mut emu = Emulator::create(
        Path::new("cores/fceumm_libretro"),
        Path::new("roms/zelda.nes"),
    );
    // Have to run emu for one frame before we can get the framebuffer size
    emu.run([Buttons::new(), Buttons::new()]);
    let (w, h) = emu.framebuffer_size();
    // So reset it afterwards
    emu.reset();
    let mut inputs = vec![];
    let args: Vec<_> = env::args().collect();
    if args.len() > 1 {
        mappy::read_fm2(&mut inputs, &Path::new(&args[1]));
    }
    let mut mappy = MappyState::new(w, h);
    let start = Instant::now();
    for (_i, input_pair) in inputs.iter().enumerate() {
        emu.run(*input_pair);
        mappy.process_screen(&emu);
        // if i > 280 && i % 60 == 0 {
        // println!("Scroll: {:?} : {:?}", mappy.splits, mappy.scroll);
        // println!("Known tiles: {:?}", mappy.tiles.gfx_count());
        // }
    }
    let fb = emu.create_imagebuffer();
    fb.unwrap().save("out.png").unwrap();

    println!("Known tiles: {:?}", mappy.tiles.gfx_count());
    println!("Emulation only: 0.316842184 for 867 inputs, avg 0.00036544689850057674");
    println!(
        "Net: {:} for {:} inputs, avg {:} per frame",
        start.elapsed().as_secs_f64(),
        inputs.len(),
        start.elapsed().as_secs_f64() / (inputs.len() as f64)
    );
    mappy.dump_tiles(Path::new("out/"));
}
