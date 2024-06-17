use indexmap::IndexMap;
use plot_data::Config;
use plotters::{
    coord::Shift,
    prelude::*,
    style::text_anchor::{HPos, Pos, VPos},
};
use polars::prelude::*;

use crate::utils::{draw_subtitle, draw_x_axis_label, split_with_columns, Legend, StepIter};

pub fn plot_survival<DB>(
    root: &DrawingArea<DB, Shift>,
    config: &Config,
    n_cols: usize,
    block_hits: LazyFrame,
    survival: LazyFrame,
) -> anyhow::Result<()>
where
    DB: DrawingBackend,
    DB::ErrorType: 'static,
{
    root.fill(&WHITE)?;

    // Add regions for combined axis.
    let (legend_area, x_axis_area, plot_area) = {
        let (plot_area, legend_area) = root.split_vertically(root.dim_in_pixel().1 - 30);
        let (plot_area, x_axis_area) = plot_area.split_vertically(plot_area.dim_in_pixel().1 - 25);
        (legend_area, x_axis_area, plot_area)
    };

    let legend_label_style = TextStyle::from(("Arial", 18).into_font())
        .with_anchor::<RGBAColor>(Pos::new(HPos::Left, VPos::Bottom))
        .into_text_style(&legend_area);
    let mut legend = Legend::new_with_mapping(legend_label_style, config.legend_mapping.clone());

    let axis_desc_style = TextStyle::from(("Arial", 16).into_font());
    let survival_by_label = survival.collect()?.partition_by_stable(["label"], true)?;

    let plot_regions = split_with_columns(&plot_area, survival_by_label.len(), n_cols);
    for (survival, region) in survival_by_label.iter().zip(plot_regions) {
        let (title, plot) = region.split_vertically(25);
        let (coverage_plot, survival_plot) = plot.split_horizontally(50.percent_width());

        let name = survival["binary"].str_value(0)?;
        let label = survival["label"].str_value(0)?;
        draw_subtitle(&label, &title, 0, 20)?;

        // let max_hours = survival["duration"].f64()?.max().unwrap();
        let max_hours = config.survival_plot_max_hours;
        let trials = config.trials;

        let mut survival_subchart = ChartBuilder::on(&survival_plot);
        let mut survival_ctx = survival_subchart
            .margin(4)
            .set_label_area_size(LabelAreaPosition::Bottom, 15)
            .set_label_area_size(LabelAreaPosition::Left, 30)
            .build_cartesian_2d(0_f32..max_hours, 0_f32..trials as f32)?;
        survival_ctx
            .configure_mesh()
            .max_light_lines(0)
            .x_label_formatter(&|value| format!("{}", ((value * 100.0).round() / 100.0)))
            .x_labels(6)
            .x_label_style(TextStyle::from(("Arial", 14).into_font()))
            .y_label_formatter(&|value| format!("{}", *value as u64))
            .y_labels(8)
            .y_label_style(TextStyle::from(("Arial", 14).into_font()))
            .y_desc("Remaining Trials")
            .axis_desc_style(axis_desc_style.clone())
            .draw()
            .unwrap();

        let coverage = block_hits.clone().filter(col("binary").eq(lit(name.as_ref()))).collect()?;
        let max_y = coverage["blocks"].u32()?.max().unwrap();
        let mut coverage_subchart = ChartBuilder::on(&coverage_plot);
        let mut coverage_ctx = coverage_subchart
            .margin(4)
            .set_label_area_size(LabelAreaPosition::Bottom, 15)
            .set_label_area_size(LabelAreaPosition::Left, 45)
            .build_cartesian_2d(0_f32..max_hours, 0_f32..max_y as f32)?;
        coverage_ctx
            .configure_mesh()
            .max_light_lines(0)
            .x_label_formatter(&|value| format!("{}", *value as u64))
            .x_labels(6)
            .x_label_style(TextStyle::from(("Arial", 14).into_font()))
            .y_label_formatter(&|value| format!("{}", *value as u64))
            .y_labels(8)
            .y_label_style(TextStyle::from(("Arial", 14).into_font()))
            .y_desc("#Blocks Hit")
            .axis_desc_style(axis_desc_style.clone())
            .draw()
            .unwrap();

        let survival_by_fuzzer = survival.partition_by_stable(["fuzzer"], true)?;
        let coverage_by_fuzzer = coverage.partition_by_stable(["fuzzer"], true)?;

        // Ensure that survival and coverage datasets are sorted by the same fuzzer.
        let survival_by_fuzzer: IndexMap<String, &DataFrame> = survival_by_fuzzer
            .iter()
            .map(|x| (x["fuzzer"].str_value(0).unwrap().to_string(), x))
            .collect();
        let coverage_by_fuzzer: IndexMap<String, &DataFrame> = coverage_by_fuzzer
            .iter()
            .map(|x| (x["fuzzer"].str_value(0).unwrap().to_string(), x))
            .collect();

        for (fuzzer, survival) in survival_by_fuzzer {
            let data_len = survival.height();
            if data_len == 0 {
                // No data for this partition, this can occur if we run the code on an incomplete
                // snapshot.
                continue;
            }

            // let (color, marker) = legend.find_or_insert(fuzzer.as_ref());
            let entry = legend.get_or_insert(fuzzer.as_ref());

            let hours = survival["duration"].f64()?;
            let count = survival["count"].u32()?;

            let non_null_count = hours.into_iter().filter(|x| x.is_some()).count() as u32;
            let to_end =
                (non_null_count < trials).then_some((max_hours, (trials - non_null_count) as f32));

            let hours = || hours.into_iter().flatten().map(|x| x as f32);
            let count = || count.into_no_null_iter().map(|x| (trials - x - 1) as f32);
            let data =
                || [(0.0, trials as f32)].into_iter().chain(hours().zip(count())).chain(to_end);

            survival_ctx.draw_series(LineSeries::new(StepIter::new(data()), &entry.color))?;
            entry.marker.draw_markers(&mut survival_ctx, hours().zip(count()), &entry.color)?;

            let coverage: &DataFrame = match coverage_by_fuzzer.get(&fuzzer) {
                Some(df) => df,
                None => continue,
            };

            for trial in coverage.partition_by_stable(["trial"], false)? {
                let hours = trial["hours"].f64()?;
                let blocks = trial["blocks"].u32()?;
                let last = [(max_hours, blocks.max().unwrap_or(0) as f32)];
                let hours = || hours.into_no_null_iter().map(|x| x as f32);
                let blocks = || blocks.into_no_null_iter().map(|x| x as f32);
                let data = || hours().zip(blocks()).chain(last);
                coverage_ctx.draw_series(LineSeries::new(StepIter::new(data()), &entry.color))?;
            }

            if !config.survival_hide_rect {
                let max_blocks = coverage["blocks"].u32().unwrap().max().unwrap_or(0);
                // Draw a rectangle over the coverage plot corresponding to the region we are evaluating
                // the block survival for.
                let x0 = survival["start_time"].f64()?.min().unwrap_or(max_hours as f64) as f32;
                let y0 = survival["start_blocks"].u32()?.min().unwrap_or(max_blocks) as f32;
                let x1 = survival["end_time"].f64()?.max().unwrap_or(max_hours as f64) as f32;
                let y1 = survival["end_blocks"].u32()?.max().unwrap_or(max_blocks) as f32;

                let rect = Rectangle::new([(x0, y1), (x1, y0)], entry.color.mix(0.2).filled());
                coverage_ctx.draw_series([rect])?;
                let rect = Rectangle::new([(x0, y1), (x1, y0)], BLACK.stroke_width(1));
                coverage_ctx.draw_series([rect])?;
            }
        }
    }

    let axis_label_style = TextStyle::from(("Arial", 20).into_font());
    draw_x_axis_label(x_axis_area, "Duration (hours)", &axis_label_style)?;
    legend.draw(&legend_area.margin(5, 0, 0, 0))?;

    root.present()?;
    Ok(())
}
