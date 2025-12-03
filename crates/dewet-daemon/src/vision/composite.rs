use image::{
    ImageBuffer, Rgba, RgbaImage,
    imageops::{FilterType, resize},
};

pub struct CompositeRenderer {
    width: u32,
    height: u32,
}

impl CompositeRenderer {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn render(&self, parts: &CompositeParts) -> RgbaImage {
        let mut canvas = ImageBuffer::from_pixel(self.width, self.height, Rgba([10, 10, 12, 255]));
        let half_w = self.width / 2;
        let half_h = self.height / 2;

        overlay(
            &mut canvas,
            0,
            0,
            &resize_image(&parts.desktop, half_w, half_h),
        );
        overlay(
            &mut canvas,
            half_w,
            0,
            &resize_image(&parts.memory_visualization, half_w, half_h),
        );
        overlay(
            &mut canvas,
            0,
            half_h,
            &resize_image(&parts.chat_transcript, half_w, half_h),
        );
        overlay(
            &mut canvas,
            half_w,
            half_h,
            &resize_image(&parts.character_status, half_w, half_h),
        );

        draw_label(&mut canvas, 12, 18, "DESKTOP");
        draw_label(&mut canvas, half_w + 12, 18, "MEMORY MAP");
        draw_label(&mut canvas, 12, half_h + 18, "RECENT CHAT");
        draw_label(&mut canvas, half_w + 12, half_h + 18, "COMPANIONS");

        canvas
    }
}

impl Default for CompositeRenderer {
    fn default() -> Self {
        Self {
            width: 1536,
            height: 1536,
        }
    }
}

pub struct CompositeParts {
    pub desktop: RgbaImage,
    pub memory_visualization: RgbaImage,
    pub chat_transcript: RgbaImage,
    pub character_status: RgbaImage,
}

fn resize_image(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    resize(image, width, height, FilterType::CatmullRom)
}

fn overlay(canvas: &mut RgbaImage, x: u32, y: u32, src: &RgbaImage) {
    for (dx, dy, pixel) in src.enumerate_pixels() {
        let tx = x + dx;
        let ty = y + dy;
        if tx < canvas.width() && ty < canvas.height() {
            canvas.put_pixel(tx, ty, *pixel);
        }
    }
}

fn draw_label(canvas: &mut RgbaImage, x: u32, y: u32, text: &str) {
    let mut cursor = x;
    for ch in text.chars() {
        draw_char(canvas, cursor, y, ch);
        cursor += 6;
    }
}

fn draw_char(canvas: &mut RgbaImage, x: u32, y: u32, ch: char) {
    if let Some(pattern) = glyph_pattern(ch) {
        for (row, bits) in pattern.iter().enumerate() {
            for col in 0..5 {
                if (bits >> (4 - col)) & 1 == 1 {
                    let px = x + col as u32;
                    let py = y + row as u32;
                    if px < canvas.width() && py < canvas.height() {
                        canvas.put_pixel(px, py, Rgba([255, 255, 255, 255]));
                    }
                }
            }
        }
    }
}

fn glyph_pattern(ch: char) -> Option<&'static [u8; 7]> {
    match ch {
        'A' => Some(&[
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ]),
        'B' => Some(&[
            0b11110, 0b10001, 0b11110, 0b10001, 0b10001, 0b10001, 0b11110,
        ]),
        'C' => Some(&[
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ]),
        'D' => Some(&[
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ]),
        'E' => Some(&[
            0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000, 0b11111,
        ]),
        'F' => Some(&[
            0b11111, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000, 0b10000,
        ]),
        'H' => Some(&[
            0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001, 0b10001,
        ]),
        'I' => Some(&[
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ]),
        'K' => Some(&[
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ]),
        'L' => Some(&[
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ]),
        'M' => Some(&[
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ]),
        'N' => Some(&[
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ]),
        'O' => Some(&[
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ]),
        'P' => Some(&[
            0b11110, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000, 0b10000,
        ]),
        'R' => Some(&[
            0b11110, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001, 0b10001,
        ]),
        'S' => Some(&[
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ]),
        'T' => Some(&[
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ]),
        'Y' => Some(&[
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ]),
        ' ' => Some(&[0, 0, 0, 0, 0, 0, 0]),
        _ => None,
    }
}
