use ansimake::{ConversionConfig, Image};
use clap::Parser;
use std::io::{self, Write};

#[derive(Parser)]
#[command(name = "ansimake")]
#[command(about = "Convert PNG images to ANSI art")]
struct Args {
    #[arg()]
    image_path: String,

    #[arg(short = 'b', long = "bw")]
    black_white: bool,

    #[arg(short = 'w', long = "width")]
    width: Option<u32>,

    #[arg(long = "height")]
    height: Option<u32>,

    #[arg(short = 't', long = "tolerance", default_value = "0")]
    color_tolerance: f32,

    #[arg(short = 'B', long = "blocks")]
    use_blocks: bool,
}

fn get_terminal_size() -> (u32, u32) {
    if let Some((w, h)) = terminal_size::terminal_size() {
        (u32::from(w.0), u32::from(h.0))
    } else {
        (80, 24)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let img = if args.black_white {
        Image::load(&args.image_path)?.to_grayscale()
    } else {
        Image::load(&args.image_path)?
    };

    let original_width = img.width();
    let original_height = img.height();
    #[allow(clippy::cast_precision_loss)]
    let aspect_ratio = original_width as f32 / original_height as f32;

    let (width, height) = if let Some(w) = args.width {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let h = args
            .height
            .map_or_else(|| (w as f32 / aspect_ratio) as u32, |h| h);
        (w as usize, h as usize)
    } else {
        let (term_width, term_height) = get_terminal_size();
        let max_width = term_width.saturating_sub(2);
        let max_height = term_height.saturating_sub(2);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        {
            let width = max_width.min((max_height as f32 * aspect_ratio) as u32);
            let height = (width as f32 / aspect_ratio) as u32;
            let height = height.min(max_height);
            let width = (height as f32 * aspect_ratio) as u32;
            (width as usize, height as usize)
        }
    };

    let config = ConversionConfig {
        size: (width, height),
        alpha_threshold: 128,
        color_tolerance: args.color_tolerance,
        use_blocks: args.use_blocks,
        ..Default::default()
    };
    let ansi_art = img.to_ansi(&config);

    let stdout = io::stdout();
    let mut handle = stdout.lock();
    write!(handle, "{ansi_art}")?;
    handle.flush()?;

    Ok(())
}
