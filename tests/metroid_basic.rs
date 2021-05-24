use std::path::Path;
mod common;
use common::run;
use mappy::metaroom::MetaroomID;
#[test]
fn test_metroid_basic() {
    let mappy = run(
        Path::new("roms/metroid.nes"),
        &[
            Path::new("tests/data/metroid_basic.fm2"),
        ],
    );
    let rooms = mappy.rooms.read().unwrap();
    assert_eq!(rooms.len(), 10);
    let metarooms: Vec<_> = mappy.metarooms.metarooms().collect();
    assert_eq!(metarooms.len(), 6);
    assert_eq!(rooms.len(), 10);
    assert_eq!(metarooms.len(), 6);
    assert_eq!(metarooms[0].registrations, [(2, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[0]), [MetaroomID(9)]);
    assert_eq!(metarooms[1].registrations, [(1, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[1]), [MetaroomID(2)]);
    assert_eq!(metarooms[2].registrations, [(0, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[2]), [MetaroomID(1)]);
    assert_eq!(metarooms[3].registrations, [(7, (0, 0)), (3, (0, 20))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[3]), [MetaroomID(11)]);
    assert_eq!(metarooms[4].registrations, [(8, (0, 0)), (6, (0, 0)), (4, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[4]), [MetaroomID(13), MetaroomID(9)]);
    assert_eq!(metarooms[5].registrations, [(9, (0, 0)), (5, (0, -6))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[5]), [MetaroomID(11)]);
}
