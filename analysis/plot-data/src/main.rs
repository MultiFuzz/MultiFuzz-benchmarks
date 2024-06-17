use std::path::{Path, PathBuf};

use plot_data::order_by_binary;
use polars::prelude::*;

use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn main() -> anyhow::Result<()> {
    polars::enable_string_cache();

    let config_path: PathBuf =
        std::env::var_os("CONFIG").map_or_else(|| "config.ron".into(), |x| x.into());
    let config = plot_data::Config::from_path(&config_path)?;
    let _ = std::fs::create_dir_all("output");

    let show = std::env::args().nth(1).map(|x| {
        x.split(',')
            .map(|x| x.trim().to_owned())
            .collect::<Vec<_>>()
    });
    let should_show = |target: &str| {
        show.as_ref()
            .map_or(true, |x| x.iter().any(|x| x == target))
    };

    if should_show("coverage") {
        let mut coverage_table = plot_data::analysis::coverage_table(&config)?
            .sort_by_exprs(
                [col("fuzzer"), order_by_binary()],
                SortMultipleOptions::new()
                    .with_nulls_last(true)
                    .with_maintain_order(true),
            )
            .collect()?;
        println!("total_blocks: {:?}", coverage_table);
        write_csv(&mut coverage_table, "output/total_blocks.csv")?;

        let block_hits = plot_data::load_block_hits(&config)?.collect()?;
        println!("block hits: {block_hits}");
    }

    if should_show("median-coverage") {
        let mut median_coverage = plot_data::analysis::median_coverage(&config)?;
        println!("median_coverage: {:?}", median_coverage);
        write_csv(&mut median_coverage, "output/median_coverage.csv")?;
    }

    if should_show("final-coverage") {
        let coverage = plot_data::load_block_hits(&config)?;
        let final_coverage = coverage
            .group_by(["dataset", "fuzzer", "binary", "trial"])
            .agg([col("blocks").max().alias("total_blocks")])
            .sort_by_exprs(
                [col("binary"), col("fuzzer"), col("trial")],
                SortMultipleOptions::new().with_nulls_last(true),
            )
            .collect();
        println!("{final_coverage:?}");
    }

    if should_show("survival") {
        let coverage = plot_data::load_raw_coverage(&config)?;
        let survival = plot_data::analysis::block_survival(coverage, &config.survival)?;
        let (mut survival, mut profile) = survival.profile()?;
        println!("{profile}");
        println!("{survival}");
        write_csv(&mut survival, "output/survival.csv")?;
        write_csv(&mut profile, "output/profile.csv")?;
    }

    Ok(())
}

fn write_csv(df: &mut DataFrame, path: impl AsRef<Path>) -> anyhow::Result<()> {
    Ok(CsvWriter::new(&mut std::fs::File::create(path)?)
        .include_header(true)
        .with_separator(b',')
        .finish(df)?)
}
