use mappy::MappyState;
use retro_rs::Emulator;
use std::path::Path;

pub fn run(rom: &Path, fm2s: &[&Path]) -> MappyState {
    let mut emu = Emulator::create(Path::new("cores/fceumm_libretro"), rom);
    let mut start_state = vec![0; emu.save_size()];
    emu.save(&mut start_state);
    let mut mappy = MappyState::new(256, 240);
    for fm2 in fm2s {
        emu.load(&start_state);
        mappy.handle_reset();
        let mut inputs = vec![];
        mappy::read_fm2(&mut inputs, fm2);
        for (_i, input_pair) in inputs.iter().enumerate() {
            emu.run(*input_pair);
            mappy.process_screen(&mut emu);
        }
    }
    mappy.finish();
    mappy
}
#[allow(unused)]
pub fn print_testcase(mappy: MappyState, rooms: &[mappy::room::Room], metarooms: &[mappy::metaroom::Metaroom]) {
    println!(
        "assert_eq!(rooms.len(), {:?});",
        rooms.len()
    );
    println!(
        "assert_eq!(metarooms.len(), {:?});",
        metarooms.len()
    );
    for (mi, m) in metarooms.iter().enumerate() {
        println!(
            "assert_eq!(metarooms[{}].registrations, {:?});",
            mi, m.registrations
        );
        println!(
            "assert_eq!(mappy.metaroom_exits(&metarooms[{}]), {:?});",
            mi,
            mappy.metaroom_exits(m)
        );
    }
}
