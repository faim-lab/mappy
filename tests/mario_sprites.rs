use std::path::Path;
use mappy::MappyState;
mod common;
use common::run_probes;
#[test]
fn test_mario_sprites() {
    let mappy = run_probes(
        Path::new("roms/mario.nes"),
        &[Path::new("tests/data/mario_sprites.fm2")],
        &[(375, Box::new(|mappy:&MappyState| {
            dbg!(mappy.now);
            assert_eq!(mappy.live_tracks.len(), 9);
            assert_eq!(mappy.live_blobs.len(), 1);
        })),
          (376, Box::new(|mappy:&MappyState| {
              assert_eq!(mappy.live_tracks.len(), 9);
              assert_eq!(mappy.live_blobs.len(), 1);
          }))
        ]
    );
    assert_eq!(mappy.live_tracks.len(), 9);
    assert_eq!(mappy.live_blobs.len(), 1);
}
