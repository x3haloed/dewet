use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use image::{DynamicImage, ImageBuffer, ImageFormat, Luma, Rgba, RgbaImage, imageops::FilterType};
use rand::{Rng, distributions::Uniform};
use serde::Serialize;
#[cfg(feature = "native-capture")]
use tracing::warn;

use crate::config::VisionConfig;

const THUMB_WIDTH: u32 = 64;
const THUMB_HEIGHT: u32 = 36;

pub struct VisionPipeline {
    config: VisionConfig,
    provider: Box<dyn ScreenProvider + Send>,
    last_thumb: Option<ImageBuffer<Luma<u8>, Vec<u8>>>,
}

impl VisionPipeline {
    pub fn new(config: VisionConfig) -> Self {
        #[allow(unused_mut)]
        let mut provider: Box<dyn ScreenProvider + Send> = Box::new(MockScreenProvider::default());

        #[cfg(feature = "native-capture")]
        {
            provider = match NativeScreenProvider::new() {
                Ok(native) => Box::new(native),
                Err(err) => {
                    warn!(?err, "Falling back to mock screen provider");
                    Box::new(MockScreenProvider::default())
                }
            };
        }

        Self {
            config,
            provider,
            last_thumb: None,
        }
    }

    pub fn capture_interval(&self) -> Duration {
        self.config.capture_interval()
    }

    pub fn capture_frame(&mut self) -> Result<VisionFrame> {
        let image = self.provider.capture_frame()?;
        let thumb = make_thumb(&image);

        let diff_score = self
            .last_thumb
            .as_ref()
            .map(|prev| difference_score(&thumb, prev))
            .unwrap_or(1.0);

        self.last_thumb = Some(thumb);

        Ok(VisionFrame {
            timestamp: Utc::now(),
            image,
            diff_score,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VisionFrame {
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing)]
    pub image: DynamicImage,
    pub diff_score: f32,
}

impl VisionFrame {
    pub fn as_png(&self) -> Result<Vec<u8>> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        self.image.write_to(&mut cursor, ImageFormat::Png)?;
        Ok(cursor.into_inner())
    }

    pub fn rgba(&self) -> RgbaImage {
        self.image.to_rgba8()
    }
}

trait ScreenProvider {
    fn capture_frame(&mut self) -> Result<DynamicImage>;
}

#[derive(Default)]
struct MockScreenProvider {
    tick: u64,
}

impl ScreenProvider for MockScreenProvider {
    fn capture_frame(&mut self) -> Result<DynamicImage> {
        self.tick += 1;
        let mut img = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(1536, 1536);
        let mut rng = rand::thread_rng();
        let dist = Uniform::new_inclusive(0u8, 15);

        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let base = ((x + y) % 255) as u8;
            let noise = rng.sample(dist);
            *pixel = Rgba([base ^ noise, (base / 2) ^ noise, (base / 3) ^ noise, 255]);
        }

        let overlay_color = Rgba([
            ((self.tick * 13) % 255) as u8,
            120,
            ((self.tick * 7) % 255) as u8,
            255,
        ]);
        for x in 100..400 {
            for y in 100..300 {
                if x < img.width() && y < img.height() {
                    img.put_pixel(x, y, overlay_color);
                }
            }
        }

        Ok(DynamicImage::ImageRgba8(img))
    }
}

#[cfg(feature = "native-capture")]
struct NativeScreenProvider {
    monitor: xcap::Monitor,
}

#[cfg(feature = "native-capture")]
impl NativeScreenProvider {
    fn new() -> Result<Self> {
        let monitors = xcap::Monitor::all()
            .map_err(|e| anyhow::anyhow!("Failed to enumerate monitors: {}", e))?;
        let monitor = monitors
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No monitors found"))?;
        Ok(Self { monitor })
    }
}

#[cfg(feature = "native-capture")]
impl ScreenProvider for NativeScreenProvider {
    fn capture_frame(&mut self) -> Result<DynamicImage> {
        let raw = self.monitor.capture_image()?;
        let width = raw.width();
        let height = raw.height();
        let bytes = raw.to_vec();
        let img = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_vec(width as u32, height as u32, bytes)
            .ok_or_else(|| anyhow::anyhow!("failed to convert capture buffer"))?;
        Ok(DynamicImage::ImageRgba8(img))
    }
}

fn make_thumb(image: &DynamicImage) -> ImageBuffer<Luma<u8>, Vec<u8>> {
    image
        .resize(THUMB_WIDTH, THUMB_HEIGHT, FilterType::Lanczos3)
        .to_luma8()
}

fn difference_score(
    current: &ImageBuffer<Luma<u8>, Vec<u8>>,
    previous: &ImageBuffer<Luma<u8>, Vec<u8>>,
) -> f32 {
    let total_pixels = (THUMB_WIDTH * THUMB_HEIGHT) as f32;
    let mut delta = 0f32;
    for (cur, prev) in current.pixels().zip(previous.pixels()) {
        let cur_val = cur[0] as f32;
        let prev_val = prev[0] as f32;
        delta += (cur_val - prev_val).abs();
    }
    delta / (total_pixels * 255.0)
}
