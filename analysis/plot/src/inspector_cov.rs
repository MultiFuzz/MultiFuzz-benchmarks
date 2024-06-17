use anyhow::Context;
use plotters::{
    coord::Shift,
    prelude::*,
    style::text_anchor::{HPos, Pos, VPos},
};
use polars::prelude::*;

use plot_data::Config;

use crate::utils::{
    draw_subtitle, draw_x_axis_label, draw_y_axis_label, polygon_between, split_with_columns,
    CustomPalette, Legend, Marker, StepIter,
};

pub fn coverage_over_time<DB>(
    root: &DrawingArea<DB, Shift>,
    config: &Config,
    data: &DataFrame,
    n_cols: u32,
) -> anyhow::Result<()>
where
    DB: DrawingBackend,
    DB::ErrorType: 'static,
{
    root.fill(&WHITE)?;

    let by_arch = data.partition_by_stable(["arch"], true).context("partition_by(arch)")?;
    let n_arch = by_arch.len();

    // Add regions for combined axis.
    let (legend_area, y_axis_area, x_axis_area, plot_area) = {
        let (plot_area, legend_area) = root.split_vertically(root.dim_in_pixel().1 - 45);
        let (y_axis_area, plot_area) = plot_area.split_horizontally(20);
        let (plot_area, x_axis_area) = plot_area.split_vertically(plot_area.dim_in_pixel().1 - 25);
        (legend_area, y_axis_area, x_axis_area, plot_area)
    };

    let legend_label_style = TextStyle::from(("Arial", 18).into_font())
        .with_anchor::<RGBAColor>(Pos::new(HPos::Left, VPos::Bottom))
        .into_text_style(&legend_area);
    let mut legend = Legend::new_with_mapping(legend_label_style, config.legend_mapping.clone());

    let plot_regions = split_with_columns(&plot_area, n_arch, n_cols as usize);
    for (df, region) in by_arch.iter().zip(plot_regions) {
        let name = df["arch"].str_value(0)?;
        let _max_y = df["cov_max"].f32()?.max().unwrap();
        let max_x = df["testcase"].i64()?.max().unwrap() as f32;

        // Split the region into a chart area and a subtitle area. (We don't use `chart.caption(..)`
        // to add subtitles to each plot because we want to center the labels excluding the axis
        // ticks).
        let (subtitle, plot) = region.split_vertically(18);

        let left_axis_padding = 35;
        draw_subtitle(&name, &subtitle, left_axis_padding, 16)?;

        let mut subchart = ChartBuilder::on(&plot);
        let mut ctx = subchart
            .margin(4)
            .set_label_area_size(LabelAreaPosition::Bottom, 15)
            .set_label_area_size(LabelAreaPosition::Left, left_axis_padding)
            // .build_cartesian_2d(0_f32..max_x, 0_f32..100.0)?;
            .build_cartesian_2d((0_f32..max_x).log_scale(), 0_f32..100.0)?;
        ctx.configure_mesh()
            .y_max_light_lines(0)
            .x_max_light_lines(0)
            .x_labels(7)
            .x_label_formatter(&|value| format!("{:e}", value))
            .x_label_style(TextStyle::from(("Arial", 14).into_font()))
            .y_labels(8)
            .y_label_style(TextStyle::from(("Arial", 14).into_font()))
            .draw()
            .unwrap();

        for df in df.partition_by_stable(["kind"], true).context("partition_by(kind)")? {
            let kind = df["kind"].str_value(0)?;
            let entry = legend.get_or_insert(kind.as_ref());
            draw_coverage_subplot(&mut ctx, &df, &entry.color, entry.marker, 30, max_x)?;
        }
    }

    let axis_label_style = TextStyle::from(("Arial", 20).into_font());
    draw_y_axis_label(y_axis_area, "% Constructors Covered", &axis_label_style)?;
    draw_x_axis_label(x_axis_area, "Instructions", &axis_label_style)?;

    legend.draw(&legend_area.margin(5, 0, 0, 0))?;
    // legend.draw_vertical(&legend_area.margin(5, 0, 0, 0))?;

    root.present()?;
    Ok(())
}

pub fn draw_coverage_subplot<DB, RangeX, RangeY>(
    ctx: &mut ChartContext<DB, Cartesian2d<RangeX, RangeY>>,
    df: &DataFrame,
    color: &PaletteColor<CustomPalette>,
    marker: Marker,
    marker_resolution: usize,
    max_x: f32,
) -> anyhow::Result<()>
where
    DB: DrawingBackend,
    DB::ErrorType: 'static,
    RangeX: Ranged<ValueType = f32>,
    RangeY: Ranged<ValueType = f32>,
{
    let data_len = df.height();
    if data_len == 0 {
        // No data for this partition, this can occur if we run the code on an incomplete
        // snapshot.
        return Ok(());
    }

    let testcases = df["testcase"].i64()?;
    let cov_med = df["cov_median"].f32()?;
    let cov_min = df["cov_min"].f32()?;
    let cov_max = df["cov_max"].f32()?;

    // // Draw horizontal dotted line showing max
    let max = cov_max.max().unwrap();
    ctx.draw_series(DashedLineSeries::new(vec![(0.0, max), (max_x, max)], 2, 1, color.into()))?;

    // Create helper functions for constructing iterators with the correct types.
    let x = || testcases.into_no_null_iter().map(|x| x as f32);
    let y_med = || cov_med.into_no_null_iter().map(|x| x as f32);
    let y_min = || cov_min.into_no_null_iter().map(|x| x as f32);
    let y_max = || cov_max.into_no_null_iter().map(|x| x as f32);

    // Draw a polygon covering the min-max coverage.
    ctx.draw_series([Polygon::new(
        polygon_between(x().zip(y_max()), x().zip(y_min())),
        color.mix(0.2).filled(),
    )])?;

    // Draw line showing median coverage.
    let data = || x().zip(y_med());
    ctx.draw_series(LineSeries::new(StepIter::new(data()), &color))?;

    let step_size = (data_len / marker_resolution).max(1);
    marker.draw_markers(ctx, data().step_by(step_size).chain(data().last()), color)?;

    Ok(())
}
