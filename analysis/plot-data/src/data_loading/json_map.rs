use std::{collections::HashMap, path::Path};

#[derive(Copy, Clone, Debug)]
pub struct HailFuzzCoverage {
    pub addr: u64,
    pub time_ms: u64,
    pub input_id: u64,
}

impl<'de> serde::Deserialize<'de> for HailFuzzCoverage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum CoverageEntry {
            V1((u64, u64, u64)),
            V0((u64, u64)),
        }
        match serde::Deserialize::deserialize(deserializer)? {
            CoverageEntry::V0((addr, time_ms)) => Ok(Self { addr, time_ms, input_id: 0 }),
            CoverageEntry::V1((addr, time_ms, input_id)) => Ok(Self { addr, time_ms, input_id }),
        }
    }
}

pub fn load_coverage_data(
    path: &Path,
    legacy: bool,
) -> anyhow::Result<HashMap<String, Vec<HailFuzzCoverage>>> {
    // Handle paths that point to the hail-fuzz directory compared to those that point to an
    // individual coverage file.
    let input = match path.is_dir() {
        true => super::open_buffered_file(&path.join("coverage.json"))?,
        false => super::open_buffered_file(path)?,
    };

    // Handle the case where the input is merged and unmerged.
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum HailFuzzCoverageData {
        Single(Vec<HailFuzzCoverage>),
        Merged(HashMap<String, Vec<HailFuzzCoverage>>),
    }

    let data: HailFuzzCoverageData = serde_json::from_reader(input)?;
    Ok(match data {
        HailFuzzCoverageData::Single(data) => {
            let tags = super::bench_tags_from_hail_fuzz_path(Some(path), legacy);
            [(tags, data)].into_iter().collect()
        }
        HailFuzzCoverageData::Merged(data) => data,
    })
}
