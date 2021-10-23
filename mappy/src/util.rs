impl Mappy {
    pub fn dump_map(&self, dotfolder: &Path) {
        use std::collections::BTreeMap;
        use std::fs;
        use tabbycat::attributes::*;
        use tabbycat::{AttrList, Edge, GraphBuilder, GraphType, Identity, StmtList};
        let rooms = &self.rooms.read().unwrap();
        let gname = "map".to_string();
        let node_image_paths: BTreeMap<usize, String> = self
            .metarooms
            .all_metarooms()
            .map(|mr| (mr.id.0, format!("mr_{}.png", mr.id.0)))
            .collect();
        let node_labels: BTreeMap<usize, String> = self
            .metarooms
            .metarooms()
            .map(|mr| {
                let r = mr.region(rooms);
                (
                    mr.id.0,
                    format!("{},{}<>{},{}\n", r.x, r.y, r.w, r.h)
                        + &mr
                        .registrations
                        .iter()
                        .map(|(ri, pos)| format!("{}@{},{}", ri, pos.0, pos.1))
                        .collect::<Vec<_>>()
                        .join("\n"),
                )
            })
            .collect();
        let mut all_stmts = StmtList::new();
        for mr in self.metarooms.all_metarooms() {
            self.dump_metaroom(
                &mr,
                &dotfolder.join(Path::new(&node_image_paths[&mr.id.0].clone())),
            );
        }
        for mr in self.metarooms.metarooms() {
            let mut stmts = StmtList::new();
            let mr_ident = Identity::from(mr.id.0);
            let mut attrs = AttrList::new()
                .add_pair(xlabel(&node_labels[&mr.id.0]))
                .add_pair(image(&node_image_paths[&mr.id.0]));
            if let Some(_) = mr
                .registrations
                .iter()
                .find(|(rid, _pos)| *rid == 0 || self.resets.contains(rid))
            {
                attrs = attrs.add_pair(shape(Shape::Box));
            } else {
                attrs = attrs.add_pair(shape(Shape::Plain))
            }
            stmts = stmts.add_node(mr_ident.clone(), None, Some(attrs));
            for mr2_id in self.metaroom_exits(mr) {
                stmts = stmts.add_edge(
                    Edge::head_node(mr_ident.clone(), None)
                        .arrow_to_node(Identity::from(mr2_id.0), None),
                );
            }
            all_stmts = all_stmts.extend(stmts);
        }
        let graph = GraphBuilder::default()
            .graph_type(GraphType::DiGraph)
            .strict(false)
            .id(Identity::id(&gname).unwrap())
            .stmts(all_stmts)
            .build()
            .unwrap();
        fs::write(dotfolder.join(Path::new("graph.dot")), graph.to_string()).unwrap();
    }
    pub fn dump_tiles(&self, root: &Path) {
        let mut buf = vec![0_u8; TILE_SIZE * TILE_SIZE * 3];
        for (ti, tile) in self.tiles.read().unwrap().gfx_iter().enumerate() {
            tile.write_rgb888(&mut buf);
            let img: ImageBuffer<Rgb<u8>, _> =
                ImageBuffer::from_raw(TILE_SIZE as u32, TILE_SIZE as u32, &buf[..])
                .expect("Couldn't create image buffer");
            img.save(root.join(format!("t{:}.png", ti))).unwrap();
        }
    }
    pub fn dump_tiles_single(where_to: &Path, tiles: &TileDB) {
        let all_gfx: Vec<_> = tiles.gfx_iter().collect();
        let colrows = (all_gfx.len() as f32).sqrt().ceil() as usize;
        let mut t_buf = vec![0_u8; TILE_SIZE * TILE_SIZE * 3];
        let mut buf = vec![0_u8; colrows * colrows * TILE_SIZE * TILE_SIZE * 3];
        for (ti, tile) in all_gfx.into_iter().enumerate() {
            let row = ti / colrows;
            let col = ti % colrows;
            tile.write_rgb888(&mut t_buf);
            for trow in 0..TILE_SIZE {
                let image_step = TILE_SIZE * 3;
                let image_pitch = colrows * image_step;
                let image_row_start = (row * TILE_SIZE + trow) * image_pitch + col * image_step;
                let image_row_end = (row * TILE_SIZE + trow) * image_pitch + (col + 1) * image_step;
                let tile_row_start = trow * TILE_SIZE * 3;
                let tile_row_end = (trow + 1) * TILE_SIZE * 3;
                assert_eq!(
                    image_row_end - image_row_start,
                    tile_row_end - tile_row_start
                );
                assert_eq!(tile_row_end - tile_row_start, TILE_SIZE * 3);
                for tcolor in 0..TILE_SIZE * 3 {
                    assert_eq!(buf[image_row_start + tcolor], 0);
                }
                buf[image_row_start..image_row_end]
                    .copy_from_slice(&t_buf[tile_row_start..tile_row_end]);
            }
        }
        let img: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_raw(
            colrows as u32 * TILE_SIZE as u32,
            colrows as u32 * TILE_SIZE as u32,
            &buf[..],
        )
            .expect("Couldn't create image buffer");
        img.save(where_to).unwrap();
    }

    pub fn dump_room(&self, room: &Room, at: (u32, u32), tiles_wide: u32, buf: &mut [u8]) {
        let region = room.region();
        let tiles = self.tiles.read().unwrap();
        for y in region.y..(region.y + region.h as i32) {
            for x in region.x..(region.x + region.w as i32) {
                let tile = room.get(x, y);
                let tile_change_data_db =
                    tiles.get_change_by_id(tile.unwrap_or(tiles.get_initial_change()));
                let to_tile_gfx_id = tile_change_data_db.unwrap().to;
                let corresponding_tile_gfx = tiles.get_tile_by_id(to_tile_gfx_id);
                corresponding_tile_gfx.unwrap().write_rgb888_at(
                    ((x + at.0 as i32 - region.x) * (TILE_SIZE as i32)) as usize,
                    ((y + at.1 as i32 - region.y) * (TILE_SIZE as i32)) as usize,
                    buf,
                    tiles_wide as usize * TILE_SIZE,
                );
            }
        }
    }

    pub fn dump_current_room(&self, path: &Path) {
        if self.current_room.is_none() {
            return;
        }
        let room = self.current_room.as_ref().unwrap();
        let region = room.region();
        let mut buf =
            vec![0_u8; TILE_SIZE * (region.w as usize) * TILE_SIZE * (region.h as usize) * 3];
        self.dump_room(room, (0, 0), region.w, &mut buf);
        let img = ImageBuffer::<Rgb<u8>, _>::from_raw(
            region.w * TILE_SIZE as u32,
            region.h * TILE_SIZE as u32,
            &buf[..],
        )
            .expect("Couldn't create image buffer");
        img.save(path).unwrap();
    }

    pub fn dump_metaroom(&self, mr: &Metaroom, path: &Path) {
        // need to dump every room into the same image.
        // so, first get net region of metaroom and build the image buffer.
        // then offset every reg so that the toppiest leftiest reg is at 0,0.
        let rooms = self.rooms.read().unwrap();
        let region = mr.region(&rooms);
        let mut buf =
            vec![0_u8; TILE_SIZE * (region.w as usize) * TILE_SIZE * (region.h as usize) * 3];
        for (room_i, pos) in mr.registrations.iter() {
            assert!(pos.0 - region.x >= 0);
            assert!(pos.1 - region.y >= 0);
            let new_pos = ((pos.0 - region.x) as u32, (pos.1 - region.y) as u32);
            self.dump_room(&rooms[*room_i], new_pos, region.w, &mut buf);
        }
        let img = ImageBuffer::<Rgb<u8>, _>::from_raw(
            region.w * TILE_SIZE as u32,
            region.h * TILE_SIZE as u32,
            &buf[..],
        )
            .expect("Couldn't create image buffer");
        img.save(path).unwrap();
    }
}
