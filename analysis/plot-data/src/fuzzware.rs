use std::path::Path;

use anyhow::ensure;
use polars::prelude::*;

use crate::{load_glob, parse_u64_with_prefix, polars_parse_u64};

pub fn read_all(glob: &str) -> anyhow::Result<Option<LazyFrame>> {
    let data = load_glob(glob, read_raw_csv, |_| true)?;
    if data.is_empty() {
        eprintln!("WARNING: No raw Fuzzware csv files found for: {glob}");
        return Ok(None);
    }

    // Merge data and explode the list of hit blocks to separate rows.
    Ok(Some(concat(data, UnionArgs::default())?
        .drop_nulls(Some(vec![col("blocks")]))
        .drop(["num_bbs_total"])
        .with_column(col("blocks").str().split(lit(" ")))
        .explode(["blocks"])
        .rename(["blocks"], ["block"])
        .with_column(polars_parse_u64(col("block")))))
}

/// Read fuzzware data from raw CSV files.
fn read_raw_csv(path: &Path) -> anyhow::Result<LazyFrame> {
    fn extract_binary_and_trial_path(path: &Path) -> Option<(&str, u32)> {
        // Check which format the path is in by checking the parent directory.
        let parent_name = path.parent()?.file_name()?.to_str()?;

        // <binary>/fuzzware-project-run-<trial>/stats/covered_bbs_by_second_into_experiment.csv
        let (binary, trial) = if parent_name == "stats" {
            let project = path.parent()?.parent()?;
            let binary = project.parent()?.file_name()?.to_str()?;
            let (_, trial) = project.file_name()?.to_str()?.rsplit_once('-')?;
            (binary, trial)
        }
        // <binary>/<trial>_covered_bbs_by_second_into_experiment.csv
        else {
            let stem = path.file_stem()?.to_str()?;
            let (trial, _) = stem.split_once('_')?;
            (parent_name, trial)
        };

        // Parse trial, and adjust to be zero-based.
        let trial = parse_u64_with_prefix(trial).ok()?.checked_sub(1)? as u32;
        Some((binary, trial))
    }

    let (binary, trial) = extract_binary_and_trial_path(path).ok_or_else(|| {
        anyhow::format_err!(
            "Failed to parse binary and trial ID from path: {}",
            path.display()
        )
    })?;

    let mut schema = Schema::new();
    schema.with_column("seconds".into(), DataType::Int64);
    schema.with_column("num_bbs_total".into(), DataType::UInt32);
    schema.with_column("blocks".into(), DataType::String);
    Ok(LazyCsvReader::new(path)
        .with_has_header(false)
        .with_comment_prefix(Some("#"))
        .with_separator(b'\t')
        .with_schema(Some(schema.into()))
        .finish()?
        .with_columns([lit(binary).alias("binary"), lit(trial).alias("trial")]))
}

pub mod legacy {
    use super::*;

    const DAT_GLOB: &str = "../results/multi-stream/reference/Fuzzware/*.resampled.csv";
    pub fn read_all() -> anyhow::Result<LazyFrame> {
        let data = load_glob(DAT_GLOB, read_fuzzware_dat, |_| true)?;
        ensure!(
            !data.is_empty(),
            "No resampled Fuzzware dat csv files found for: {DAT_GLOB}"
        );
        Ok(concat(data, UnionArgs::default())?)
    }

    /// Extract binary name and trial from a path that ends like
    /// `P2IM_CNC_fuzzware-project-run-01.resampled.csv`
    fn extract_binary_and_trial_from_dat_path(path: &Path) -> Option<(&str, u32)> {
        let stem = path.file_stem().and_then(|x| x.to_str())?;
        let (binary, rest) = stem.split_once("_fuzzware-project-run-")?;

        let trial_str = rest.strip_suffix(".resampled").unwrap_or(rest);
        let trial = parse_u64_with_prefix(trial_str).ok()?.checked_sub(1)? as u32;

        Some((binary, trial))
    }

    /// Read data from preprocessed `.dat` files.
    fn read_fuzzware_dat(path: &Path) -> anyhow::Result<LazyFrame> {
        // Fuzzware doesn't include any metadata about the trial run in the file, so extract binary
        // and trial name from the file name instead.
        let (binary, trial) = extract_binary_and_trial_from_dat_path(path).ok_or_else(|| {
            anyhow::format_err!(
                "Failed to parse binary and trial ID from path: {}",
                path.display()
            )
        })?;

        let mut schema = Schema::new();
        schema.with_column("hours".into(), DataType::Float64);
        schema.with_column("blocks".into(), DataType::UInt32);
        Ok(LazyCsvReader::new(path)
            .with_has_header(false)
            .with_separator(b' ')
            .with_schema(Some(schema.into()))
            .finish()?
            .with_columns([lit(binary).alias("binary"), lit(trial).alias("trial")]))
    }
}
