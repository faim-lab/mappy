use std::path::Path;
mod common;
use common::run;
#[test]
fn test_zelda_d1() {
    use mappy::metaroom::MetaroomID;
    let mappy = run(
        Path::new("roms/zelda.nes"),
        &[Path::new("tests/data/zelda_d1.fm2")],
    );
    let rooms = mappy.rooms.read().unwrap();
    assert_eq!(rooms.len(), 52);
    let metarooms: Vec<_> = mappy.metarooms.metarooms().collect();
    assert_eq!(metarooms.len(), 30);
    assert_eq!(metarooms[0].registrations, [(51, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[0]), []);
    assert_eq!(metarooms[1].registrations, [(50, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[1]), [MetaroomID(73)]);
    assert_eq!(metarooms[2].registrations, [(43, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[2]), [MetaroomID(69)]);
    assert_eq!(metarooms[3].registrations, [(42, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[3]), [MetaroomID(59)]);
    assert_eq!(metarooms[4].registrations, [(12, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[4]), [MetaroomID(55)]);
    assert_eq!(metarooms[5].registrations, [(11, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[5]), [MetaroomID(13)]);
    assert_eq!(metarooms[6].registrations, [(7, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[6]), [MetaroomID(9)]);
    assert_eq!(metarooms[7].registrations, [(10, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[7]), [MetaroomID(12)]);
    assert_eq!(
        metarooms[8].registrations,
        [(35, (0, 0)), (33, (0, 0)), (31, (0, 0)), (25, (0, 0))]
    );
    assert_eq!(
        mappy.metaroom_exits(&metarooms[8]),
        [MetaroomID(67), MetaroomID(44)]
    );
    assert_eq!(metarooms[9].registrations, [(5, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[9]), [MetaroomID(7)]);
    assert_eq!(metarooms[10].registrations, [(2, (0, 0)), (0, (0, 0))]);
    assert_eq!(
        mappy.metaroom_exits(&metarooms[10]),
        [MetaroomID(4), MetaroomID(1)]
    );
    assert_eq!(metarooms[11].registrations, [(22, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[11]), [MetaroomID(65)]);
    assert_eq!(metarooms[12].registrations, [(9, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[12]), [MetaroomID(11)]);
    assert_eq!(metarooms[13].registrations, [(3, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[13]), [MetaroomID(5)]);
    assert_eq!(metarooms[14].registrations, [(28, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[14]), [MetaroomID(34)]);
    assert_eq!(metarooms[15].registrations, [(45, (0, 0)), (20, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[15]), [MetaroomID(65)]);
    assert_eq!(metarooms[16].registrations, [(1, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[16]), [MetaroomID(3)]);
    assert_eq!(
        metarooms[17].registrations,
        [(34, (0, 0)), (30, (0, 0)), (26, (0, 0))]
    );
    assert_eq!(
        mappy.metaroom_exits(&metarooms[17]),
        [MetaroomID(46), MetaroomID(34)]
    );
    assert_eq!(metarooms[18].registrations, [(4, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[18]), [MetaroomID(6)]);
    assert_eq!(metarooms[19].registrations, [(8, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[19]), [MetaroomID(10)]);
    assert_eq!(metarooms[20].registrations, [(16, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[20]), [MetaroomID(55)]);
    assert_eq!(metarooms[21].registrations, [(14, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[21]), [MetaroomID(55)]);
    assert_eq!(metarooms[22].registrations, [(29, (0, 0)), (27, (0, 0))]);
    assert_eq!(
        mappy.metaroom_exits(&metarooms[22]),
        [MetaroomID(44), MetaroomID(32)]
    );
    assert_eq!(metarooms[23].registrations, [(6, (0, 0))]);
    assert_eq!(mappy.metaroom_exits(&metarooms[23]), [MetaroomID(8)]);
    assert_eq!(
        metarooms[24].registrations,
        [(40, (0, 0)), (17, (0, 0)), (15, (0, 0)), (13, (0, 0))]
    );
    assert_eq!(
        mappy.metaroom_exits(&metarooms[24]),
        [MetaroomID(57), MetaroomID(18), MetaroomID(15)]
    );
    assert_eq!(metarooms[25].registrations, [(41, (0, 0)), (18, (0, 0))]);
    assert_eq!(
        mappy.metaroom_exits(&metarooms[25]),
        [MetaroomID(58), MetaroomID(69)]
    );
    assert_eq!(
        metarooms[26].registrations,
        [(46, (0, 0)), (23, (0, 0)), (21, (0, 0))]
    );
    assert_eq!(
        mappy.metaroom_exits(&metarooms[26]),
        [MetaroomID(67), MetaroomID(25)]
    );
    assert_eq!(
        metarooms[27].registrations,
        [(47, (0, 0)), (36, (0, 0)), (32, (0, 0)), (24, (0, 0))]
    );
    assert_eq!(
        mappy.metaroom_exits(&metarooms[27]),
        [MetaroomID(69), MetaroomID(46)]
    );
    assert_eq!(
        metarooms[28].registrations,
        [
            (48, (0, 0)),
            (44, (0, 0)),
            (38, (0, 0)),
            (37, (0, 0)),
            (19, (0, 0))
        ]
    );
    assert_eq!(
        mappy.metaroom_exits(&metarooms[28]),
        [MetaroomID(71), MetaroomID(63), MetaroomID(69)]
    );
    assert_eq!(metarooms[29].registrations, [(49, (0, 0)), (39, (0, 0))]);
    assert_eq!(
        mappy.metaroom_exits(&metarooms[29]),
        [MetaroomID(72), MetaroomID(55)]
    );
}
