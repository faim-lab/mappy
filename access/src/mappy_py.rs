use mappy::MappyState;
use pyo3::prelude::*;
use retro_rs::Emulator;
use std::cell::RefCell;
use std::rc::Rc;

pub fn load_module(fpath: &std::path::Path, name: &str) -> Py<PyModule> {
    Python::with_gil(|py| {
        let basepath = fpath.parent().unwrap_or_else(|| std::path::Path::new("."));
        pyo3::py_run!(py, basepath, r#"import sys; sys.path.append(basepath);"#);
        PyModule::from_code(
            py,
            std::ffi::CString::new(std::fs::read_to_string(fpath).expect("Python module: file not found at given path")).as_ref().unwrap(),
            &std::ffi::CString::new(fpath.to_str().unwrap()).unwrap(),
            &std::ffi::CString::new(name).unwrap(),
        )
        .expect("Invalid Python filter module")
        .into()
    })
}

pub fn with_mappy<F, T>(emulator: Rc<RefCell<Emulator>>, mappy: Rc<RefCell<MappyState>>, f: F) -> T
where
    F: for<'p> FnOnce(Python<'p>, self::Mappy) -> T,
{
    let (width, height) = emulator.borrow().framebuffer_size();
    Python::with_gil(|py| {
        f(
            py,
            self::Mappy {
                emulator,
                mappy,
                width,
                height,
            },
        )
    })
}

#[pyclass(unsendable)]
pub struct Mappy {
    mappy: Rc<RefCell<MappyState>>,
    #[allow(dead_code)]
    emulator: Rc<RefCell<Emulator>>,
    #[pyo3(get)]
    width: usize,
    #[pyo3(get)]
    height: usize,
    // TODO: mouse position, held keys
}

#[pymethods]
impl Mappy {
    #[getter]
    fn scroll(&self) -> PyResult<(i32, i32)> {
        Ok(self.mappy.borrow().scroll)
    }
    #[getter]
    fn sprites(&self) -> PyResult<Vec<Sprite>> {
        Ok(self
            .mappy
            .borrow()
            .prev_sprites
            .iter()
            .filter_map(|s| {
                if s.is_valid() {
                    Some(Sprite { sprite: *s })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>())
    }
    #[getter]
    fn playfield(&self) -> PyResult<(i32, i32, u32, u32)> {
        let mappy::Rect { x, y, w, h } = self.mappy.borrow().split_region();
        Ok((x, y, w, h))
    }
    fn screen_tile_at(&self, x: i32, y: i32) -> PyResult<u32> {
        let mappy = self.mappy.borrow();
        let t = mappy.current_room.as_ref().unwrap().get(x, y).unwrap();
        Ok(t.index())
    }
    fn tile_hash(&self, idx: usize) -> PyResult<u128> {
        let mappy = self.mappy.borrow();
        let db = mappy.tiles.read().unwrap();
        let tgfx = db.get_tile_by_index(idx).unwrap();
        Ok(tgfx.perceptual_hash())
    }
    /// Fills `into` with the pixels of tile_gfx.
    fn read_tile_gfx(&self, idx: usize, into: &Bound<'_,pyo3::types::PyByteArray>) -> PyResult<()> {
        use pyo3::types::PyByteArrayMethods;
        let mappy = self.mappy.borrow();
        let db = mappy.tiles.read().unwrap();
        assert!(into.len() >= mappy::tile::TILE_NUM_PX * 3);
        let tgfx = db.get_tile_by_index(idx).unwrap();
        tgfx.write_rgb888(unsafe { into.as_bytes_mut() });
        Ok(())
    }
    fn room_tile_at(&self, x: i32, y: i32) -> PyResult<(u16, u16)> {
        let mappy = self.mappy.borrow();
        let db = mappy.tiles.read().unwrap();
        let tc = db
            .get_change_by_id(mappy.current_room.as_ref().unwrap().get(x, y).unwrap())
            .unwrap();
        Ok((tc.from.index(), tc.to.index()))
    }
    // TODO queries for tracks, ...
}
#[pyclass]
pub struct Sprite {
    sprite: mappy::sprites::SpriteData,
}

#[pymethods]
impl Sprite {
    #[getter]
    fn index(&self) -> PyResult<u8> {
        Ok(self.sprite.index)
    }
    #[getter]
    fn x(&self) -> PyResult<u8> {
        Ok(self.sprite.x)
    }
    #[getter]
    fn y(&self) -> PyResult<u8> {
        Ok(self.sprite.y)
    }
    #[getter]
    fn width(&self) -> PyResult<u8> {
        Ok(self.sprite.width())
    }
    #[getter]
    fn height(&self) -> PyResult<u8> {
        Ok(self.sprite.height())
    }
    #[getter]
    fn vflip(&self) -> PyResult<bool> {
        Ok(self.sprite.vflip())
    }
    #[getter]
    fn hflip(&self) -> PyResult<bool> {
        Ok(self.sprite.hflip())
    }
    #[getter]
    fn bg(&self) -> PyResult<bool> {
        Ok(self.sprite.bg())
    }
    #[getter]
    fn pal(&self) -> PyResult<u8> {
        Ok(self.sprite.pal())
    }
    #[getter]
    fn key(&self) -> PyResult<u32> {
        Ok(self.sprite.key())
    }
}
