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

    /// Render composite with optional historical screenshots
    /// Layout with history:
    /// +----------------+--------+
    /// |                | HIST 1 |
    /// |   DESKTOP      +--------+
    /// |   (current)    | HIST 2 |
    /// |                +--------+
    /// |                | HIST 3 |
    /// +--------+-------+--------+
    /// | CHAT   | MEMORY| STATUS |
    /// +--------+-------+--------+
    pub fn render(&self, parts: &CompositeParts) -> RgbaImage {
        self.render_with_history(parts, &[])
    }
    
    pub fn render_with_history(&self, parts: &CompositeParts, history: &[&RgbaImage]) -> RgbaImage {
        let mut canvas = ImageBuffer::from_pixel(self.width, self.height, Rgba([10, 10, 12, 255]));
        
        // Calculate layout based on whether we have history
        let has_history = !history.is_empty();
        
        if has_history {
            // Layout with history panel on the right
            let history_width = self.width / 4;  // 25% for history
            let main_width = self.width - history_width;  // 75% for main content
            let top_height = (self.height * 2) / 3;  // Desktop takes 2/3 height
            let bottom_height = self.height - top_height;
            let bottom_panel_width = main_width / 3;
            
            // Desktop (large, top-left)
            overlay(
                &mut canvas,
                0,
                0,
                &resize_image(&parts.desktop, main_width, top_height),
            );
            draw_label(&mut canvas, 12, 18, "DESKTOP");
            
            // History filmstrip (right column)
            let hist_panel_height = top_height / 3;
            for (i, hist_img) in history.iter().take(3).enumerate() {
                let y = (i as u32) * hist_panel_height;
                overlay(
                    &mut canvas,
                    main_width,
                    y,
                    &resize_image(hist_img, history_width, hist_panel_height),
                );
                // Label each history panel
                let label = match i {
                    0 => "PREV 1",
                    1 => "PREV 2", 
                    2 => "PREV 3",
                    _ => "HIST",
                };
                draw_label(&mut canvas, main_width + 8, y + 14, label);
            }
            
            // Fill remaining history slots with placeholder if needed
            for i in history.len()..3 {
                let y = (i as u32) * hist_panel_height;
                draw_label(&mut canvas, main_width + 8, y + 14, "NO HIST");
            }
            
            // Bottom row: Chat, Memory, Status
            overlay(
                &mut canvas,
                0,
                top_height,
                &resize_image(&parts.chat_transcript, bottom_panel_width, bottom_height),
            );
            draw_label(&mut canvas, 12, top_height + 14, "RECENT CHAT");
            
            overlay(
                &mut canvas,
                bottom_panel_width,
                top_height,
                &resize_image(&parts.memory_visualization, bottom_panel_width, bottom_height),
            );
            draw_label(&mut canvas, bottom_panel_width + 8, top_height + 14, "MEMORY");
            
            overlay(
                &mut canvas,
                bottom_panel_width * 2,
                top_height,
                &resize_image(&parts.character_status, bottom_panel_width + history_width, bottom_height),
            );
            draw_label(&mut canvas, bottom_panel_width * 2 + 8, top_height + 14, "STATUS");
        } else {
            // Original 2x2 layout when no history
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
        }

        canvas
    }
}

impl Default for CompositeRenderer {
    fn default() -> Self {
        // Use wider aspect ratio to better fit typical 16:9/16:10 screens
        // This reduces letterboxing waste and keeps text readable
        Self {
            width: 2048,
            height: 1280,
        }
    }
}

pub struct CompositeParts {
    pub desktop: RgbaImage,
    pub memory_visualization: RgbaImage,
    pub chat_transcript: RgbaImage,
    pub character_status: RgbaImage,
}

/// Resize image to fit within bounds while preserving aspect ratio (letterboxing)
fn resize_image(image: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    resize_with_letterbox(image, width, height, Rgba([10, 10, 12, 255]))
}

/// Resize image to fit within bounds, preserving aspect ratio with letterboxing
fn resize_with_letterbox(image: &RgbaImage, target_w: u32, target_h: u32, bg_color: Rgba<u8>) -> RgbaImage {
    let src_w = image.width() as f32;
    let src_h = image.height() as f32;
    let target_w_f = target_w as f32;
    let target_h_f = target_h as f32;
    
    // Calculate scale to fit within bounds
    let scale_w = target_w_f / src_w;
    let scale_h = target_h_f / src_h;
    let scale = scale_w.min(scale_h);
    
    // Calculate new dimensions
    let new_w = (src_w * scale).round() as u32;
    let new_h = (src_h * scale).round() as u32;
    
    // Resize the image
    let resized = resize(image, new_w, new_h, FilterType::CatmullRom);
    
    // Create canvas with background color
    let mut canvas = ImageBuffer::from_pixel(target_w, target_h, bg_color);
    
    // Calculate offset to center the image
    let offset_x = (target_w.saturating_sub(new_w)) / 2;
    let offset_y = (target_h.saturating_sub(new_h)) / 2;
    
    // Overlay resized image onto canvas
    overlay(&mut canvas, offset_x, offset_y, &resized);
    
    canvas
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
