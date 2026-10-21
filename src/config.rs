use serde::{Deserialize, Deserializer, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color(pub [u8; 4]);

impl Color {
    pub fn to_rgba8(self) -> [u8; 4] {
        self.0
    }

    pub fn to_f32_array(self) -> [f32; 4] {
        [
            self.0[0] as f32 / 255.0,
            self.0[1] as f32 / 255.0,
            self.0[2] as f32 / 255.0,
            self.0[3] as f32 / 255.0,
        ]
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ColorHelper {
            Hex(String),
            Array4([u8; 4]),
            Array3([u8; 3]),
        }

        match ColorHelper::deserialize(deserializer)? {
            ColorHelper::Hex(s) => {
                let clean = s.trim_start_matches('#');
                if clean.len() == 6 {
                    let r =
                        u8::from_str_radix(&clean[0..2], 16).map_err(serde::de::Error::custom)?;
                    let g =
                        u8::from_str_radix(&clean[2..4], 16).map_err(serde::de::Error::custom)?;
                    let b =
                        u8::from_str_radix(&clean[4..6], 16).map_err(serde::de::Error::custom)?;
                    Ok(Color([r, g, b, 255]))
                } else if clean.len() == 8 {
                    let r =
                        u8::from_str_radix(&clean[0..2], 16).map_err(serde::de::Error::custom)?;
                    let g =
                        u8::from_str_radix(&clean[2..4], 16).map_err(serde::de::Error::custom)?;
                    let b =
                        u8::from_str_radix(&clean[4..6], 16).map_err(serde::de::Error::custom)?;
                    let a =
                        u8::from_str_radix(&clean[6..8], 16).map_err(serde::de::Error::custom)?;
                    Ok(Color([r, g, b, a]))
                } else {
                    Err(serde::de::Error::custom(format!(
                        "Invalid hex color: '{}', expected #RRGGBB or #RRGGBBAA",
                        s
                    )))
                }
            }
            ColorHelper::Array4(arr) => Ok(Color(arr)),
            ColorHelper::Array3([r, g, b]) => Ok(Color([r, g, b, 255])),
        }
    }
}

impl Serialize for Color {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        format!(
            "#{:02x}{:02x}{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
        .serialize(serializer)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UiConfig {
    #[serde(default = "default_true")]
    pub show_top_bar: bool,

    #[serde(default = "default_true")]
    pub show_bottom_bar: bool,

    #[serde(default)]
    pub font: Option<String>,

    #[serde(default)]
    pub font_path: Option<String>,

    #[serde(default = "default_font_size")]
    pub font_size: f32,
}

impl UiConfig {
    pub fn font_spec(&self) -> Option<&str> {
        self.font.as_deref().or(self.font_path.as_deref())
    }
}

fn default_true() -> bool {
    true
}

fn default_font_size() -> f32 {
    20.0
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_top_bar: true,
            show_bottom_bar: true,
            font: None,
            font_path: None,
            font_size: 20.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColorConfig {
    #[serde(default = "default_bg_color")]
    pub background_color: Color,

    #[serde(default = "default_card_border_color")]
    pub card_border_color: Color,

    #[serde(default = "default_selected_border_color")]
    pub selected_border_color: Color,

    #[serde(default = "default_card_border_width")]
    pub card_border_width: f32,

    #[serde(default = "default_selected_border_width")]
    pub selected_border_width: f32,

    #[serde(default = "default_unselected_opacity")]
    pub unselected_opacity: f32,

    #[serde(default = "default_panel_bg_color")]
    pub panel_bg_color: Color,

    #[serde(default = "default_panel_border_color")]
    pub panel_border_color: Color,

    #[serde(default = "default_text_color")]
    pub text_color: Color,

    #[serde(default = "default_accent_color")]
    pub accent_color: Color,

    #[serde(default = "default_dim_text_color")]
    pub dim_text_color: Color,
}

fn default_bg_color() -> Color {
    Color([11, 14, 20, 225])
}
fn default_card_border_color() -> Color {
    Color([255, 255, 255, 64])
}
fn default_selected_border_color() -> Color {
    Color([0, 225, 255, 255])
}
fn default_card_border_width() -> f32 {
    1.5
}
fn default_selected_border_width() -> f32 {
    4.5
}
fn default_unselected_opacity() -> f32 {
    0.65
}
fn default_panel_bg_color() -> Color {
    Color([16, 20, 26, 220])
}
fn default_panel_border_color() -> Color {
    Color([0, 200, 255, 180])
}
fn default_text_color() -> Color {
    Color([245, 248, 255, 255])
}
fn default_accent_color() -> Color {
    Color([0, 225, 255, 255])
}
fn default_dim_text_color() -> Color {
    Color([140, 155, 175, 230])
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            background_color: default_bg_color(),
            card_border_color: default_card_border_color(),
            selected_border_color: default_selected_border_color(),
            card_border_width: default_card_border_width(),
            selected_border_width: default_selected_border_width(),
            unselected_opacity: default_unselected_opacity(),
            panel_bg_color: default_panel_bg_color(),
            panel_border_color: default_panel_border_color(),
            text_color: default_text_color(),
            accent_color: default_accent_color(),
            dim_text_color: default_dim_text_color(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,

    #[serde(default = "default_thumbnail_size")]
    pub max_thumbnail_size: u32,
}

fn default_thumbnail_size() -> u32 {
    960
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_thumbnail_size: 960,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub ui: UiConfig,

    #[serde(default)]
    pub colors: ColorConfig,

    #[serde(default)]
    pub cache: CacheConfig,
}

const DEFAULT_CONFIG_TEMPLATE: &str = r##"# swall configuration file

[ui]
# Show the top panel with image counter and filename
show_top_bar = true

# Show the bottom panel with keyboard controls
show_bottom_bar = true

# Font name/family (e.g. "Noto Sans", "JetBrains Mono", "monospace", "sans-serif")
# or direct file path to a .ttf/.otf font file.
# If empty or not found, built-in crisp bitmap font is used.
font = ""

# Font size in pixels
font_size = 20.0

[colors]
# Colors can be specified as hex string ("#0b0e14", "#0b0e14e1") or array ([r, g, b, a])
background_color = "#0b0e14e1"
panel_bg_color = "#10141adc"
panel_border_color = "#00c8ffb4"

card_border_color = "#ffffff40"
selected_border_color = "#00e1ffff"

card_border_width = 1.5
selected_border_width = 4.5
unselected_opacity = 0.65

text_color = "#f5f8ffff"
accent_color = "#00e1ffff"
dim_text_color = "#8c9baff0"

[cache]
# Enable disk caching of downscaled thumbnails for instant loading
enabled = true

# Maximum thumbnail dimension in pixels (960x540 for 16:9 previews)
max_thumbnail_size = 960
"##;

impl Config {
    pub fn config_path() -> Option<PathBuf> {
        if let Ok(dir) = std::env::var("XDG_CONFIG_HOME") {
            Some(PathBuf::from(dir).join("swall/config.toml"))
        } else if let Ok(home) = std::env::var("HOME") {
            Some(PathBuf::from(home).join(".config/swall/config.toml"))
        } else {
            None
        }
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };

        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                match toml::from_str::<Config>(&content) {
                    Ok(cfg) => return cfg,
                    Err(e) => {
                        eprintln!(
                            "swall: warning: failed to parse config at {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
        } else if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
            let _ = fs::write(&path, DEFAULT_CONFIG_TEMPLATE);
        }

        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_template_config() {
        let cfg: Config = toml::from_str(DEFAULT_CONFIG_TEMPLATE).unwrap();
        assert!(cfg.ui.show_top_bar);
        assert!(cfg.ui.show_bottom_bar);
        assert_eq!(cfg.colors.selected_border_color, Color([0, 225, 255, 255]));
        assert_eq!(cfg.cache.max_thumbnail_size, 960);
    }

    #[test]
    fn test_parse_color_hex_and_array() {
        #[derive(Deserialize)]
        struct S {
            c1: Color,
            c2: Color,
            c3: Color,
        }
        let toml_str = r##"
            c1 = "#ff007f"
            c2 = "#00e1ffaa"
            c3 = [10, 20, 30, 40]
        "##;
        let s: S = toml::from_str(toml_str).unwrap();
        assert_eq!(s.c1.to_rgba8(), [255, 0, 127, 255]);
        assert_eq!(s.c2.to_rgba8(), [0, 225, 255, 170]);
        assert_eq!(s.c3.to_rgba8(), [10, 20, 30, 40]);
    }
}
