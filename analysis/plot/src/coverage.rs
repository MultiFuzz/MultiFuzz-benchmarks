use anyhow::Context;
use plotters::{
    coord::{types::RangedCoordf32, Shift},
    prelude::*,
    style::text_anchor::{HPos, Pos, VPos},
};
use polars::prelude::*;

use plot_data::{name_of_binary, Config};

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

    let by_binary = data.partition_by_stable(["binary"], true).context("partition_by(binary)")?;
    let n_binaries = by_binary.len();

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

    let plot_regions = split_with_columns(&plot_area, n_binaries, n_cols as usize);
    for (df, region) in by_binary.iter().zip(plot_regions) {
        let name = df["binary"].str_value(0)?;
        let max_y = df["blocks_max"].u32()?.max().unwrap();

        // Split the region into a chart area and a subtitle area. (We don't use `chart.caption(..)`
        // to add subtitles to each plot because we want to center the labels excluding the axis
        // ticks).
        let (subtitle, plot) = region.split_vertically(18);

        let left_axis_padding = 35;
        draw_subtitle(&name_of_binary(&name), &subtitle, left_axis_padding, 16)?;

        let mut subchart = ChartBuilder::on(&plot);
        let mut ctx = subchart
            .margin(4)
            .set_label_area_size(LabelAreaPosition::Bottom, 15)
            .set_label_area_size(LabelAreaPosition::Left, left_axis_padding)
            .build_cartesian_2d(0_f32..24_f32, 0_f32..max_y as f32)?;
        ctx.configure_mesh()
            .max_light_lines(0)
            .x_label_formatter(&|value| format!("{}", *value as u64))
            .x_labels(6)
            .x_label_style(TextStyle::from(("Arial", 14).into_font()))
            .y_label_formatter(&|value| format!("{}", *value as u64))
            .y_labels(8)
            .y_label_style(TextStyle::from(("Arial", 14).into_font()))
            .draw()
            .unwrap();

        for df in df.partition_by_stable(["fuzzer"], true).context("partition_by(fuzzer)")? {
            let fuzzer = df["fuzzer"].str_value(0)?;
            let entry = legend.get_or_insert(fuzzer.as_ref());
            draw_coverage_subplot(&mut ctx, &df, &entry.color, entry.marker)?;
        }

        // Fade plots of binaries that have bug exploits.
        if config.has_bug_exploit(&name) {
            region.fill(&RGBColor(230, 230, 230).mix(0.4))?;
        }
    }

    let axis_label_style = TextStyle::from(("Arial", 20).into_font());
    draw_y_axis_label(y_axis_area, "#Blocks Hit", &axis_label_style)?;
    draw_x_axis_label(x_axis_area, "Duration (hours)", &axis_label_style)?;

    legend.draw(&legend_area.margin(5, 0, 0, 0))?;
    // legend.draw_vertical(&legend_area.margin(5, 0, 0, 0))?;

    root.present()?;
    Ok(())
}

pub fn draw_coverage_subplot<DB>(
    ctx: &mut ChartContext<DB, Cartesian2d<RangedCoordf32, RangedCoordf32>>,
    df: &DataFrame,
    color: &PaletteColor<CustomPalette>,
    marker: Marker,
) -> anyhow::Result<()>
where
    DB: DrawingBackend,
    DB::ErrorType: 'static,
{
    let data_len = df.height();
    if data_len == 0 {
        // No data for this partition, this can occur if we run the code on an incomplete
        // snapshot.
        return Ok(());
    }

    let hours = df["hours"].f64()?;
    let blocks_med = df["blocks_median"].f64()?;
    let blocks_max = df["blocks_max"].u32()?;
    let blocks_min = df["blocks_min"].u32()?;

    // Create helper functions for constructing iterators with the correct types.
    let hours = || hours.into_no_null_iter().map(|x| x as f32);
    let blocks_med = || blocks_med.into_no_null_iter().map(|x| x as f32);
    let blocks_min = || blocks_min.into_no_null_iter().map(|x| x as f32);
    let blocks_max = || blocks_max.into_no_null_iter().map(|x| x as f32);

    // Draw a polygon covering the min-max coverage.
    ctx.draw_series([Polygon::new(
        polygon_between(hours().zip(blocks_max()), hours().zip(blocks_min())),
        color.mix(0.2).filled(),
    )])?;

    // Draw line showing median coverage.
    let data = || hours().zip(blocks_med());
    ctx.draw_series(LineSeries::new(StepIter::new(data()), &color))?;

    // Draw markers along the median every 2 hours.
    let step_size = ((data_len * 2) / 24).max(1);
    marker.draw_markers(ctx, data().step_by(step_size), color)?;

    Ok(())
}
