use ab_glyph::{Font, FontArc, PxScale, ScaleFont, point};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use image::{Rgba, RgbaImage};
use std::path::PathBuf;

use crate::config::Config;

pub struct FontManager {
    pub custom_font: Option<FontArc>,
}

pub fn resolve_font(spec: &str) -> Option<PathBuf> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return None;
    }

    // 1. Direct path or ~ expansion
    let path = if let Some(stripped) = trimmed.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(stripped)
        } else {
            PathBuf::from(trimmed)
        }
    } else {
        PathBuf::from(trimmed)
    };

    if path.is_file() {
        return Some(path);
    }

    // 2. Query system fontconfig via fc-match (standard on Linux)
    if let Ok(output) = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", trimmed])
        .output()
        && output.status.success()
    {
        let matched_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !matched_path.is_empty() {
            let p = PathBuf::from(matched_path);
            if p.is_file() {
                return Some(p);
            }
        }
    }

    // 3. Fallback scan in common font directories
    let mut search_dirs = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        let h = PathBuf::from(home);
        search_dirs.push(h.join(".local/share/fonts"));
        search_dirs.push(h.join(".fonts"));
    }
    search_dirs.push(PathBuf::from("/usr/share/fonts"));
    search_dirs.push(PathBuf::from("/usr/local/share/fonts"));

    let lower_spec = trimmed.to_ascii_lowercase().replace(' ', "");
    for dir in search_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file()
                    && let Some(ext) = p.extension().and_then(|e| e.to_str())
                {
                    let lower_ext = ext.to_ascii_lowercase();
                    if (lower_ext == "ttf" || lower_ext == "otf")
                        && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
                    {
                        let clean = stem.to_ascii_lowercase().replace(['-', '_', ' '], "");
                        if clean.contains(&lower_spec) {
                            return Some(p);
                        }
                    }
                }
            }
        }
    }

    None
}

impl FontManager {
    pub fn new(font_spec: Option<&str>) -> Self {
        let custom_font = font_spec.and_then(|spec| {
            let trimmed = spec.trim();
            if trimmed.is_empty() {
                return None;
            }

            match resolve_font(trimmed) {
                Some(resolved_path) => match std::fs::read(&resolved_path) {
                    Ok(bytes) => match FontArc::try_from_vec(bytes) {
                        Ok(font) => Some(font),
                        Err(e) => {
                            eprintln!(
                                "swall: warning: failed to parse font at {}: {}",
                                resolved_path.display(),
                                e
                            );
                            None
                        }
                    },
                    Err(e) => {
                        eprintln!(
                            "swall: warning: could not read font at {}: {}",
                            resolved_path.display(),
                            e
                        );
                        None
                    }
                },
                None => {
                    eprintln!(
                        "swall: warning: font '{}' could not be resolved, falling back to built-in bitmap font",
                        trimmed
                    );
                    None
                }
            }
        });

        Self { custom_font }
    }
}

pub fn render_text(
    img: &mut RgbaImage,
    font_mgr: &FontManager,
    text: &str,
    mut x: f32,
    y: f32,
    font_size: f32,
    color: [u8; 4],
) -> f32 {
    if let Some(ref font) = font_mgr.custom_font {
        let scale = PxScale::from(font_size);
        let scaled_font = font.as_scaled(scale);
        let mut last_glyph_id = None;

        for ch in text.chars() {
            if ch.is_control() {
                continue;
            }
            let glyph_id = scaled_font.glyph_id(ch);
            if let Some(last) = last_glyph_id {
                x += scaled_font.kern(last, glyph_id);
            }
            last_glyph_id = Some(glyph_id);

            let mut glyph = scaled_font.scaled_glyph(ch);
            glyph.position = point(x, y);

            if let Some(outlined) = font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, coverage| {
                    let px = (bounds.min.x as i32 + gx as i32) as u32;
                    let py = (bounds.min.y as i32 + gy as i32) as u32;
                    if px < img.width() && py < img.height() {
                        let old = img.get_pixel(px, py).0;
                        let a_src = (color[3] as f32 / 255.0) * coverage;
                        let a_dst = old[3] as f32 / 255.0;
                        let a_out = a_src + a_dst * (1.0 - a_src);
                        if a_out > 0.0 {
                            let blend = |c_s: u8, c_d: u8| -> u8 {
                                let s = c_s as f32 / 255.0;
                                let d = c_d as f32 / 255.0;
                                let res = (s * a_src + d * a_dst * (1.0 - a_src)) / a_out;
                                (res * 255.0).clamp(0.0, 255.0) as u8
                            };
                            let new_color = [
                                blend(color[0], old[0]),
                                blend(color[1], old[1]),
                                blend(color[2], old[2]),
                                (a_out * 255.0) as u8,
                            ];
                            img.put_pixel(px, py, Rgba(new_color));
                        }
                    }
                });
            }
            x += scaled_font.h_advance(glyph_id);
        }
        x
    } else {
        // Fallback to bitmap font
        let scale = (font_size / 8.0).round().max(1.0) as u32;
        let mut cur_x = x as i32;
        let start_y = y as i32;

        for ch in text.chars() {
            if ch == '\n' {
                continue;
            }
            if let Some(glyph) = BASIC_FONTS.get(ch) {
                for (gy, byte) in glyph.iter().enumerate() {
                    for gx in 0..8 {
                        if (byte & (1 << gx)) != 0 {
                            for sy in 0..scale {
                                for sx in 0..scale {
                                    let px = cur_x + gx * (scale as i32) + (sx as i32);
                                    let py = start_y + (gy as i32) * (scale as i32) + (sy as i32);
                                    if px >= 0
                                        && px < img.width() as i32
                                        && py >= 0
                                        && py < img.height() as i32
                                    {
                                        let old = img.get_pixel(px as u32, py as u32).0;
                                        let a_src = color[3] as f32 / 255.0;
                                        let a_dst = old[3] as f32 / 255.0;
                                        let a_out = a_src + a_dst * (1.0 - a_src);
                                        if a_out > 0.0 {
                                            let blend = |c_s: u8, c_d: u8| -> u8 {
                                                let s = c_s as f32 / 255.0;
                                                let d = c_d as f32 / 255.0;
                                                let res =
                                                    (s * a_src + d * a_dst * (1.0 - a_src)) / a_out;
                                                (res * 255.0).clamp(0.0, 255.0) as u8
                                            };
                                            let new_color = [
                                                blend(color[0], old[0]),
                                                blend(color[1], old[1]),
                                                blend(color[2], old[2]),
                                                (a_out * 255.0) as u8,
                                            ];
                                            img.put_pixel(px as u32, py as u32, Rgba(new_color));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            cur_x += (8 * scale) as i32;
        }
        cur_x as f32
    }
}

pub fn get_text_dimensions(font_mgr: &FontManager, text: &str, font_size: f32) -> (f32, f32) {
    if let Some(ref font) = font_mgr.custom_font {
        let scale = PxScale::from(font_size);
        let scaled_font = font.as_scaled(scale);
        let mut width = 0.0;
        let mut last_glyph_id = None;

        for ch in text.chars() {
            if ch.is_control() {
                continue;
            }
            let glyph_id = scaled_font.glyph_id(ch);
            if let Some(last) = last_glyph_id {
                width += scaled_font.kern(last, glyph_id);
            }
            last_glyph_id = Some(glyph_id);
            width += scaled_font.h_advance(glyph_id);
        }
        let height = scaled_font.height();
        (width, height)
    } else {
        let scale = (font_size / 8.0).round().max(1.0) as u32;
        let width = (text.chars().count() as u32 * 8 * scale) as f32;
        let height = (8 * scale) as f32;
        (width, height)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_slanted_pill(
    img: &mut RgbaImage,
    center_x: i32,
    center_y: i32,
    width: i32,
    height: i32,
    bg_color: [u8; 4],
    border_color: [u8; 4],
    border_width: f32,
) {
    let cot75: f32 = 0.267_949_2;
    let sin75: f32 = 0.965_925_8;
    let delta_x = (height as f32) * cot75;

    let half_w = width as f32 * 0.5;
    let half_h = height as f32 * 0.5;

    let min_x = (center_x as f32 - half_w - delta_x * 0.5 - 2.0).floor() as i32;
    let max_x = (center_x as f32 + half_w + delta_x * 0.5 + 2.0).ceil() as i32;
    let min_y = (center_y as f32 - half_h - 2.0).floor() as i32;
    let max_y = (center_y as f32 + half_h + 2.0).ceil() as i32;

    for y in min_y..=max_y {
        if y < 0 || y >= img.height() as i32 {
            continue;
        }
        let py = y as f32 - center_y as f32;
        let dist_y = half_h - py.abs();
        if dist_y < -1.0 {
            continue;
        }

        for x in min_x..=max_x {
            if x < 0 || x >= img.width() as i32 {
                continue;
            }
            let px = x as f32 - center_x as f32;
            let u_slant = px + py * cot75;
            let dist_slant = (half_w - u_slant.abs()) * sin75;

            let dist = dist_y.min(dist_slant);
            if dist < -0.5 {
                continue;
            }

            let edge_factor = (dist + 0.5).clamp(0.0, 1.0);
            let border_factor = if border_width > 0.0 {
                (1.0 - (dist - (border_width - 0.5)).clamp(0.0, 1.0)).max(0.0)
            } else {
                0.0
            };

            let bf = border_factor * (border_color[3] as f32 / 255.0);
            let r = (bg_color[0] as f32 * (1.0 - bf) + border_color[0] as f32 * bf) as u8;
            let g = (bg_color[1] as f32 * (1.0 - bf) + border_color[1] as f32 * bf) as u8;
            let b = (bg_color[2] as f32 * (1.0 - bf) + border_color[2] as f32 * bf) as u8;
            let a = ((bg_color[3] as f32 * (1.0 - bf) + border_color[3] as f32 * bf) * edge_factor)
                as u8;

            img.put_pixel(x as u32, y as u32, Rgba([r, g, b, a]));
        }
    }
}

pub fn render_ui_overlay(
    screen_w: u32,
    screen_h: u32,
    current_index: usize,
    total_count: usize,
    file_name: &str,
    config: &Config,
    font_mgr: &FontManager,
) -> RgbaImage {
    let mut img = RgbaImage::new(screen_w, screen_h);

    if screen_w < 100 || screen_h < 100 {
        return img;
    }

    let pill_bg = config.colors.panel_bg_color.to_rgba8();
    let pill_border = config.colors.panel_border_color.to_rgba8();
    let text_white = config.colors.text_color.to_rgba8();
    let text_accent = config.colors.accent_color.to_rgba8();
    let text_dim = config.colors.dim_text_color.to_rgba8();

    let font_size = config.ui.font_size;

    // 1. Top Header: [ Index / Total  •  Filename ]
    if config.ui.show_top_bar {
        let index_str = format!("[ {} / {} ]", current_index + 1, total_count);
        let title_str = format!("{}   {}", index_str, file_name);
        let (text_w, text_h) = get_text_dimensions(font_mgr, &title_str, font_size);

        let top_y = 52i32;
        let pill_w = (text_w as i32 + 64).max(320);
        let pill_h = (text_h as i32 + 24).max(46);
        let top_center_x = (screen_w / 2) as i32;

        draw_slanted_pill(
            &mut img,
            top_center_x,
            top_y,
            pill_w,
            pill_h,
            pill_bg,
            pill_border,
            2.0,
        );

        let start_x = top_center_x as f32 - (text_w * 0.5);
        let start_y = if font_mgr.custom_font.is_some() {
            // ab_glyph baseline is relative to top
            top_y as f32 - (text_h * 0.25)
        } else {
            top_y as f32 - (text_h * 0.5)
        };

        let next_x = render_text(
            &mut img,
            font_mgr,
            &index_str,
            start_x,
            start_y,
            font_size,
            text_accent,
        );
        render_text(
            &mut img,
            font_mgr,
            &format!("   {}", file_name),
            next_x,
            start_y,
            font_size,
            text_white,
        );
    }

    // 2. Bottom Footer: Navigation hints
    if config.ui.show_bottom_bar {
        let footer_text = "h / <- : Prev     l / -> : Next     Enter : Select     Esc : Exit";
        let (footer_w, footer_h) = get_text_dimensions(font_mgr, footer_text, font_size * 0.9);
        let bot_y = (screen_h as i32 - 52).max(120);
        let bot_pill_w = footer_w as i32 + 64;
        let bot_pill_h = (footer_h as i32 + 20).max(42);
        let bot_center_x = (screen_w / 2) as i32;

        draw_slanted_pill(
            &mut img,
            bot_center_x,
            bot_y,
            bot_pill_w,
            bot_pill_h,
            pill_bg,
            [60, 75, 95, 180],
            1.5,
        );

        let bot_start_x = bot_center_x as f32 - (footer_w * 0.5);
        let bot_start_y = if font_mgr.custom_font.is_some() {
            bot_y as f32 - (footer_h * 0.25)
        } else {
            bot_y as f32 - (footer_h * 0.5)
        };

        render_text(
            &mut img,
            font_mgr,
            footer_text,
            bot_start_x,
            bot_start_y,
            font_size * 0.9,
            text_dim,
        );
    }

    img
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_overlay_creation() {
        let config = Config::default();
        let font_mgr = FontManager::new(None);
        let overlay = render_ui_overlay(1920, 1080, 0, 5, "mountain.png", &config, &font_mgr);
        assert_eq!(overlay.width(), 1920);
        assert_eq!(overlay.height(), 1080);
    }

    #[test]
    fn test_ui_overlay_hidden_panels() {
        let mut config = Config::default();
        config.ui.show_top_bar = false;
        config.ui.show_bottom_bar = false;
        let font_mgr = FontManager::new(None);
        let overlay = render_ui_overlay(1920, 1080, 0, 5, "mountain.png", &config, &font_mgr);
        assert_eq!(overlay.width(), 1920);
        assert_eq!(overlay.height(), 1080);
    }

    #[test]
    fn test_resolve_font() {
        assert_eq!(resolve_font(""), None);
        assert_eq!(resolve_font("   "), None);
        // On Linux with fontconfig, "sans-serif" or "monospace" should resolve
        if let Some(font_path) = resolve_font("sans-serif") {
            assert!(font_path.exists());
        }
    }
}
