use qrcode;
use image::{self, ImageBuffer};
use anyhow;
use std::sync::Mutex;

use crate::mpv::CmdHandle;

pub static QR_OVERLAY: Mutex<QrOverlay> = Mutex::new(QrOverlay::UnInitialized);

pub struct mpv_overlay {
    id: u32,
    visible: bool,
    image: Option<ImageBuffer<image::Rgba<u8>, Vec<u8>>>,
}

impl mpv_overlay {
    pub fn set_image(&mut self, data: &str) -> Result<(), anyhow::Error> {
        let code = qrcode::QrCode::with_error_correction_level(data.as_bytes(), qrcode::EcLevel::M)?;
        let image = code.render::<image::Rgba<u8>>()
            .min_dimensions(256, 256)
            .build();
        self.image = Some(image);
        Ok(())
    }

    pub fn show_overlay(&mut self, handle: &mut CmdHandle) -> Result<(), anyhow::Error>{
        let image = self.image.clone().ok_or(anyhow::anyhow!("overlay has not had an image set"))?; // TODO 
        let (width, height) = image.dimensions();
        let data = image.as_raw();
        let p = data.as_ptr();
        let _ = handle.command([
            "overlay-add", 
            "0", //id
            "0", "0", //x, y
            format!("&{:p}", p).as_str(), "0", // pointer, offset
            "bgra", // format
            format!("{}", width).as_str(), format!("{}" ,height).as_str(), // width, height
            format!("{}", width * 4).as_str() // stride
        ]);
        self.visible = true;
        Ok(())
    }
    pub fn hide_overlay(&mut self, handle: &mut CmdHandle) -> Result<(), anyhow::Error> {
        let _ = handle.command([
            "overlay-remove", 
            "0", //id
        ]);
        self.visible = false;
        Ok(())
    }
    pub fn toggle_overlay(&mut self, handle: &mut CmdHandle) -> Result<(), anyhow::Error> {
        self.visible = !self.visible;
        if self.visible {
            self.show_overlay(handle)
        } else {
            self.hide_overlay(handle)
        }
    }
}

pub enum QrOverlay<'a> {
    Initialized{
        overlay: mpv_overlay,
        handle: CmdHandle<'a>,
    },
    UnInitialized,
}

impl<'a> QrOverlay<'a> {
    pub fn initialize(&mut self, data: &str, handle: CmdHandle<'a>) -> Result<(), anyhow::Error> {
        let code = qrcode::QrCode::with_error_correction_level(data.as_bytes(), qrcode::EcLevel::M)?;
        let image = code.render::<image::Rgba<u8>>()
            .min_dimensions(256, 256)
            .build();
        let overlay = mpv_overlay{
            id: 0,
            visible: false,
            image: Some(image),
        };
        *self = QrOverlay::Initialized { overlay, handle };
        Ok(())
    }

    pub fn toggle_overlay(&mut self) -> Result<(), anyhow::Error> {
        match self {
            QrOverlay::UnInitialized => Err(anyhow::anyhow!("overlay uninitialized")),
            QrOverlay::Initialized { overlay, handle } => overlay.toggle_overlay(handle),
        }
    }
}
