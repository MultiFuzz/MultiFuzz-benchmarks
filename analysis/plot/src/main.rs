use std::path::{Path, PathBuf};

use anyhow::Context;
use plotters::{backend::SVGBackend, prelude::IntoDrawingArea};

mod coverage;
mod survival;
mod utils;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error running plotter: {e:#}");
        eprintln!("{}", e.backtrace());
    }
}

fn run() -> anyhow::Result<()> {
    polars::enable_string_cache();

    let config: PathBuf =
        std::env::var_os("CONFIG").map_or_else(|| "config.ron".into(), |x| x.into());

    let config = plot_data::Config::from_path(&config)?;
    let _ = std::fs::create_dir_all("output");

    let plots = std::env::args().nth(1).map(|x| {
        x.split(',')
            .map(|x| x.trim().to_owned())
            .collect::<Vec<_>>()
    });
    let should_plot = |target: &str| {
        plots
            .as_ref()
            .map_or(true, |x| x.iter().any(|x| x == target))
    };

    if should_plot("coverage") {
        eprintln!("plotting coverage");

        let data = plot_data::analysis::summarize_coverage(
            plot_data::load_block_hits(&config).context("failed to load block hits")?,
        )
        .collect()?;

        let n_binaries = data["binary"].n_unique()?;
        let (n_col, dims) = config.plot_layout.get_layout(n_binaries as u32);
        let out =
            SVGBackend::new(Path::new("output/coverage.svg"), dims.into()).into_drawing_area();
        coverage::coverage_over_time(&out, &config, &data, n_col)?;
    }

    if should_plot("survival") && !config.survival.is_empty() {
        eprintln!("plotting survival");

        let coverage = plot_data::load_raw_coverage(&config)?.cache();
        let block_survival =
            plot_data::analysis::block_survival(coverage.clone(), &config.survival)?;
        let block_hits = plot_data::analysis::raw_blocks_hit(coverage);

        let (n_col, dims) = config
            .survival_layout
            .get_layout(config.survival.len() as u32);
        let out =
            SVGBackend::new(Path::new("output/survival.svg"), dims.into()).into_drawing_area();

        survival::plot_survival(&out, &config, n_col as usize, block_hits, block_survival)?
    }

    Ok(())
}
