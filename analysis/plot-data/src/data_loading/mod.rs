use std::path::Path;

use anyhow::Context;

pub mod json_map;

/// Generates benchmark tags from a path (if possible).
pub fn bench_tags_from_hail_fuzz_path(source_path: Option<&Path>, legacy: bool) -> String {
    const UNKNOWN_TAG: &str = "v1;bench=unknown;fuzzer=unknown;binary=unknown;trial=0";

    let Some(mut path) = source_path
    else {
        return UNKNOWN_TAG.to_string();
    };

    // Check if the path points to the main directory or a file in the directory.
    if path.is_file() {
        path = match path.parent() {
            Some(dir) => dir,
            None => return UNKNOWN_TAG.to_string(),
        };
    }

    let components = path.components().rev().flat_map(|x| x.as_os_str().to_str());

    if legacy {
        if let Some((bench, fuzzer, binary, trial)) = tags_from_legacy_path(components.clone()) {
            return format!("v1;bench={bench};fuzzer={fuzzer};binary={binary};trial={trial}");
        }
    }
    else {
        if let Some((bench, fuzzer, _, binary, trial)) = tags_from_path(components.clone()) {
            return format!("v1;bench={bench};fuzzer={fuzzer};binary={binary};trial={trial}");
        }
    }

    UNKNOWN_TAG.to_string()
}

/// Extract tags from the old bench-harness directory format, i.e.:
/// `[bench]/[fuzzer]-[binary]/[trial]`
fn tags_from_legacy_path<'a>(
    mut components: impl Iterator<Item = &'a str>,
) -> Option<(&'a str, &'a str, &'a str, &'a str)> {
    let (Some(trial), Some(target), Some(bench)) =
        (components.next(), components.next(), components.next())
    else {
        return None;
    };
    let (fuzzer, binary) = target.rsplit_once("-")?;
    Some((bench, fuzzer, binary, trial))
}
/// Extract tags new bench-harness directory format, i.e.:
/// `[bench]/[fuzzer]/[group]/[binary]/[trial]`
fn tags_from_path<'a>(
    mut c: impl Iterator<Item = &'a str>,
) -> Option<(&'a str, &'a str, &'a str, &'a str, &'a str)> {
    match (c.next(), c.next(), c.next(), c.next(), c.next()) {
        (Some(trial), Some(binary), Some(group), Some(fuzzer), Some(bench)) => {
            Some((bench, fuzzer, group, binary, trial))
        }
        _ => None,
    }
}

/// Parses tags in the format: `v1;<key>=<value>;...`
pub fn parse_bench_tags(tag: &str) -> anyhow::Result<impl Iterator<Item = (&str, &str)>> {
    let mut iter = tag.split(';');
    match iter.next() {
        Some(tag) if tag == "v1" => {}
        Some(tag) => anyhow::bail!("Unknown tag version ({tag})"),
        None => anyhow::bail!("Unknown tag version (None)"),
    }
    Ok(iter.filter(|x| !x.is_empty()).filter_map(|x| x.split_once("=")))
}

fn open_buffered_file(path: impl AsRef<Path>) -> anyhow::Result<std::io::BufReader<std::fs::File>> {
    let path = path.as_ref();
    Ok(std::io::BufReader::new(
        std::fs::File::open(&path)
            .with_context(|| format!("failed to open: {}", path.display()))?,
    ))
}
