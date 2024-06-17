use std::{
    io::BufRead,
    path::{Path, PathBuf},
};

use anyhow::{ensure, Context};
use polars::prelude::*;

use crate::{data_loading, load_glob, parse_u64_with_prefix, polars_parse_u64};

/// Load data from the "live" coverage file that is generated while MultiFuzz is running. Useful for
/// checking incomplete or unprocessed runs, however may contain inaccurate coverage information
/// (e.g. from crashing inputs).
pub fn read_raw_coverage_csv_all(glob: &str) -> anyhow::Result<LazyFrame> {
    let ignore_resampled_csv = |x: &Path| {
        x.file_name().and_then(|x| x.to_str()).map_or(true, |x| !x.contains("resampled"))
    };
    let data = load_glob(glob, read_raw_v1_csv, ignore_resampled_csv)?;
    ensure!(!data.is_empty(), "No raw MultiFuzz csv files found for: {glob}");
    Ok(concat(data, UnionArgs::default())?)
}

fn read_raw_v1_csv(path: &Path) -> anyhow::Result<LazyFrame> {
    let mut schema = Schema::new();
    schema.with_column("block".into(), DataType::String);
    schema.with_column("time".into(), DataType::Int64);
    schema.with_column("inputs".into(), DataType::UInt32);

    // The input file contain multiple entries merged together with tag values in between so we read
    // the file line-by-line searching for tagged chunks.
    let mut buf = vec![];
    let mut current_tag: Option<String> = None;
    let mut reader = std::io::BufReader::new(
        std::fs::File::open(path).with_context(|| format!("failed to read: {}", path.display()))?,
    );

    let read_csv_chunk = |buf: &[u8], tags: &str| -> anyhow::Result<LazyFrame> {
        let df = CsvReadOptions::default()
            .with_schema(Some(schema.clone().into()))
            .into_reader_with_file_handle(std::io::Cursor::new(buf))
            .finish()
            .with_context(|| {
                format!("failed to read csv chunk for: {tags} ({} bytes)", buf.len())
            })?;

        let mut lf = df.lazy().with_column(polars_parse_u64(col("block")));
        for (key, value) in data_loading::parse_bench_tags(tags)? {
            lf = lf.with_column(lit(value).alias(key));
        }

        Ok(lf)
    };

    // Discard the header (we manually specify fields in the schema above).
    let _ = reader.read_until(b'\n', &mut buf)?;
    buf.clear();

    // Iterate over and parse each tagged chuck as a new data frame.
    let mut offset = 0;
    let mut chunks = vec![];
    loop {
        let n = reader.read_until(b'\n', &mut buf)?;
        if n == 0 {
            break;
        }

        // Check if we found a new tagged chunk.
        if buf[offset..].starts_with(b"###") {
            if let Some(tag) = current_tag.as_ref() {
                chunks.push(read_csv_chunk(&buf[..offset], tag)?);
            }
            current_tag = Some(std::str::from_utf8(&buf[offset + 3..])?.trim().to_owned());
            buf.clear();
            offset = 0;
        }
        else {
            offset += n;
        }
    }

    if chunks.is_empty() {
        anyhow::bail!("No tagged chunks were found in file: {}", path.display());
    }

    // Merge all the chunks together.
    Ok(concat(chunks, UnionArgs::default())?.with_columns(add_metadata()))
}

pub fn read_all(glob: &String) -> anyhow::Result<Option<LazyFrame>> {
    let data = load_glob(glob, |path| Ok(read_coverage_json(path)?), |_| true)?;
    if data.is_empty() {
        eprintln!("WARNING: No raw MultiFuzz json files found for: {glob}");
        return Ok(None);
    }
    Ok(Some(concat(data, UnionArgs::default())?))
}

pub fn read_coverage_json(path: &Path) -> PolarsResult<LazyFrame> {
    let args = ScanArgsAnonymous { name: "scan_coverage_json", ..ScanArgsAnonymous::default() };
    LazyFrame::anonymous_scan(Arc::new(LazyCoverageJson(path.into())), args)
}

struct LazyCoverageJson(PathBuf);

impl AnonymousScan for LazyCoverageJson {
    fn scan(&self, scan_opts: AnonymousScanArgs) -> PolarsResult<DataFrame> {
        let path = self.0.as_path();
        let mut entries = vec![];
        for (tags, data) in data_loading::json_map::load_coverage_data(path, false)
            .map_err(polars::error::to_compute_err)?
        {
            let mut lf = df! {
                "block" => data.iter().map(|x| x.addr).collect::<Series>(),
                "time" => data.iter().map(|x| x.time_ms as i64).collect::<Series>(),
                "input" => data.iter().map(|x| x.input_id).collect::<Series>(),
            }?
            .lazy();
            let schema: &Schema = scan_opts.schema.as_ref();
            for (key, mut value) in
                data_loading::parse_bench_tags(&tags).map_err(polars::error::to_compute_err)?
            {
                if let Some(dtype) = schema.get(key) {
                    if key == "binary" {
                        value = normalize_binary_name(value);
                    }
                    lf = lf.with_column(lit(value).alias(key).cast(dtype.clone()));
                }
            }
            entries.push(lf);
        }
        concat(entries, UnionArgs::default())?.with_columns(add_metadata()).collect()
    }

    fn schema(&self, _infer_schema_length: Option<usize>) -> PolarsResult<Arc<Schema>> {
        let mut schema = Schema::new();
        schema.with_column("block".into(), DataType::String);
        schema.with_column("time".into(), DataType::Int64);
        schema.with_column("input".into(), DataType::UInt64);
        schema.with_column("trial".into(), DataType::String);
        schema.with_column("binary".into(), DataType::String);
        schema.with_column("fuzzer".into(), DataType::String);
        Ok(Arc::new(schema))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub fn read_testcases_json(glob: &str) -> anyhow::Result<LazyFrame> {
    // @todo: this schema is incomplete.
    let mut schema = Schema::new();
    schema.with_column("id".into(), DataType::UInt32);
    schema.with_column("len".into(), DataType::UInt32);
    schema.with_column("untrimed_len".into(), DataType::UInt32);
    let schema = Arc::new(schema);

    let data = load_glob(glob, |path| read_trial_json(path, schema.clone()), |_| true)?;
    ensure!(!data.is_empty(), "No files found for: {glob}");
    Ok(concat_lf_diagonal(data, UnionArgs::default())?)
}

pub fn read_trial_json(path: &Path, schema: Arc<Schema>) -> anyhow::Result<LazyFrame> {
    // Parse target, binary name and file from path: e.g.
    // `[bench]/[target]-[binary]/[trial]/file.json`
    fn extract_metadata_from_path(path: &Path) -> Option<(&str, &str, u32)> {
        let mut components = path.components().rev();
        let mut next = || components.next().and_then(|x| x.as_os_str().to_str());

        let (Some(_file), Some(trial), Some(target_and_binary), Some(bench)) =
            (next(), next(), next(), next())
        else {
            return None;
        };

        let trial = parse_u64_with_prefix(trial).ok()? as u32;
        let (_target, binary) = target_and_binary.rsplit_once('-')?;

        Some((bench, binary, trial))
    }
    let (bench, binary, trial) = extract_metadata_from_path(path).ok_or_else(|| {
        anyhow::format_err!("failed to read metadata from path: {}", path.display())
    })?;

    let binary = normalize_binary_name(binary);

    let args = ScanArgsAnonymous {
        schema: Some(schema),
        name: "scan_json",
        ..ScanArgsAnonymous::default()
    };
    Ok(LazyFrame::anonymous_scan(Arc::new(crate::LazyJsonReader(path.into())), args)?.with_columns(
        [lit(bench).alias("bench"), lit(trial).alias("trial"), lit(binary).alias("binary")],
    ))
}

fn add_metadata() -> [Expr; 2] {
    [
        col("trial").str().to_integer(lit(10), false).cast(DataType::UInt32),
        (col("binary").str().replace(lit(".*/"), lit(""), false)),
    ]
}

pub mod legacy {
    use super::*;

    const RESAMPLED_CSV_GLOB: &str = "../results/multi-stream/*.resampled.csv";
    pub fn read_coverage_csv_all() -> anyhow::Result<LazyFrame> {
        let data = load_glob(
            RESAMPLED_CSV_GLOB,
            |p| read_resampled_v1_csv(p, &["bench", "fuzzer", "binary", "trial", "mode"]),
            |_| true,
        )?;
        ensure!(!data.is_empty(), "No data found for: {RESAMPLED_CSV_GLOB}");
        Ok(concat(data, UnionArgs::default())?)
    }

    fn read_resampled_v1_csv(path: &Path, keys: &[impl AsRef<str>]) -> anyhow::Result<LazyFrame> {
        let mut schema = Schema::new();
        schema.with_column("tag".into(), DataType::String);
        schema.with_column("time".into(), DataType::Int64);
        schema.with_column("count".into(), DataType::UInt32);

        let mut lf = LazyCsvReader::new(path)
            .with_has_header(true)
            .with_schema(Some(schema.into()))
            .finish()?;

        // Check the first tag to determine how we are going to parse tags for this binary
        let tags = &lf.clone().first().collect()?["tag"];
        let first_tag = tags.str_value(0)?;
        if !first_tag.starts_with("v1;") {
            anyhow::bail!("Unsupported tag format: {first_tag}");
        }

        // Extract keys from tag and add them as new columns.
        for key in keys {
            let key = key.as_ref();
            lf = lf
                .with_column(col("tag").str().extract(lit(format!("{key}=([^;]+)")), 1).alias(key))
        }

        Ok(lf.with_columns(add_metadata()))
    }
}

pub fn normalize_binary_name(name: &str) -> &str {
    match name {
        "6LoWPAN_Receiver" => "6LoWPAN_Sender",
        "3Dprinter" => "uEmu.3Dprinter",
        "GPSTracker" => "uEmu.GPSTracker",
        _ => name,
    }
}
