use mappy::MappyState;
use retro_rs::Emulator;
use std::path::Path;

#[allow(unused)]
pub fn run(rom: &Path, fm2s: &[&Path]) -> MappyState {
    run_probes(rom, fm2s, &[])
}
pub fn run_probes(rom: &Path, fm2s: &[&Path], probes: &[(usize,Box<dyn Fn(&MappyState) -> ()>)]) -> MappyState {
    let mut emu = Emulator::create(Path::new("cores/fceumm_libretro"), rom);
    let mut start_state = vec![0; emu.save_size()];
    emu.save(&mut start_state);
    let mut mappy = MappyState::new(256, 240);
    let mut t = 0;
    let mut probe = 0;
    for fm2 in fm2s {
        emu.load(&start_state);
        mappy.handle_reset();
        let mut inputs = vec![];
        mappy::read_fm2(&mut inputs, fm2);
        for (_i, input_pair) in inputs.iter().enumerate() {
            if probe < probes.len() && probes[probe].0 <= t {
                dbg!(probes[probe].0, t);
                probes[probe].1(&mappy);
                probe += 1;
            }
            emu.run(*input_pair);
            mappy.process_screen(&mut emu, *input_pair);
            t += 1;
        }
    }
    mappy.finish();
    mappy
}
#[allow(unused)]
pub fn print_testcase(mappy: &MappyState) {
    let rooms = mappy.rooms.read().unwrap();
    let metarooms: Vec<_> = mappy.metarooms.metarooms().collect();
    println!("assert_eq!(rooms.len(), {:?});", rooms.len());
    println!("assert_eq!(metarooms.len(), {:?});", metarooms.len());
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
