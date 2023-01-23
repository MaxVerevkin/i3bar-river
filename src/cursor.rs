use std::io;

use wayrs_client::connection::Connection;
use wayrs_client::protocol::wl_compositor::WlCompositor;
use wayrs_client::protocol::wl_pointer::WlPointer;
use wayrs_client::protocol::wl_shm::Format;
use wayrs_client::protocol::wl_surface::WlSurface;

use wayrs_shm_alloc::ShmAlloc;

use xcursor::parser::Image;

use crate::state::State;

#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("theme not found")]
    ThemeNotFound,
    #[error("theme could not be parsed")]
    ThemeParseError,
    #[error(transparent)]
    ReadError(#[from] io::Error),
}

pub struct Cursor {
    pub surface: WlSurface,
    pub images: Vec<Image>,
}

impl Cursor {
    pub fn new(
        conn: &mut Connection<State>,
        compositor: WlCompositor,
    ) -> Result<Self, CursorError> {
        let theme_name = std::env::var("XCURSOR_THEME").ok();
        let theme_name = theme_name.as_deref().unwrap_or("default");

        let theme_path = xcursor::CursorTheme::load(theme_name)
            .load_icon("default")
            .ok_or(CursorError::ThemeNotFound)?;
        let raw_theme = std::fs::read(theme_path)?;

        let mut images =
            xcursor::parser::parse_xcursor(&raw_theme).ok_or(CursorError::ThemeParseError)?;
        images.sort_unstable_by_key(|img| img.size);

        let surface = compositor.create_surface(conn);

        Ok(Cursor { surface, images })
    }

    pub fn set(
        &self,
        conn: &mut Connection<State>,
        serial: u32,
        pointer: WlPointer,
        scale: u32,
        shm: &mut ShmAlloc,
    ) {
        let target_size = std::env::var("XCURSOR_SIZE")
            .ok()
            .and_then(|x| x.parse().ok())
            .unwrap_or(24)
            * scale;

        let image = match self
            .images
            .binary_search_by_key(&target_size, |img| img.size)
        {
            Ok(indx) => &self.images[indx],
            Err(indx) if indx == 0 => self.images.first().unwrap(),
            Err(indx) if indx >= self.images.len() => self.images.last().unwrap(),
            Err(indx) => {
                let a = &self.images[indx - 1];
                let b = &self.images[indx];
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
            Format::Argb8888,
        );

        assert_eq!(image.pixels_argb.len(), canvas.len(),);
        canvas.copy_from_slice(&image.pixels_rgba);

        self.surface.attach(conn, buffer.wl, 0, 0);
        self.surface
            .damage_buffer(conn, 0, 0, image.width as i32, image.height as i32);
        self.surface.set_buffer_scale(conn, scale as i32);
        self.surface.commit(conn);

        pointer.set_cursor(
            conn,
            serial,
            self.surface,
            (image.xhot / scale) as i32,
            (image.yhot / scale) as i32,
        );
    }
}
