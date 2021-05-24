use std::path::Path;
mod common;
use common::run;
#[test]
fn test_mario_11_ab() {
    let mappy = run(
        Path::new("roms/mario.nes"),
        &[
            Path::new("tests/data/mario_11_a.fm2"),
            Path::new("tests/data/mario_11_b.fm2"),
        ],
    );
    let rooms = mappy.rooms.read().unwrap();
    assert_eq!(rooms.len(), 4);
    let metarooms: Vec<_> = mappy.metarooms.metarooms().collect();
    assert_eq!(metarooms.len(), 2);
    assert_eq!(metarooms[0].registrations, vec![(1, (0, 0)),]);
    assert_eq!(
        metarooms[1].registrations,
        vec![(3, (0, 0)), (2, (320, 0)), (0, (0, 0)),]
    );
    let exits1 = mappy.metaroom_exits(metarooms[0]);
    assert_eq!(exits1, vec![metarooms[1].id]);
    let exits2 = mappy.metaroom_exits(metarooms[1]);
    assert_eq!(exits2, vec![metarooms[0].id]);
}
