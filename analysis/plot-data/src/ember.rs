use std::path::Path;

use polars::prelude::*;

use crate::{load_glob, parse_u64_with_prefix};

pub fn read_all(glob: &str, resampled: bool) -> anyhow::Result<Option<LazyFrame>> {
    let loader = if resampled { read_resampled_csv } else { read_raw_csv };
    let data = load_glob(glob, loader, |_| true)?;
    if data.is_empty() {
        eprintln!("WARNING: No files found for: {glob}");
        return Ok(None);
    }
    Ok(Some(concat(data, UnionArgs::default())?.collect()?.lazy()))
}

/// Read ember data from tab delimited CSV files (e.g. `P2IM-PLC-run-05.txt`).
fn read_raw_csv(path: &Path) -> anyhow::Result<LazyFrame> {
    let (_target, binary, trial) = extract_group_binary_trial_from_path(path).ok_or_else(|| {
        anyhow::format_err!("Failed to parse binary and trial ID from path: {}", path.display())
    })?;

    // ```
    // Seconds\tBlocks
    // 0\t59
    // 60\t308
    // ```
    let mut schema = Schema::new();
    schema.with_column("seconds".into(), DataType::Float64);
    schema.with_column("blocks".into(), DataType::UInt32);
    Ok(LazyCsvReader::new(path)
        .with_has_header(false)
        .with_skip_rows(1)
        .with_separator(b'\t')
        .with_schema(Some(schema.into()))
        .finish()?
        .with_columns([lit(binary).alias("binary"), lit(trial).alias("trial")]))
}

/// Read resampled Ember-IO data from comma delimeted CSV files (e.g. `P2IM-PLC-run-05.csv`).
fn read_resampled_csv(path: &Path) -> anyhow::Result<LazyFrame> {
    let (_target, binary, trial) = extract_group_binary_trial_from_path(path).ok_or_else(|| {
        anyhow::format_err!("Failed to parse binary and trial ID from path: {}", path.display())
    })?;

    let mut schema = Schema::new();
    schema.with_column("seconds".into(), DataType::Float64);
    schema.with_column("blocks".into(), DataType::UInt32);
    Ok(LazyCsvReader::new(path)
        .with_has_header(false)
        .with_separator(b',')
        .with_schema(Some(schema.into()))
        .finish()?
        .with_column(col("seconds").floor().cast(DataType::Int64))
        .with_columns([lit(binary).alias("binary"), lit(trial).alias("trial")]))
}

fn extract_group_binary_trial_from_path(path: &Path) -> Option<(&str, &str, u32)> {
    let stem = path.file_stem().and_then(|x| x.to_str())?;
    // Remove resampled suffix (if it is there).
    let stem = stem.strip_suffix(".resampled").unwrap_or(stem);

    // e.g., P2IM or uEMU
    let (target, rest) = stem.split_once('-')?;
    // Binaries names may include interior `-` characters, so use split from the back.
    let (rest, trial) = rest.rsplit_once('-')?;
    let (binary, _run_str) = rest.rsplit_once('-')?;

    // Parse trial, and adjust to be zero-based.
    let trial = parse_u64_with_prefix(trial).ok()?.checked_sub(1)? as u32;

    // Adjust name of binary to match Fuzzware and MultiFuzz.
    let binary = normalize_binary_name(binary);

    Some((target, binary, trial))
}

pub fn normalize_binary_name(name: &str) -> &str {
    match name {
        "6LoWPAN-Receiver" => "6LoWPAN_Receiver",
        "6LoWPAN-Sender" => "6LoWPAN_Sender",
        "Zephyr_SocketCAN" => "Zepyhr_SocketCan",
        "uTasker_MODBUS" => "utasker_MODBUS",
        "uTasker_USB" => "utasker_USB",
        "GPS_Tracker" => "uEmu.GPSTracker",
        "RF_Doorlock" => "RF_Door_Lock",
        "3D_Printer" => "uEmu.3Dprinter",
        _ => name,
    }
}
