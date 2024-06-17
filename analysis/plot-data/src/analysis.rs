use indexmap::IndexMap;
use polars::prelude::*;

use crate::{config::Config, map_binary_names, order_by_binary, Coverage};

/// Generate a Fuzzware-style coverage table showing min/max/median/total blocks reached by a fuzzer
/// over all trials.
pub fn coverage_table(config: &Config) -> anyhow::Result<LazyFrame> {
    // Call collect here to avoid crash caused by: https://github.com/pola-rs/polars/issues/5490
    let coverage = crate::load_raw_coverage(config)?.collect()?.lazy();

    let total_unique_blocks = coverage
        .clone()
        .group_by(["dataset", "fuzzer", "binary"])
        .agg([col("block").n_unique().alias("bb_total")]);

    let total_blocks_per_trial = coverage
        .group_by(["dataset", "fuzzer", "binary", "trial"])
        .agg([col("block").count().alias("total_blocks")]);

    let join_key = [col("dataset"), col("fuzzer"), col("binary")];
    let summary = total_blocks_per_trial
        .group_by(["dataset", "fuzzer", "binary"])
        .agg([
            min("total_blocks").alias("bb_min"),
            median("total_blocks").alias("bb_avg"),
            max("total_blocks").alias("bb_max"),
        ])
        .join(total_unique_blocks, &join_key, &join_key, JoinType::Inner.into())
        .sort_by_exprs(
            &join_key,
            SortMultipleOptions::new().with_nulls_last(true).with_maintain_order(true),
        );

    Ok(summary)
}

pub fn load_preprocessed_coverage_table(config: &Config) -> anyhow::Result<LazyFrame> {
    let coverage = crate::load_block_hits(config)?;

    let total_blocks_per_trial = coverage
        .group_by(["dataset", "fuzzer", "binary", "trial"])
        .agg([col("blocks").max().alias("total_blocks")]);

    let sort_key = [col("dataset"), col("fuzzer"), col("binary")];
    let summary = total_blocks_per_trial
        .group_by(["dataset", "fuzzer", "binary"])
        .agg([
            min("total_blocks").alias("bb_min"),
            median("total_blocks").alias("bb_avg"),
            max("total_blocks").alias("bb_max"),
        ])
        .sort_by_exprs(
            &sort_key,
            SortMultipleOptions::new().with_nulls_last(true).with_maintain_order(true),
        );

    Ok(summary)
}

pub fn median_coverage(config: &Config) -> anyhow::Result<DataFrame> {
    let coverage = load_preprocessed_coverage_table(config)?.cache();
    let reference = coverage
        .clone()
        .filter(col("fuzzer").eq(lit(config.reference.as_str())))
        .select([col("binary"), col("bb_avg").alias("reference_avg")]);

    let with_reference = coverage
        .join(reference, &[col("binary")], &[col("binary")], JoinType::Left.into())
        .with_columns([((col("bb_avg") / col("reference_avg")) * lit(100.0_f64)).alias("% ref")])
        .sort_by_exprs(
            [order_by_binary(), col("dataset")],
            SortMultipleOptions::new().with_nulls_last(true).with_maintain_order(true),
        )
        .with_column(map_binary_names(col("binary")));

    const FORMAT_REFERNCE: bool = false;
    if FORMAT_REFERNCE {
        let df = with_reference
            .with_column(
                when(col("fuzzer").neq(lit(config.reference.as_str())))
                    .then(format_str("{}\t({}%)", [
                        col("bb_avg").cast(DataType::UInt32),
                        col("% ref").round(1),
                    ])?)
                    .otherwise(format_str("{}", [col("bb_avg").cast(DataType::UInt32)])?)
                    .alias("val"),
            )
            .collect()?;
        Ok(pivot::pivot_stable(&df, ["binary"], ["fuzzer"], Some(["val"]), false, None, None)?)
    }
    else {
        let summary =
            with_reference.clone().group_by(["fuzzer"]).agg([median("% ref")]).collect()?;
        eprintln!("{summary:?}");

        let df = with_reference
            .with_columns([
                col("bb_avg").cast(DataType::UInt32),
                // format_str("({}%)", [col("% ref").round(1)])?.alias("% ref"),
            ])
            .collect()?;

        let mut data = pivot::pivot_stable(
            &df,
            ["binary"],
            ["fuzzer"],
            Some(["bb_avg", "% ref"]),
            false,
            None,
            None,
        )?;
        data.sort_in_place(
            ["% ref_fuzzer_MultiFuzz"],
            SortMultipleOptions::new().with_order_descending(true).with_maintain_order(true),
        )?;

        let cols = data.get_column_names();
        let sorted_cols = sort_columns_by_element(&cols);

        Ok(data.select(&sorted_cols)?)
    }
}

fn sort_columns_by_element<'a>(cols: &[&'a str]) -> Vec<&'a str> {
    let mut sorted_cols = vec![];
    for (i, &col) in cols.iter().enumerate() {
        if let Some(fuzzer) = col.strip_prefix("bb_avg") {
            sorted_cols.push(col);
            if let Some(percent) = cols[i + 1..].iter().find(|x| x.ends_with(fuzzer)) {
                sorted_cols.push(percent);
            }
        }
        else if col.starts_with("% ref") {
            // Already inserted.
        }
        else {
            sorted_cols.push(col);
        }
    }
    sorted_cols
}

pub type UniqueBlocks = LazyFrame;

pub fn unique_blocks_per_fuzzer(config: &Config) -> anyhow::Result<UniqueBlocks> {
    let coverage = crate::load_raw_coverage(config)?;

    // Keep track of all the fuzzers that found each block
    let blocks_found = coverage
        .group_by(["binary", "block", "fuzzer"])
        .agg([])
        .with_column(col("fuzzer").implode().over(["binary", "block"]).alias("fuzzers"));

    // Count the number of blocks that only a single fuzzer found.
    let found_by_one_fuzzer = col("fuzzers").list().len().eq(lit(1));
    let unique_blocks_per_fuzzer = blocks_found
        .group_by(["binary", "fuzzer"])
        .agg([(col("block").filter(found_by_one_fuzzer)).count().alias("unique_blocks")])
        .sort_by_exprs(
            [order_by_binary(), col("fuzzer")],
            SortMultipleOptions::new().with_nulls_last(true).with_maintain_order(true),
        );

    Ok(unique_blocks_per_fuzzer)
}

/// Represents a lazy frame generated by `block_diff`
pub type BlockDiff = LazyFrame;

pub fn block_diff(config: &Config, fuzzer_a: &str, fuzzer_b: &str) -> anyhow::Result<BlockDiff> {
    let coverage = crate::load_raw_coverage(config)?;

    let block_first_found =
        coverage.group_by(["binary", "block", "fuzzer"]).agg([col("hours").max()]).cache();

    // Compute the fastest time each of the fuzzers found the target block
    let fuzzer_a_found = block_first_found
        .clone()
        .filter(col("fuzzer").eq(lit(fuzzer_a)))
        .drop(["fuzzer"])
        .rename(["hours"], [fuzzer_a]);
    let fuzzer_b_found = block_first_found
        .filter(col("fuzzer").eq(lit(fuzzer_b)))
        .drop(["fuzzer"])
        .rename(["hours"], [fuzzer_b]);

    // Find the difference between the time it took to find each block in each fuzer.
    let join_key = [col("binary"), col("block")];
    let difference = fuzzer_a_found
        .join_builder()
        .with(fuzzer_b_found)
        .left_on(join_key.clone())
        .right_on(join_key)
        .how(JoinType::Outer)
        .coalesce(JoinCoalesce::CoalesceColumns)
        .finish()
        .with_column((col(fuzzer_a) - col(fuzzer_b)).alias("diff"));

    Ok(difference.sort_by_exprs(
        [order_by_binary(), col("diff"), col("block")],
        SortMultipleOptions::new()
            .with_order_descendings([false, true, false])
            .with_maintain_order(true)
            .with_nulls_last(true),
    ))
}

/// Represents a lazy frame generated by `blocks_hit_per_period`
pub type BlockHits = LazyFrame;

pub fn blocks_hit_per_period(
    coverage: Coverage,
    duration: i64,
    resolution: i64,
    index: &'static str,
    by: impl AsRef<[Expr]>,
) -> anyhow::Result<BlockHits> {
    cumulative_count_by_period(coverage, duration, resolution, index, "block", by, "blocks")
}

pub fn cumulative_count_by_period(
    df: Coverage,
    duration: i64,
    resolution: i64,
    index: &'static str,
    agg: &'static str,
    by: impl AsRef<[Expr]>,
    alias: &'static str,
) -> anyhow::Result<BlockHits> {
    let by = by.as_ref();
    // Count the total number of occurances found in a particular time period, then compute the
    // cumulative sum of the count.
    let period = Duration::new(duration / resolution);
    let bucket_counts = df
        .group_by_dynamic(col(index), by, DynamicGroupOptions {
            index_column: index.into(),
            every: period,
            period,
            offset: Duration::new(0),
            label: Label::DataPoint,
            include_boundaries: false,
            closed_window: ClosedWindow::Left,
            start_by: StartBy::WindowBound,
            check_sorted: false,
        })
        .agg([col(agg).count().alias("agg_count")])
        .with_column(col("agg_count").cum_sum(false).over(by).alias(alias))
        .drop(["agg_count"]);
    fill_missing(bucket_counts, duration, resolution, index, by)
}

/// Perform an asof join with a dataframe containing every period, filling empty periods within each
/// subgroup with the last seen value.
pub fn fill_missing(
    hits: LazyFrame,
    duration: i64,
    resolution: i64,
    index: &'static str,
    by: impl AsRef<[Expr]>,
) -> anyhow::Result<LazyFrame> {
    let periods = df! {
        index => {
            let mut i = (0..duration).step_by(duration as usize / resolution as usize).collect::<Series>();
            i.set_sorted_flag(polars::series::IsSorted::Ascending);
            i
        }
    }?;

    let schema = hits.schema()?;
    Ok(hits
        .group_by_stable(by)
        .apply(
            move |mut df| {
                df.sort_in_place([index], SortMultipleOptions::new().with_maintain_order(true))?;
                periods.join_asof(&df, index, index, AsofStrategy::Backward, None, None)
            },
            schema,
        )
        .drop_nulls(None))
}

pub fn dynamic_fill_missing(
    hits: LazyFrame,
    step: usize,
    index: &'static str,
    by: impl AsRef<[Expr]>,
) -> anyhow::Result<LazyFrame> {
    let schema = hits.schema()?;
    Ok(hits
        .group_by_stable(by)
        .apply(
            move |mut df| {
                let max = df[index].i64().unwrap().max().unwrap();
                let periods = df! {
                    index => {
                        let mut i = (0..max + step as i64).step_by(step).collect::<Series>();
                        i.set_sorted_flag(polars::series::IsSorted::Ascending);
                        i
                    }
                }?;
                df.sort_in_place([index], SortMultipleOptions::new().with_maintain_order(true))?;
                periods.join_asof(&df, index, index, AsofStrategy::Backward, None, None)
            },
            schema,
        )
        .drop_nulls(None))
}

pub fn raw_blocks_hit(coverage: Coverage) -> BlockHits {
    coverage
        .group_by([col("fuzzer"), col("binary"), col("trial"), col("hours")])
        .agg([col("block").count().alias("new_blocks")])
        .sort(["hours"], Default::default())
        .with_column(
            col("new_blocks").cum_sum(false).over(["fuzzer", "binary", "trial"]).alias("blocks"),
        )
        .drop(["new_blocks"])
}

pub fn get_average_input_sizes(testcases: LazyFrame) -> LazyFrame {
    // If untrimed_len is zero, then the input was not trimmed, so correct the untrimed value here.
    let update_untrimmed = when(col("untrimed_len").eq(0))
        .then(col("len"))
        .otherwise(col("untrimed_len"))
        .alias("untrimed_len");

    // We could also remove the untrimmed inputs.
    // let remove_untrimmed = filter(col("untrimed_len").neq(0))

    testcases
        .with_column(update_untrimmed)
        .group_by(["binary", "trial"])
        .agg([mean("len"), mean("untrimed_len")])
}

#[derive(Clone, serde::Deserialize)]
pub struct SurvivalRegion {
    pub binary: String,
    pub start: u64,
    pub end: u64,
}

pub fn block_survival(
    coverage: LazyFrame,
    regions: &IndexMap<String, SurvivalRegion>,
) -> Result<LazyFrame, PolarsError> {
    fn at_first_hit(value: Expr, block: u64) -> Expr {
        value.filter(col("block").eq(lit(block))).first()
    }

    // Keep track of total unique blocks hit at each time step. Note: we force a collect here
    // because otherwise polars seems to be really slow.
    let data = coverage
        .sort(["hours"], Default::default())
        .with_column(
            col("block").cum_count(false).over(["fuzzer", "binary", "trial"]).alias("blocks"),
        )
        .collect()?
        .lazy();

    let mut result = vec![];
    for (i, (label, region)) in regions.into_iter().enumerate() {
        let entry = data
            .clone()
            .filter(col("binary").eq(lit(region.binary.as_str())))
            .group_by(["fuzzer", "binary", "trial"])
            .agg([
                at_first_hit(col("hours"), region.start).alias("start_time"),
                at_first_hit(col("blocks"), region.start).alias("start_blocks"),
                at_first_hit(col("hours"), region.end).alias("end_time"),
                at_first_hit(col("blocks"), region.end).alias("end_blocks"),
            ])
            .with_column((col("end_time") - col("start_time")).alias("duration"))
            .sort(["duration"], SortMultipleOptions::new().with_nulls_last(true))
            .with_columns([
                col("duration").cum_count(false).over(["fuzzer", "binary"]).alias("count"),
                lit(label.as_str()).alias("label"),
                lit(i as u32).alias("index"),
            ]);
        result.push(entry);
    }

    Ok(concat(result, UnionArgs::default())?.sort_by_exprs(
        [col("index"), col("fuzzer"), col("count")],
        SortMultipleOptions::new().with_nulls_last(false).with_maintain_order(true),
    ))
}

pub fn summarize_coverage(block_hits: BlockHits) -> LazyFrame {
    block_hits
        .sort(["hours"], Default::default())
        .group_by_stable(["hours", "binary", "fuzzer", "dataset"])
        .agg([
            median("blocks").alias("blocks_median"),
            max("blocks").alias("blocks_max"),
            min("blocks").alias("blocks_min"),
        ])
        .sort_by_exprs(
            [order_by_binary(), col("dataset")],
            SortMultipleOptions::new().with_nulls_last(false).with_maintain_order(true),
        )
}

pub fn summarize_inspector(df: LazyFrame) -> LazyFrame {
    df.sort(["testcase"], Default::default())
        .group_by_stable(["testcase", "arch", "kind"])
        .agg([
            median("cov").alias("cov_median"),
            max("cov").alias("cov_max"),
            min("cov").alias("cov_min"),
        ])
        .sort_by_exprs(
            [col("arch")],
            SortMultipleOptions::new().with_nulls_last(false).with_maintain_order(true),
        )
}
