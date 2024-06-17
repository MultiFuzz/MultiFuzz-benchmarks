use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use indexmap::IndexMap;

use crate::{analysis::SurvivalRegion, metadata::MetadataSource};

pub(crate) fn parse_duration_str(name: &str) -> Option<Duration> {
    if let Some(hours) = name
        .strip_suffix("hours")
        .or_else(|| name.strip_suffix("hour"))
        .or_else(|| name.strip_suffix("hrs"))
        .or_else(|| name.strip_suffix("hr"))
        .or_else(|| name.strip_suffix("h"))
    {
        return Some(Duration::from_secs_f64(hours.parse::<f64>().ok()? * 60.0 * 60.0));
    }
    else if let Some(mins) = name
        .strip_suffix("minutes")
        .or_else(|| name.strip_suffix("minute"))
        .or_else(|| name.strip_suffix("mins"))
        .or_else(|| name.strip_suffix("min"))
        .or_else(|| name.strip_suffix("m"))
    {
        return Some(Duration::from_secs_f64(mins.parse::<f64>().ok()? * 60.0));
    }
    else if let Some(seconds) = name
        .strip_suffix("seconds")
        .or_else(|| name.strip_suffix("second"))
        .or_else(|| name.strip_suffix("secs"))
        .or_else(|| name.strip_suffix("sec"))
        .or_else(|| name.strip_suffix("s"))
    {
        return Some(Duration::from_secs_f64(seconds.parse::<f64>().ok()?));
    }
    None
}
fn parse_duration<'de, D>(deserializer: D) -> std::result::Result<Duration, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(untagged)]
    enum U64OrString {
        U64(u64),
        String(String),
    }
    let value: U64OrString = serde::Deserialize::deserialize(deserializer)?;
    match value {
        U64OrString::U64(v) => Ok(Duration::from_millis(v)),
        U64OrString::String(v) => parse_duration_str(&v)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid time format: {v}"))),
    }
}

fn one_day() -> Duration {
    Duration::from_secs(60 * 60 * 24)
}

#[derive(Clone, serde::Deserialize)]
pub enum DataSource {
    EmberCsv {
        glob: String,
        #[serde(deserialize_with = "parse_duration", default = "one_day")]
        duration: Duration,
        #[serde(default)]
        resampled: bool,
    },
    FuzzwareBlocksCsv {
        glob: String,
        #[serde(deserialize_with = "parse_duration", default = "one_day")]
        duration: Duration,
    },
    MultiFuzzBench {
        glob: String,
        #[serde(deserialize_with = "parse_duration", default = "one_day")]
        duration: Duration,
    },
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
pub enum VecOrOne<T> {
    Vec(Vec<T>),
    One(T),
}

impl<T> From<VecOrOne<T>> for Vec<T> {
    fn from(value: VecOrOne<T>) -> Self {
        match value {
            VecOrOne::Vec(v) => v,
            VecOrOne::One(o) => vec![o],
        }
    }
}

#[derive(Default, Clone, serde::Deserialize)]
pub enum FilterExpr {
    Col(String),
    Str(String),
    U32(u32),
    U64(u64),
    Eq(Box<FilterExpr>, Box<FilterExpr>),
    Neq(Box<FilterExpr>, Box<FilterExpr>),
    And(Vec<FilterExpr>),
    Or(Vec<FilterExpr>),
    Not(Box<FilterExpr>),
    #[default]
    True,
}

#[derive(Clone, serde::Deserialize)]
pub struct Dataset {
    pub source: DataSource,
    #[serde(default)]
    pub filter: FilterExpr,
}

#[derive(Clone, Default, serde::Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl From<Size> for (u32, u32) {
    fn from(value: Size) -> Self {
        (value.width, value.height)
    }
}

fn default_cell_size() -> Size {
    Size { width: 160, height: 180 }
}

#[derive(Clone, serde::Deserialize)]
pub struct PlotLayout {
    #[serde(default = "default_cell_size")]
    pub cell_size: Size,
    #[serde(default)]
    pub max_columns: u32,
    #[serde(default)]
    pub min_size: Size,
}

impl PlotLayout {
    pub fn get_layout(&self, n_binaries: u32) -> (u32, Size) {
        let n_col = match self.max_columns {
            0 => n_binaries,
            max => u32::min(n_binaries, max),
        };
        let n_rows = 1 + (n_binaries - 1) / n_col;
        let dims = Size {
            width: u32::max(55 + self.cell_size.width * n_col, self.min_size.width),
            height: u32::max(55 + self.cell_size.height * n_rows, self.min_size.height),
        };
        (n_col, dims)
    }
}

impl Default for PlotLayout {
    fn default() -> Self {
        Self {
            cell_size: Size { width: 160, height: 180 },
            max_columns: 5,
            min_size: Size { width: 800, height: 500 },
        }
    }
}

#[derive(Clone, serde::Deserialize)]
pub struct Diff {
    pub fuzzer_a: String,
    pub fuzzer_b: String,
}

#[derive(Clone, serde::Deserialize)]
pub struct Config {
    #[serde(default)]
    pub path: PathBuf,
    #[serde(default)]
    pub plot_layout: PlotLayout,
    #[serde(default)]
    pub survival_layout: PlotLayout,
    #[serde(default)]
    pub survival_hide_rect: bool,
    #[serde(default)]
    pub filter: FilterExpr,
    #[serde(default)]
    pub coverage_metadata: Option<MetadataSource>,
    #[serde(default)]
    pub data: IndexMap<String, Vec<Dataset>>,
    pub time_resolution: u64,
    pub trials: u32,
    #[serde(default)]
    pub survival: IndexMap<String, SurvivalRegion>,
    #[serde(default)]
    pub survival_plot_max_hours: f32,
    #[serde(default)]
    pub diff: Option<Diff>,
    pub reference: String,
    #[serde(default)]
    pub legend_mapping: HashMap<String, usize>,
    /// List of binaries to mark as gray because they contain bug-exploits.
    #[serde(default)]
    pub bug_exploit: Vec<String>,
}

impl Config {
    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        let parse = || -> anyhow::Result<Self> { Ok(ron::de::from_bytes(&std::fs::read(path)?)?) };
        let mut data = parse().with_context(|| format!("error parsing: {}", path.display()))?;
        data.path = path.to_owned();
        Ok(data)
    }

    pub fn datasets(&self) -> impl Iterator<Item = (usize, &String, &Dataset)> {
        self.data
            .iter()
            .enumerate()
            .flat_map(|(id, (name, sources))| sources.iter().map(move |x| (id, name, x)))
    }

    pub fn has_bug_exploit(&self, name: &str) -> bool {
        self.bug_exploit.iter().any(|x| x == name)
    }
}
