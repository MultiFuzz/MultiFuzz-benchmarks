use std::path::Path;

use anyhow::Context;
use polars::prelude::*;

pub use crate::config::Config;
use crate::{
    analysis::BlockHits,
    config::{DataSource, FilterExpr},
    metadata::Metadata,
};

pub mod analysis;
pub mod config;
mod data_loading;
pub mod ember;
pub mod fuzzware;
pub mod metadata;
pub mod multifuzz;

pub fn name_of_binary(name: &str) -> String {
    let name = name.strip_prefix("P2IM_").unwrap_or(name);
    let name = name.strip_prefix("uEmu_").unwrap_or(name);

    match name {
        // P2IM dataset
        "CNC" => "P2IM/CNC".into(),
        "Drone" => "P2IM/Drone".into(),
        "Heat_Press" => "P2IM/Heat Press".into(),
        "Reflow_Oven" => "P2IM/Reflow Oven".into(),
        "Soldering_Iron" => "P2IM/Soldering Iron".into(),
        "Console" => "P2IM/Console".into(),
        "Gateway" => "P2IM/Gateway".into(),
        "PLC" => "P2IM/PLC".into(),
        "Robot" => "P2IM/Robot".into(),
        "Steering_Control" => "P2IM/Steering Control".into(),

        // uEmu dataset
        "uEmu.3Dprinter" => "uEmu/3D Printer".into(),
        "uEmu.GPSTracker" => "uEmu/GPS Tracker".into(),
        "LiteOS_IoT" => "uEmu/LiteOS IoT".into(),
        "utasker_MODBUS" => "uEmu/uTasker MODBUS".into(),
        "utasker_USB" => "uEmu/uTasker USB".into(),
        "Zepyhr_SocketCan" => "uEmu/Zephyr SocketCan".into(),

        // Pretender
        "RF_Door_Lock" => "Pretender/RF Door Lock".into(),
        "Thermostat" => "Pretender/Thermostat".into(),

        // WYCINWYC
        "XML_Parser" => "WYCINWYC/XML Parser".into(),

        // HALucinator
        "6LoWPAN_Receiver" => "HALucinator/6LoWPAN Receiver".into(),
        "6LoWPAN_Sender" => "HALucinator/6LoWPAN".into(),

        // MultiFuzz
        "riot-gnrc_networking" => "gnrc networking".into(),
        "riot-filesystem" => "File System".into(),
        "riot-ccn-lite-relay" => "CCN-Lite Relay".into(),

        // Unknown binary, use original name but with a `?` for debugging.
        _ => format!("{name}?"),
    }
}

pub fn binary_order(name: &str) -> u64 {
    const NAMES: &[&str] = &[
        // P2IM
        "CNC",
        "Console",
        "Drone",
        "Reflow_Oven",
        "Robot",
        "Steering_Control",
        // uEmu
        "6LoWPAN_Sender",
        "6LoWPAN_Receiver",
        "LiteOS_IoT",
        "uEmu.3Dprinter",
        "uEmu.GPSTracker",
        "utasker_MODBUS",
        "utasker_USB",
        "XML_Parser",
        "Zepyhr_SocketCan",
        // Binaries with bug exploits.
        "Gateway",
        "Soldering_Iron",
        "Heat_Press",
        "PLC",
        "Thermostat",
        "RF_Door_Lock",
        // New
        "riot-lorawan",
        "riot-gnrc_networking",
        "riot-filesystem",
        "riot-ccn-lite-relay",
    ];
    NAMES.iter().position(|x| *x == name).unwrap_or(usize::MAX) as u64
}

pub fn has_bug_exploit(name: &str) -> bool {
    match name {
        "Heat_Press" | "PLC" | "Soldering_Iron" | "RF_Door_Lock" | "Thermostat" | "Gateway" => true,
        _ => false,
    }
}

pub fn map_binary_names(col: Expr) -> Expr {
    col.map(
        |rows| {
            Ok(Some(
                rows.str()?
                    .into_no_null_iter()
                    .map(name_of_binary)
                    .collect(),
            ))
        },
        GetOutput::default(),
    )
}

pub fn order_by_binary() -> Expr {
    fn get_binary_order(col_binary: Series) -> PolarsResult<Series> {
        Ok(col_binary
            .str()?
            .into_no_null_iter()
            .map(binary_order)
            .collect())
    }
    col("binary").map(|bin| Ok(Some(get_binary_order(bin)?)), GetOutput::default())
}

pub fn parse_filter_expr(filter: &FilterExpr) -> Expr {
    match filter {
        FilterExpr::Col(name) => col(name.as_str()),
        FilterExpr::Str(e) => lit(e.as_str()),
        FilterExpr::U32(x) => lit(*x),
        FilterExpr::U64(x) => lit(*x),
        FilterExpr::Eq(a, b) => parse_filter_expr(a).eq(parse_filter_expr(b)),
        FilterExpr::Neq(a, b) => parse_filter_expr(a).neq(parse_filter_expr(b)),
        FilterExpr::And(exprs) => {
            let mut expr = lit(true);
            for a in exprs {
                expr = expr.and(parse_filter_expr(a));
            }
            expr
        }
        FilterExpr::Or(exprs) => {
            let mut expr = lit(false);
            for a in exprs {
                expr = expr.or(parse_filter_expr(a));
            }
            expr
        }
        FilterExpr::Not(expr) => parse_filter_expr(expr).not(),
        FilterExpr::True => lit(true),
    }
}

pub fn load_block_hits(config: &Config) -> anyhow::Result<BlockHits> {
    let mut data = vec![];
    let res = config.time_resolution as i64;

    let valid_blocks = match config.coverage_metadata.as_ref() {
        Some(metadata) => {
            Some(valid_blocks(&Metadata::from_source(&config.path, metadata.clone())?)?.cache())
        }
        None => None,
    };
    let filter_valid = |lf: LazyFrame| {
        let Some(valid_blocks) = valid_blocks.clone() else {
            return lf;
        };
        lf.join(
            valid_blocks.clone(),
            [col("binary"), col("block")],
            [col("binary"), col("block")],
            JoinType::Inner.into(),
        )
        .sort(["time"], SortMultipleOptions::default())
    };

    let group = &[col("binary"), col("trial")];
    for (id, name, entry) in config.datasets() {
        let filter = parse_filter_expr(&entry.filter);
        let dataset = match &entry.source {
            DataSource::FuzzwareBlocksCsv { glob, duration } => {
                let Some(data) = fuzzware::read_all(glob)? else {
                    continue;
                };
                let raw = filter_valid(data.filter(filter).rename(["seconds"], ["time"]));
                analysis::blocks_hit_per_period(raw, duration.as_secs() as i64, res, "time", group)?
                    .with_column(secs_to_hours(col("time")))
                    .drop(["time"])
            }
            DataSource::MultiFuzzBench { glob, duration } => {
                let Some(data) = multifuzz::read_all(glob)? else {
                    continue;
                };
                let raw = filter_valid(data.filter(filter));
                let duration_ms = duration.as_millis() as i64;
                analysis::blocks_hit_per_period(raw, duration_ms, res, "time", group)?
                    .with_column(millis_to_hours(col("time")))
                    .drop(["time"])
            }
            DataSource::EmberCsv {
                glob,
                duration,
                resampled,
            } => {
                let Some(data) = ember::read_all(glob, *resampled)? else {
                    continue;
                };
                let raw = data
                    .filter(filter)
                    .rename(["seconds"], ["time"])
                    .with_column(lit(name.as_str()).alias("fuzzer"));
                analysis::fill_missing(raw, duration.as_secs() as i64, res, "time", group)?
                    .with_column(secs_to_hours(col("time")))
                    .drop(["time"])
            }
        };
        data.push(dataset.with_columns([
            lit(name.as_str()).alias("fuzzer"),
            lit(id as u32).alias("dataset"),
        ]))
    }
    let global_filter = parse_filter_expr(&config.filter);
    Ok(concat_lf_diagonal(data, UnionArgs::default())?.filter(global_filter))
}

/// Represents a lazy frame generated by `load_raw_coverage`
pub type Coverage = LazyFrame;

pub fn load_raw_coverage(config: &Config) -> anyhow::Result<Coverage> {
    let global_filter = parse_filter_expr(&config.filter);
    let mut data = vec![];
    for (id, name, entry) in config.datasets() {
        let filter = global_filter.clone().and(parse_filter_expr(&entry.filter));
        let dataset = match &entry.source {
            DataSource::FuzzwareBlocksCsv { glob, .. } => {
                let Some(data) = fuzzware::read_all(glob)? else {
                    continue;
                };
                data.filter(filter)
                    .with_column(secs_to_hours(col("seconds")))
                    .drop(["seconds"])
            }
            DataSource::MultiFuzzBench { glob, .. } => {
                let Some(data) = multifuzz::read_all(glob)? else {
                    continue;
                };
                data.filter(filter)
                    .with_column(millis_to_hours(col("time")))
                    .drop(["time"])
            }
            DataSource::EmberCsv { .. } => {
                // Raw coverage unsupported
                continue;
            }
        };
        data.push(dataset.with_columns([
            lit(name.as_str()).alias("fuzzer"),
            lit(id as u32).alias("dataset"),
        ]));
    }
    let data = concat_lf_diagonal(data, UnionArgs::default())?;

    // Filter coverage to only include valid blocks (if metadata is available).
    if let Some(metadata) = config.coverage_metadata.as_ref() {
        let valid_blocks = valid_blocks(&Metadata::from_source(&config.path, metadata.clone())?)?;
        let join_key = [col("binary"), col("block")];
        Ok(data
            .join(valid_blocks, &join_key, &join_key, JoinType::Inner.into())
            .sort(["hours"], SortMultipleOptions::default()))
    } else {
        Ok(data)
    }
}

pub fn valid_blocks(metadata: &Metadata) -> PolarsResult<LazyFrame> {
    let entries = metadata
        .binary_mapping
        .iter()
        .map(|(binary, idx)| {
            let blocks = metadata.block_maps[*idx]
                .blocks()
                .map(|x| x.start)
                .collect::<Series>();
            df! { "block" => blocks }
                .unwrap()
                .lazy()
                .with_column(lit(binary.as_str()).alias("binary"))
        })
        .collect::<Vec<_>>();
    concat(entries, UnionArgs::default())
}

fn millis_to_hours(time_ms: Expr) -> Expr {
    (time_ms / lit(1000.0 * 60.0 * 60.0)).alias("hours")
}

fn secs_to_hours(time_secs: Expr) -> Expr {
    (time_secs / lit(60.0 * 60.0)).alias("hours")
}

/// Parse a u64 with either no prefix (decimal), '0x' prefix (hex), or '0b' (binary)
pub fn parse_u64_with_prefix(value: &str) -> Result<u64, std::num::ParseIntError> {
    if value.len() < 2 {
        return value.parse();
    }

    let (value, radix) = match &value[0..2] {
        "0x" => (&value[2..], 16),
        "0b" => (&value[2..], 2),
        _ => (value, 10),
    };
    u64::from_str_radix(value, radix)
}

pub fn polars_parse_u64(expr: Expr) -> Expr {
    let parse = |input: Series| -> PolarsResult<Series> {
        input
            .str()?
            .into_no_null_iter()
            .map(parse_u64_with_prefix)
            .collect::<Result<Series, _>>()
            .map_err(polars::error::to_compute_err)
    };
    expr.map(move |x| Ok(Some(parse(x)?)), GetOutput::default())
}

pub fn polars_format_u64(expr: Expr) -> Expr {
    let format = |input: Series| -> PolarsResult<Series> {
        let values = input
            .u64()?
            .into_iter()
            .map(|entry| entry.map(|x| format!("{x:#x}")));
        Ok(ChunkedArray::<StringType>::from_iter_options(input.name(), values).into_series())
    };
    expr.map(move |x| Ok(Some(format(x)?)), GetOutput::default())
}

pub fn load_glob(
    glob: &str,
    mut load: impl FnMut(&Path) -> anyhow::Result<LazyFrame>,
    mut filter: impl FnMut(&Path) -> bool,
) -> anyhow::Result<Vec<LazyFrame>> {
    let files = glob::glob(glob)
        .unwrap()
        .filter(|p| p.as_ref().ok().map_or(true, |path| filter(path)))
        .collect::<Result<Vec<_>, glob::GlobError>>()
        .with_context(|| format!("Error parsing glob: {glob}"))?;
    files
        .iter()
        .map(|p| load(p).with_context(|| format!("error loading: {}", p.display())))
        .collect::<anyhow::Result<Vec<_>>>()
}

struct LazyJsonReader(pub std::path::PathBuf);

impl AnonymousScan for LazyJsonReader {
    fn scan(&self, scan_opts: AnonymousScanArgs) -> PolarsResult<DataFrame> {
        let path = self.0.as_path();
        let reader = std::io::BufReader::new(std::fs::File::open(path).map_err(|e| {
            polars::error::to_compute_err(format!("{e}: failed to read {}", path.display()))
        })?);
        JsonReader::new(reader)
            .with_schema(scan_opts.schema)
            .finish()
    }

    fn schema(&self, _infer_schema_length: Option<usize>) -> PolarsResult<Arc<Schema>> {
        polars_bail!(ComputeError: "schema must be provided for JSON file");
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
