use fframes::{
    CombinedMediaProvider, EncoderOptions, MediaDirectory, MediaProvider, RenderOptions,
    StaticMediaProvider, cli,
};
use fframes_skia_renderer::{
    SkiaFFramesRenderer, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig,
    vulkan::SkiaVulkanCtx,
};
use ph_today::{Board, HEIGHT, PhTodayMedia, PhTodayVideo, WIDTH};
use fframes::cli::clap; // the derive below expands to `clap::...`
use std::{path::PathBuf, process::ExitCode};

/// Flags of this video next to the standard ones of `fframes::cli` (render, frame, strip,
/// inspect, audio, ...). Run `cargo run --release -- --help`.
#[derive(Debug, clap::Args)]
struct VideoArgs {
    /// Folder written by `scripts/fetch.py`: products.json and media/.
    #[arg(long, default_value = "data", global = true)]
    data: PathBuf,
}

fn main() -> ExitCode {
    let args = cli::parse::<VideoArgs>();
    let board = match Board::load(&args.app.data) {
        Ok(board) => board,
        Err(err) => {
            eprintln!("{err}");
            return ExitCode::FAILURE;
        }
    };
    let day_media = match MediaDirectory::read_folder(args.app.data.join("media")) {
        Ok(dir) => dir,
        Err(err) => {
            eprintln!("{}: {err}. Run `python scripts/fetch.py` first.", args.app.data.join("media").display());
            return ExitCode::FAILURE;
        }
    };
    let day_media = day_media.process_media_source().expect("decode the day's images");
    // Loaded at runtime rather than embedded so the music and effects stay stereo.
    let sounds = MediaDirectory::read_folder("audio").expect("audio/ (run `python scripts/make_music.py`)");
    let sounds = sounds.process_media_source().expect("decode audio/");
    let static_media = PhTodayMedia::prepare().expect("media");
    let media = CombinedMediaProvider::from([&static_media as &dyn MediaProvider, &sounds, &day_media]);
    let video = PhTodayVideo::new(&board);
    let gpu = SkiaVulkanCtx::new(WIDTH, HEIGHT).expect("GPU context");

    cli::new(
        &video,
        RenderOptions {
            media: Some(&media),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx264"),
                // Sharp, high-bitrate 1080p: screenshots carry small UI text.
                codec_params: Some(&[("crf", "14"), ("preset", "slow"), ("profile", "high")]),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .args(args)
    .default_output("product-hunt-today.mp4")
    // Frame previews (frame, strip, onion, snapshot) render with the same Skia backend.
    .backend(
        SkiaFFramesRenderer::new_vulkan(
            &gpu,
            SkiaPipelineConfig {
                concurrency_policy: SkiaPipelineConcurrencyPolicy::MaxPerformance,
                ..Default::default()
            },
        )
        .expect("skia renderer"),
    )
    // `preview` opens the real-time GPU player.
    .preview(fframes_native_player::cli_preview)
    .run()
}
