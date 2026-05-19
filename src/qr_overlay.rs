use qrcode;
use image::{self, ImageBuffer};
use anyhow;
use std::sync::Mutex;

use crate::mpv::CmdHandle;

pub static QR_OVERLAY: Mutex<QrOverlay> = Mutex::new(QrOverlay::UnInitialized);

pub enum QrOverlay<'a> {
    Initialized{
        id: u32,
        visible: bool,
        image: ImageBuffer<image::Rgba<u8>, Vec<u8>>,
        handle: CmdHandle<'a>,
    },
    UnInitialized,
}

static OVERLAY_ID: Mutex<u32> = Mutex::new(0);
impl<'a> QrOverlay<'a> {
    pub fn initialize(&mut self, data: &str, handle: CmdHandle<'a>) -> Result<(), anyhow::Error> {
        let code = qrcode::QrCode::with_error_correction_level(data.as_bytes(), qrcode::EcLevel::M)?;
        let image = code.render::<image::Rgba<u8>>()
            .min_dimensions(256, 256)
            .build();
        *self = QrOverlay::Initialized {
            id: *OVERLAY_ID.lock().unwrap(),
            visible: false,
            image,
            handle,
        };
        *OVERLAY_ID.lock().unwrap() += 1;
        Ok(())
    }

    pub fn show_overlay(&mut self) -> Result<(), anyhow::Error> {
        match self {
            QrOverlay::UnInitialized => Err(anyhow::anyhow!("overlay uninitialized")),
            QrOverlay::Initialized { id, visible, image, handle } => {
                let (width, height) = image.dimensions();
                let data = image.as_raw();
                let p = data.as_ptr();
                let _ = handle.command([
                    "overlay-add", 
                    format!("{}", id).as_str(), //id
                    "0", "0", //x, y
                    format!("&{:p}", p).as_str(), "0", // pointer, offset
                    "bgra", // format
                    format!("{}", width).as_str(), format!("{}" ,height).as_str(), // width, height
                    format!("{}", width * 4).as_str() // stride
                ]);
                *visible = true;
                Ok(())
            },
        }
    }

    pub fn hide_overlay(&mut self) -> Result<(), anyhow::Error> {
        match self {
            QrOverlay::UnInitialized => Err(anyhow::anyhow!("overlay uninitialized")),
            QrOverlay::Initialized { id, visible, image:_,  handle } => {
                let _ = handle.command([
                    "overlay-remove", 
                    format!("{}", id).as_str(), //id
                ]);
                *visible = false;
                Ok(())
            },
        }
    }

    pub fn toggle_overlay(&mut self) -> Result<(), anyhow::Error> {
        match self {
            QrOverlay::UnInitialized => Err(anyhow::anyhow!("overlay uninitialized")),
            QrOverlay::Initialized { visible, .. } => {
                if *visible {
                    self.hide_overlay()
                } else {
                    self.show_overlay()
                }
            },
        }
    }
}
