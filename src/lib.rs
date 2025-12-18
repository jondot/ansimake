#![doc = include_str!("../README.md")]

use image::imageops::{colorops::grayscale, FilterType};
use image::{DynamicImage, GenericImageView};
use palette::{IntoColor, Lab, Srgb};
use std::fmt::Write;
use std::path::Path;

const SHADE_BLOCKS: &[char] = &[' ', '\u{2591}', '\u{2592}', '\u{2593}', '\u{2588}'];

const TOP_HALF: &str = "\u{2580}";
const BOTTOM_HALF: &str = "\u{2584}";

fn rgb_to_brightness(r: u8, g: u8, b: u8) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (0.299f32.mul_add(
            f32::from(r),
            0.587f32.mul_add(f32::from(g), 0.114 * f32::from(b)),
        )) as u8
    }
}

fn get_shade_block(brightness: u8) -> char {
    let normalized = f32::from(brightness) / 255.0;
    let perceptual = normalized.powf(1.0 / 2.2);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    {
        let max_index = SHADE_BLOCKS.len() - 1;
        let len_f32 = u32::try_from(max_index).unwrap_or(0) as f32;
        let index = (perceptual * len_f32).round() as usize;
        SHADE_BLOCKS[index.min(max_index)]
    }
}

fn get_structured_block(
    pixels: &[Vec<[u8; 4]>],
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) -> char {
    if y < height && x < width {
        let pix = pixels[y][x];
        let brightness = rgb_to_brightness(pix[0], pix[1], pix[2]);
        get_shade_block(brightness)
    } else {
        ' '
    }
}

fn cielab_distance(r1: u8, g1: u8, b1: u8, r2: u8, g2: u8, b2: u8) -> f32 {
    let color1: Srgb<u8> = Srgb::new(r1, g1, b1);
    let color2: Srgb<u8> = Srgb::new(r2, g2, b2);
    let lab1: Lab = color1.into_linear::<f32>().into_color();
    let lab2: Lab = color2.into_linear::<f32>().into_color();
    let dl = lab1.l - lab2.l;
    let da = lab1.a - lab2.a;
    let db = lab1.b - lab2.b;
    db.mul_add(db, dl.mul_add(dl, da * da)).sqrt()
}

fn quantize_color(
    r: u8,
    g: u8,
    b: u8,
    palette: &mut Vec<(u8, u8, u8)>,
    tolerance: f32,
) -> (u8, u8, u8) {
    if tolerance <= 0.0 {
        return (r, g, b);
    }

    let mut min_dist = tolerance + 1.0;
    let mut best_color = None;

    for &(pr, pg, pb) in palette.iter() {
        let dist = cielab_distance(r, g, b, pr, pg, pb);
        if dist < min_dist {
            min_dist = dist;
            best_color = Some((pr, pg, pb));
        }
    }

    best_color.map_or_else(
        || {
            let color = (r, g, b);
            palette.push(color);
            color
        },
        |color| color,
    )
}

pub struct Image {
    inner: DynamicImage,
}

impl Image {
    /// Load an image from a file path.
    ///
    /// # Errors
    ///
    /// Returns an error if the image file cannot be opened or decoded.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, image::ImageError> {
        let img = image::open(path)?;
        Ok(Self { inner: img })
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.inner.width()
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.inner.height()
    }

    #[must_use]
    pub fn to_grayscale(mut self) -> Self {
        self.inner = DynamicImage::ImageLuma8(grayscale(&self.inner));
        self
    }

    #[must_use]
    pub fn to_ansi(&self, config: &ConversionConfig) -> String {
        convert_image(&self.inner, config)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConversionConfig {
    pub size: (usize, usize),
    pub alpha_threshold: u8,
    pub raw: bool,
    pub resize_filter: FilterType,
    pub color_tolerance: f32,
    pub use_blocks: bool,
}

impl Default for ConversionConfig {
    fn default() -> Self {
        Self {
            size: (80, 24),
            alpha_threshold: 128,
            raw: false,
            resize_filter: FilterType::Nearest,
            color_tolerance: 0.0,
            use_blocks: false,
        }
    }
}

fn resize_image(image: &DynamicImage, config: &ConversionConfig) -> Vec<Vec<[u8; 4]>> {
    let mut pixels: Vec<Vec<[u8; 4]>> = vec![];
    #[allow(clippy::cast_possible_truncation)]
    {
        let width = u32::try_from(config.size.0).unwrap_or(u32::MAX);
        let height = u32::try_from(config.size.1).unwrap_or(u32::MAX);
        for (x, y, pix) in image.resize(width, height, config.resize_filter).pixels() {
            if x == 0 {
                pixels.push(vec![]);
            }
            pixels[y as usize].push(pix.0);
        }
    }
    pixels
}

fn convert_blocks_mode(pixels: &[Vec<[u8; 4]>], config: &ConversionConfig, esc: &str) -> String {
    let mut color_palette: Vec<(u8, u8, u8)> = Vec::new();
    let mut out = String::new();

    for line in 0..pixels.len() {
        let mut last_fg: Option<(u8, u8, u8)> = None;
        for char in 0..pixels[line].len() {
            let mut pix: [u8; 4] = pixels[line][char];

            if config.color_tolerance > 0.0 {
                let (r, g, b) = quantize_color(
                    pix[0],
                    pix[1],
                    pix[2],
                    &mut color_palette,
                    config.color_tolerance,
                );
                pix[0] = r;
                pix[1] = g;
                pix[2] = b;
            }

            if pix[3] < config.alpha_threshold {
                out.push(' ');
                last_fg = None;
            } else {
                let block =
                    get_structured_block(pixels, char, line, pixels[line].len(), pixels.len());
                let current_fg = (pix[0], pix[1], pix[2]);
                if last_fg != Some(current_fg) {
                    write!(out, "{esc}[38;2;{};{};{}m", pix[0], pix[1], pix[2]).unwrap();
                    last_fg = Some(current_fg);
                }
                out.push(block);
            }
        }
        if last_fg.is_some() {
            write!(out, "{esc}[0m").unwrap();
        }
        out.push('\n');
    }
    out
}

fn convert_half_blocks_mode(
    pixels: &[Vec<[u8; 4]>],
    config: &ConversionConfig,
    esc: &str,
) -> String {
    let mut color_palette: Vec<(u8, u8, u8)> = Vec::new();
    let mut out = String::new();

    for line in (0..pixels.len()).filter(|index| index % 2 == 0) {
        for char in 0..pixels[line].len() {
            let mut top_pix: [u8; 4] = pixels[line][char];
            let mut bot_pix: [u8; 4] = if line + 1 >= pixels.len() {
                [0; 4]
            } else {
                pixels[line + 1][char]
            };

            if config.color_tolerance > 0.0 {
                let (r, g, b) = quantize_color(
                    top_pix[0],
                    top_pix[1],
                    top_pix[2],
                    &mut color_palette,
                    config.color_tolerance,
                );
                top_pix[0] = r;
                top_pix[1] = g;
                top_pix[2] = b;

                if bot_pix[3] >= config.alpha_threshold {
                    let (r, g, b) = quantize_color(
                        bot_pix[0],
                        bot_pix[1],
                        bot_pix[2],
                        &mut color_palette,
                        config.color_tolerance,
                    );
                    bot_pix[0] = r;
                    bot_pix[1] = g;
                    bot_pix[2] = b;
                }
            }

            let top_invis: bool = top_pix[3] < config.alpha_threshold;
            let bot_invis: bool = bot_pix[3] < config.alpha_threshold;
            if top_invis && bot_invis {
                out.push(' ');
            } else if top_invis && !bot_invis {
                write!(
                    out,
                    "{esc}[38;2;{};{};{}m{}{esc}[0m",
                    bot_pix[0], bot_pix[1], bot_pix[2], BOTTOM_HALF
                )
                .unwrap();
            } else if !top_invis && bot_invis {
                write!(
                    out,
                    "{esc}[38;2;{};{};{}m{}{esc}[0m",
                    top_pix[0], top_pix[1], top_pix[2], TOP_HALF
                )
                .unwrap();
            } else {
                write!(
                    out,
                    "{esc}[38;2;{};{};{};48;2;{};{};{}m{}{esc}[0m",
                    bot_pix[0],
                    bot_pix[1],
                    bot_pix[2],
                    top_pix[0],
                    top_pix[1],
                    top_pix[2],
                    BOTTOM_HALF
                )
                .unwrap();
            }
        }
        out.push('\n');
    }
    out
}

fn convert_image(image: &DynamicImage, config: &ConversionConfig) -> String {
    let esc = if config.raw { "\\x1b" } else { "\x1b" };
    let pixels = resize_image(image, config);

    if config.use_blocks {
        convert_blocks_mode(&pixels, config, esc)
    } else {
        convert_half_blocks_mode(&pixels, config, esc)
    }
}
