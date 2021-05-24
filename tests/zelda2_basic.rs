use std::path::Path;
mod common;
use common::run;
#[test]
fn test_mario_11_ab() {
    let mappy = run(
        Path::new("roms/zelda2.nes"),
        &[
            Path::new("tests/data/zelda2_basic.fm2"),
        ],
    );
    let rooms = mappy.rooms.read().unwrap();
    assert_eq!(rooms.len(), 14);
    let metarooms: Vec<_> = mappy.metarooms.metarooms().collect();
    assert_eq!(metarooms.len(), 8);
    // once it's basically working: common::print_testcase(...); panic!();
}
