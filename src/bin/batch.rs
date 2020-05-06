use retro_rs::{Emulator, Buttons};
use mappy::MappyState;
use std::path::Path;
use std::time::Instant;
fn main() {
    let mut emu = Emulator::create(
        Path::new("../../cores/fceumm_libretro"),
        Path::new("../../roms/mario.nes"),
    );
    // Have to run emu for one frame before we can get the framebuffer size
    emu.run([Buttons::new(), Buttons::new()]);
    let (w,h) = emu.framebuffer_size();
    // So reset it afterwards
    emu.reset();
    let mut mappy = MappyState::new(w,h);
    let start = Instant::now();
    let inputs = vec![[Buttons::new(), Buttons::new()];360];
    for input_pair in inputs.iter() {
        emu.run(*input_pair);
        mappy.process_screen(&emu);
        println!("Scroll: {:?} : {:?}",mappy.splits, mappy.scroll);
        println!("Known tiles: {:?}", mappy.tiles.len());
    }
    println!("Emulation only: 0.110773661 for 360 inputs, avg 0.00030770523055555557 per frame");
    println!("Net: {:} for {:} inputs, avg {:} per frame",
             start.elapsed().as_secs_f64(),
             inputs.len(),
             start.elapsed().as_secs_f64()/(inputs.len() as f64));
    assert!(mappy.tiles.len() == 80);
    mappy.dump_tiles(Path::new("../../out/"));
}
