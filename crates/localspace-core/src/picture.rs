//! A picture a person attaches, made ready for the model (docs/DECISIONS.md,
//! 2026-10-18; plan 38, P2). PNG, JPEG and WebP are read; anything larger
//! than the limits is refused before it is decoded; a phone photo is turned
//! the way its camera says; the longer side is brought down to 1,280
//! pixels; and the result is a JPEG when a JPEG came in (a photo) and a PNG
//! otherwise (a screenshot, or a WebP, which the engine cannot read). Only
//! this shrunk copy is kept: it is what the model sees.

use std::io::Cursor;

use image::imageops::FilterType;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

/// The largest file taken, in bytes: a 48 to 50 megapixel phone photo fits.
pub const MAX_BYTES: usize = 30 * 1024 * 1024;
/// The most pixels taken (60 megapixels), checked from the header before
/// any decoding, so that a small file that claims a huge picture is refused.
pub const MAX_PIXELS: u64 = 60_000_000;
/// The longer side of what the model is sent, in pixels.
pub const LONGEST_SIDE: u32 = 1280;
/// The quality a photo is saved at once shrunk.
const JPEG_QUALITY: u8 = 85;

/// The limits a picture is held to: the constants above, or smaller ones in
/// a test.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_pixels: u64,
    pub longest_side: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_bytes: MAX_BYTES,
            max_pixels: MAX_PIXELS,
            longest_side: LONGEST_SIDE,
        }
    }
}

/// A picture ready for the model, and for keeping with the chat.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub bytes: Vec<u8>,
    /// `image/png` or `image/jpeg`: what the engine is told it is.
    pub mime: &'static str,
    pub width: u32,
    pub height: u32,
}

/// Why a file was not taken as a picture.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("the file is {megabytes} MB; pictures up to 30 MB are taken")]
    TooLarge { megabytes: u64 },
    #[error("the picture is {width} by {height} pixels; pictures up to 60 megapixels are taken")]
    TooManyPixels { width: u32, height: u32 },
    #[error("the file is not a PNG, JPEG or WebP picture")]
    NotAPicture,
    #[error("the picture could not be read: {0}")]
    Unreadable(String),
}

/// `bytes` made ready for the model, held to the default limits.
pub fn prepare(bytes: &[u8]) -> Result<Prepared, Refused> {
    prepare_with(bytes, Limits::default())
}

/// `bytes` made ready for the model, held to `limits`.
pub fn prepare_with(bytes: &[u8], limits: Limits) -> Result<Prepared, Refused> {
    if bytes.len() > limits.max_bytes {
        return Err(Refused::TooLarge {
            megabytes: (bytes.len() as u64).div_ceil(1024 * 1024),
        });
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| Refused::Unreadable(e.to_string()))?;
    let format = match reader.format() {
        Some(f @ (ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)) => f,
        _ => return Err(Refused::NotAPicture),
    };
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| Refused::Unreadable(e.to_string()))?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > limits.max_pixels {
        return Err(Refused::TooManyPixels { width, height });
    }
    // A phone photo is often stored on its side, with a note of which way up.
    let orientation = decoder
        .orientation()
        .map_err(|e| Refused::Unreadable(e.to_string()))?;
    let mut picture =
        DynamicImage::from_decoder(decoder).map_err(|e| Refused::Unreadable(e.to_string()))?;
    picture.apply_orientation(orientation);
    if picture.width().max(picture.height()) > limits.longest_side {
        picture = picture.resize(
            limits.longest_side,
            limits.longest_side,
            FilterType::Lanczos3,
        );
    }
    let (width, height) = (picture.width(), picture.height());
    let mut out = Vec::new();
    let mime = if format == ImageFormat::Jpeg {
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
        picture
            .to_rgb8()
            .write_with_encoder(encoder)
            .map_err(|e| Refused::Unreadable(e.to_string()))?;
        "image/jpeg"
    } else {
        picture
            .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
            .map_err(|e| Refused::Unreadable(e.to_string()))?;
        "image/png"
    };
    Ok(Prepared {
        bytes: out,
        mime,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn encoded(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let picture = RgbImage::from_fn(width, height, |x, y| {
            Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
        });
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(picture)
            .write_to(&mut Cursor::new(&mut out), format)
            .expect("encoding a test picture");
        out
    }

    #[test]
    fn a_small_screenshot_stays_a_png_of_its_own_size() {
        let p = prepare(&encoded(718, 502, ImageFormat::Png)).expect("taken");
        assert_eq!((p.width, p.height, p.mime), (718, 502, "image/png"));
        assert!(p.bytes.starts_with(b"\x89PNG"));
    }

    #[test]
    fn a_large_photo_is_shrunk_to_1280_and_stays_a_jpeg() {
        let p = prepare(&encoded(3000, 2000, ImageFormat::Jpeg)).expect("taken");
        assert_eq!((p.width, p.height, p.mime), (1280, 853, "image/jpeg"));
        assert!(p.bytes.starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn a_tall_picture_is_shrunk_by_its_longer_side() {
        let p = prepare(&encoded(1000, 2560, ImageFormat::Png)).expect("taken");
        assert_eq!((p.width, p.height), (500, 1280));
    }

    #[test]
    fn a_webp_becomes_a_png_since_the_engine_cannot_read_webp() {
        let p = prepare(&encoded(64, 48, ImageFormat::WebP)).expect("taken");
        assert_eq!((p.width, p.height, p.mime), (64, 48, "image/png"));
    }

    #[test]
    fn a_file_over_the_size_limit_is_refused_before_it_is_read() {
        let big = vec![0u8; MAX_BYTES + 1];
        assert_eq!(
            prepare(&big).unwrap_err(),
            Refused::TooLarge { megabytes: 31 }
        );
    }

    #[test]
    fn a_picture_over_the_pixel_limit_is_refused_from_its_header() {
        let limits = Limits {
            max_pixels: 1000,
            ..Limits::default()
        };
        assert_eq!(
            prepare_with(&encoded(40, 40, ImageFormat::Png), limits).unwrap_err(),
            Refused::TooManyPixels {
                width: 40,
                height: 40
            }
        );
    }

    #[test]
    fn other_files_are_not_taken_as_pictures() {
        assert_eq!(prepare(b"GIF89a....").unwrap_err(), Refused::NotAPicture);
        assert_eq!(
            prepare(b"just some words").unwrap_err(),
            Refused::NotAPicture
        );
        assert!(matches!(
            prepare(b"\x89PNG\r\n\x1a\nnot really").unwrap_err(),
            Refused::Unreadable(_)
        ));
    }

    #[test]
    fn the_refusals_say_the_limits_in_words() {
        assert_eq!(
            Refused::TooManyPixels {
                width: 9000,
                height: 8000
            }
            .to_string(),
            "the picture is 9000 by 8000 pixels; pictures up to 60 megapixels are taken"
        );
    }
}
