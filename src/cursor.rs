use std::collections::HashMap;
use std::io;

use wayrs_client::connection::Connection;
use wayrs_client::protocol::*;
use wayrs_shm_alloc::ShmAlloc;

use xcursor::parser::Image;

use crate::state::State;

#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("cursor '{0}' not found")]
    CursorNotFound(String),
    #[error("theme could not be parsed")]
    ThemeParseError,
    #[error(transparent)]
    ReadError(#[from] io::Error),
}

pub struct CursorTheme {
    xcursor_size: u32,
    theme: xcursor::CursorTheme,
    cursors: HashMap<String, Vec<Image>>,
}

impl CursorTheme {
    pub fn new() -> Self {
        let theme_name = std::env::var("XCURSOR_THEME").ok();
        let theme_name = theme_name.as_deref().unwrap_or("default");

        let xcursor_size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|x| x.parse().ok())
            .unwrap_or(24);

        let theme = xcursor::CursorTheme::load(theme_name);

        CursorTheme {
            xcursor_size,
            theme,
            cursors: HashMap::new(),
        }
    }

    /// Use this to handle errors ahead of time.
    pub fn ensure_cursor_is_loaded(&mut self, cursor: &str) -> Result<(), CursorError> {
        let _ = self.get_images(cursor)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn set_cursor(
        &mut self,
        conn: &mut Connection<State>,
        cursor: &str,
        scale: u32,
        serial: u32,
        shm: &mut ShmAlloc,
        surface: WlSurface,
        pointer: WlPointer,
    ) -> Result<(), CursorError> {
        let target_size = self.xcursor_size * scale;

        let images = self.get_images(cursor)?;

        let image = match images.binary_search_by_key(&target_size, |img| img.size) {
            Ok(indx) => &images[indx],
            Err(indx) if indx == 0 => images.first().unwrap(),
            Err(indx) if indx >= images.len() => images.last().unwrap(),
            Err(indx) => {
                let a = &images[indx - 1];
                let b = &images[indx];
                if target_size - a.size < b.size - target_size {
                    a
                } else {
                    b
                }
            }
        };

        let (buffer, canvas) = shm.alloc_buffer(
            conn,
            image.width as i32,
            image.height as i32,
            (image.width * 4) as i32,
            wl_shm::Format::Argb8888,
        );

        assert_eq!(image.pixels_argb.len(), canvas.len(),);
        canvas.copy_from_slice(&image.pixels_rgba);

        surface.attach(conn, buffer.wl, 0, 0);
        surface.damage(conn, 0, 0, i32::MAX, i32::MAX);
        surface.set_buffer_scale(conn, scale as i32);
        surface.commit(conn);

        pointer.set_cursor(
            conn,
            serial,
            surface,
            (image.xhot / scale) as i32,
            (image.yhot / scale) as i32,
        );

        Ok(())
    }

    fn get_images(&mut self, cursor: &'_ str) -> Result<&[Image], CursorError> {
        // Borrow checker does't allow `if let Some(...) = ...` here for some reason :(
        if self.cursors.get(cursor).is_some() {
            return Ok(self.cursors.get(cursor).unwrap());
        }

        let theme_path = self
            .theme
            .load_icon(cursor)
            .ok_or_else(|| CursorError::CursorNotFound(cursor.to_owned()))?;

        let raw_theme = std::fs::read(theme_path)?;

        let mut images =
            xcursor::parser::parse_xcursor(&raw_theme).ok_or(CursorError::ThemeParseError)?;
        images.sort_unstable_by_key(|img| img.size);

        Ok(self.cursors.entry(cursor.to_owned()).or_insert(images))
    }
}
