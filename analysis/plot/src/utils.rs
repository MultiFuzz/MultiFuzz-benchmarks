use std::collections::HashMap;

use plotters::{
    coord::Shift,
    prelude::*,
    style::text_anchor::{HPos, Pos, VPos},
};

#[derive(Copy, Clone)]
pub enum Marker {
    Circle,
    Cross,
    Triangle,
    #[allow(unused)]
    Square,
}

impl Marker {
    pub const ALL: &'static [Marker] = &[Marker::Circle, Marker::Triangle, Marker::Cross];

    pub fn pick(index: usize) -> Self {
        Self::ALL[index % Self::ALL.len()]
    }

    pub fn draw_markers<DB, ColorType, RangeX, RangeY, X, Y>(
        &self,
        ctx: &mut ChartContext<DB, Cartesian2d<RangeX, RangeY>>,
        points: impl Iterator<Item = (X, Y)>,
        color: &ColorType,
    ) -> anyhow::Result<()>
    where
        DB: DrawingBackend,
        DB::ErrorType: 'static,
        RangeX: Ranged,
        RangeX::ValueType: 'static,
        RangeY: Ranged,
        RangeY::ValueType: 'static,
        X: Into<RangeX::ValueType>,
        Y: Into<RangeY::ValueType>,
        ColorType: Color,
    {
        let style = color.filled();
        match self {
            Self::Circle => {
                ctx.draw_series(points.map(|(x, y)| Circle::new((x.into(), y.into()), 2, style)))?;
            }
            Self::Cross => {
                ctx.draw_series(points.map(|(x, y)| Cross::new((x.into(), y.into()), 2, style)))?;
            }
            Self::Triangle => {
                ctx.draw_series(
                    points.map(|(x, y)| TriangleMarker::new((x.into(), y.into()), 3, style)),
                )?;
            }
            Self::Square => unimplemented!(),
        }
        Ok(())
    }

    fn icon_element<DB>(
        &self,
        size: i32,
        color: PaletteColor<CustomPalette>,
    ) -> DynElement<'static, DB, (i32, i32)>
    where
        DB: DrawingBackend,
        DB::ErrorType: 'static,
    {
        let (cx, cy) = (size / 2, size / 2);
        let icon_marker = match self {
            Marker::Circle => Circle::new((cx, cy), size / 4, color.filled()).into_dyn(),
            Marker::Triangle => TriangleMarker::new((cx, cy), size / 3, color.filled()).into_dyn(),
            Marker::Cross => Cross::new((cx, cy), size / 4, color.filled()).into_dyn(),
            Marker::Square => Rectangle::new(
                [(cx - size / 4, cy - size / 4), (cx + size / 4, cy + size / 4)],
                color.filled(),
            )
            .into_dyn(),
        };
        icon_marker
    }
}

#[derive(Copy, Clone)]
pub struct CustomPalette;
impl Palette for CustomPalette {
    const COLORS: &'static [(u8, u8, u8)] = &[
        (230, 25, 75),
        (60, 180, 75),
        (0, 130, 200),
        (245, 130, 48),
        (145, 30, 180),
        (70, 240, 240),
        (240, 50, 230),
        (210, 245, 60),
        (250, 190, 190),
        (0, 128, 128),
        (230, 190, 255),
        (170, 110, 40),
        (255, 250, 200),
        (128, 0, 0),
        (170, 255, 195),
        (128, 128, 0),
        (255, 215, 180),
        (0, 0, 128),
        (128, 128, 128),
        (0, 0, 0),
    ];
}

pub struct LegendEntry {
    pub name: String,
    pub color: PaletteColor<CustomPalette>,
    pub marker: Marker,
    pub bold: bool,
}

pub struct Legend<'a> {
    pub entries: Vec<LegendEntry>,
    pub label_style: TextStyle<'a>,
    pub icon_size: i32,
    pub icon_spacing: i32,
    pub element_spacing: i32,
    pub mapping: HashMap<String, usize>,
    pub next_id: usize,
}

impl<'a> Legend<'a> {
    #[allow(unused)]
    pub fn new(label_style: TextStyle<'a>) -> Self {
        Self::new_with_mapping(label_style, HashMap::default())
    }

    pub fn new_with_mapping(label_style: TextStyle<'a>, mapping: HashMap<String, usize>) -> Self {
        let next_id = mapping.values().copied().max().map_or(0, |id| id + 1);
        Self {
            entries: vec![],
            label_style,
            icon_size: 12,
            icon_spacing: 6,
            element_spacing: 15,
            mapping,
            next_id,
        }
    }
}

impl<'a> Legend<'a> {
    pub fn get_or_insert(&mut self, name: &str) -> &mut LegendEntry {
        let position =
            self.entries.iter().position(|x| x.name == name).unwrap_or(self.entries.len());
        if position == self.entries.len() {
            let id = match self.mapping.get(name) {
                Some(id) => *id,
                None => {
                    self.next_id += 1;
                    self.next_id - 1
                }
            };
            let color = CustomPalette::pick(id);
            let marker = Marker::pick(id);
            self.entries.push(LegendEntry { name: name.into(), color, marker, bold: false });
        }
        &mut self.entries[position]
    }

    pub fn draw<DB>(&self, area: &DrawingArea<DB, Shift>) -> anyhow::Result<()>
    where
        DB: DrawingBackend,
        DB::ErrorType: 'static,
    {
        // Keep track of the combined legend width for centering.
        let mut total_legend_w = 0;
        let mut elements = vec![];
        for entry in &self.entries {
            let mut label = MultiLineText::<_, String>::new((0, 0), &self.label_style);
            for line in entry.name.split('\n') {
                label.push_line(line);
            }
            let (label_w, _label_h) = label.estimate_dimension()?;

            // Move the label next to the icon, align the baseline of the text with the icon
            // baseline.
            label.relocate((self.icon_size + self.icon_spacing, self.icon_size));
            let total_element_w = label_w + self.icon_size + self.icon_spacing;
            elements.push((label, (entry.color, entry.marker), total_element_w));

            total_legend_w += total_element_w + self.element_spacing;
        }
        // Remove spacing after final element.
        total_legend_w -= self.element_spacing;

        // Draw all the elements at the correct position.
        let (area_w, _area_h) = area.dim_in_pixel();
        let mut x = (area_w as i32 - total_legend_w) / 2;
        for (label, (color, marker), width) in elements {
            let icon_bg =
                Rectangle::new([(0, 0), (self.icon_size, self.icon_size)], color.mix(0.4).filled());
            let icon_marker = marker.icon_element(self.icon_size, color);
            area.draw(&(EmptyElement::<_, DB>::at((x, 0)) + icon_bg + icon_marker + label))?;
            x += width + self.element_spacing;
        }

        Ok(())
    }

    #[allow(unused)]
    pub fn draw_vertical<DB>(&self, area: &DrawingArea<DB, Shift>) -> anyhow::Result<()>
    where
        DB: DrawingBackend,
        DB::ErrorType: 'static,
    {
        // Keep track of the maximum legend width for centering.
        let mut max_legend_w = 0;
        let mut elements = vec![];
        for entry in &self.entries {
            let mut label = MultiLineText::<_, String>::new((0, 0), &self.label_style);
            for line in entry.name.split('\n') {
                label.push_line(line);
            }
            let (label_w, _label_h) = label.estimate_dimension()?;

            // Move the label next to the icon, align the baseline of the text with the icon
            // baseline.
            label.relocate((self.icon_size + self.icon_spacing, self.icon_size));
            let total_element_w = label_w + self.icon_size + self.icon_spacing;
            elements.push((label, (entry.color, entry.marker), total_element_w));

            max_legend_w = max_legend_w.max(total_element_w);
        }

        // Draw all the elements at the correct position.
        let (area_w, _area_h) = area.dim_in_pixel();
        let x = (area_w as i32 - max_legend_w) / 2;
        let mut y = 0;
        for (label, (color, marker), _width) in elements {
            let icon_bg =
                Rectangle::new([(0, 0), (self.icon_size, self.icon_size)], color.mix(0.4).filled());
            let icon_marker = marker.icon_element(self.icon_size, color);
            area.draw(&(EmptyElement::<_, DB>::at((x, y)) + icon_bg + icon_marker + label))?;
            y += self.icon_size + self.icon_spacing;
        }

        Ok(())
    }
}

pub fn split_with_columns<DB>(
    plot_area: &DrawingArea<DB, Shift>,
    count: usize,
    max_cols: usize,
) -> Vec<DrawingArea<DB, Shift>>
where
    DB: DrawingBackend,
    DB::ErrorType: 'static,
{
    let cols = usize::min(count, max_cols);
    let rows = (count as f32 / cols as f32).ceil() as usize;

    // The last row might be partially empty, so count the number of entries in the last row so we
    // can center the last row later.
    let cols_in_last_row = cols - ((rows * cols) - count);

    let (width, height) = plot_area.dim_in_pixel();
    let col_width = width / cols as u32;
    let row_height = height / rows as u32;

    let mut rest = plot_area.clone();

    let mut cells = Vec::with_capacity(count);
    for _ in 0..(rows - 1) {
        let (row_area, x) = rest.split_vertically(row_height);
        cells.extend(row_area.split_evenly((1, cols)));
        rest = x;
    }

    // Handle the last row.
    let plot_area = if cols != cols_in_last_row {
        // Split into 3 regions: | padding | row | padding | so row will be centered.
        let padding = width - (cols_in_last_row as u32 * col_width);
        let (_left, x) = rest.split_horizontally(padding / 2);
        let (row_area, _right) = x.split_horizontally(width - padding);
        row_area
    }
    else {
        rest
    };
    cells.extend(plot_area.split_evenly((1, cols_in_last_row)));

    cells
}

pub fn draw_subtitle<DB>(
    text: &str,
    area: &DrawingArea<DB, Shift>,
    left_axis_padding: i32,
    font_size: u32,
) -> anyhow::Result<()>
where
    DB: DrawingBackend,
    DB::ErrorType: 'static,
{
    let subtitle_style = TextStyle::from(("Arial", font_size).into_font().style(FontStyle::Italic))
        .with_anchor::<RGBAColor>(Pos::new(HPos::Center, VPos::Top))
        .into_text_style(area);
    let (area_w, _area_h) = area.dim_in_pixel();
    area.margin(5, 0, 0, 0).draw_text(
        text,
        &subtitle_style,
        (left_axis_padding + (area_w as i32 - left_axis_padding) / 2, 0),
    )?;
    Ok(())
}

pub fn draw_x_axis_label<DB>(
    area: DrawingArea<DB, Shift>,
    label: &str,
    label_style: &TextStyle,
) -> anyhow::Result<()>
where
    DB: DrawingBackend,
    DB::ErrorType: 'static,
{
    let (w, _h) = area.dim_in_pixel();
    area.margin(5, 0, 0, 0).draw_text(
        label,
        &label_style
            .clone()
            .with_anchor::<RGBAColor>(Pos::new(HPos::Center, VPos::Top))
            .into_text_style(&area),
        (w as i32 / 2, 0),
    )?;
    Ok(())
}

pub fn draw_y_axis_label<DB>(
    area: DrawingArea<DB, Shift>,
    label: &str,
    label_style: &TextStyle,
) -> anyhow::Result<()>
where
    DB: DrawingBackend,
    DB::ErrorType: 'static,
{
    let (_w, h) = area.dim_in_pixel();
    area.margin(0, 0, 5, 0).draw_text(
        label,
        &label_style
            .transform(FontTransform::Rotate270)
            .with_anchor::<RGBAColor>(Pos::new(HPos::Center, VPos::Top))
            .into_text_style(&area),
        (0, h as i32 / 2),
    )?;
    Ok(())
}

#[allow(unused)]
pub fn draw_legend<DB>(
    area: &DrawingArea<DB, Shift>,
    entries: &[(String, (PaletteColor<CustomPalette>, Marker))],
    label_style: TextStyle,
    icon_size: i32,
    icon_spacing: i32,
    element_spacing: i32,
) -> anyhow::Result<()>
where
    DB: DrawingBackend,
    DB::ErrorType: 'static,
{
    // Keep track of the combined legend width for centering.
    let mut total_legend_w = 0;
    let mut elements = vec![];
    for (name, attr) in entries {
        let mut label = MultiLineText::<_, String>::new((0, 0), &label_style);
        for line in name.split('\n') {
            label.push_line(line);
        }
        let (label_w, _label_h) = label.estimate_dimension()?;

        // Move the label next to the icon, align the baseline of the text with the icon baseline.
        label.relocate((icon_size + icon_spacing, icon_size));
        let total_element_w = label_w + icon_size + icon_spacing;
        elements.push((label, attr, total_element_w));

        total_legend_w += total_element_w + element_spacing;
    }
    // Remove spacing after final element.
    total_legend_w -= element_spacing;

    // Draw all the elements at the correct position.
    let (area_w, _area_h) = area.dim_in_pixel();
    let mut current_x = (area_w as i32 - total_legend_w) / 2;
    for (label, (color, marker), width) in elements {
        let icon_area = Rectangle::new([(0, 0), (icon_size, icon_size)], color.mix(0.4).filled());
        let center_x = icon_size / 2;
        let center_y = icon_size / 2;
        let icon_marker = match marker {
            Marker::Circle => {
                Circle::new((center_x, center_y), icon_size / 4, color.filled()).into_dyn()
            }
            Marker::Triangle => {
                TriangleMarker::new((center_x, center_y), icon_size / 3, color.filled()).into_dyn()
            }
            Marker::Cross => {
                Cross::new((center_x, center_y), icon_size / 4, color.filled()).into_dyn()
            }
            Marker::Square => Rectangle::new(
                [
                    (center_x - icon_size / 4, center_y - icon_size / 4),
                    (center_x + icon_size / 4, center_y + icon_size / 4),
                ],
                color.filled(),
            )
            .into_dyn(),
        };

        area.draw(&(EmptyElement::<_, DB>::at((current_x, 0)) + icon_area + icon_marker + label))?;
        current_x += width + element_spacing;
    }

    Ok(())
}

pub fn polygon_between(
    mut top: impl Iterator<Item = (f32, f32)>,
    bottom: impl DoubleEndedIterator<Item = (f32, f32)>,
) -> Vec<(f32, f32)> {
    let mut polygon = vec![];
    let (start_x, start_y) = top.next().unwrap();
    polygon.push((start_x, start_y));
    let mut prev_y = start_y;
    for (next_x, next_y) in top.chain(bottom.rev()) {
        polygon.push((next_x, prev_y));
        polygon.push((next_x, next_y));
        prev_y = next_y;
    }
    polygon
}

pub struct StepIter<I, X, Y> {
    iter: I,
    prev: Option<(X, Y)>,
    next: Option<(X, Y)>,
}

impl<I, X, Y> StepIter<I, X, Y>
where
    I: Iterator<Item = (X, Y)>,
{
    pub fn new(iter: I) -> Self {
        Self { iter, prev: None, next: None }
    }
}

impl<I, X, Y> Iterator for StepIter<I, X, Y>
where
    I: Iterator<Item = (X, Y)>,
    X: Copy,
    Y: Copy
{
    type Item = (X, Y);

    fn next(&mut self) -> Option<Self::Item> {
        let (next_x, next_y) = self.next.take().or_else(|| self.iter.next())?;
        match self.prev.take() {
            Some((_prev_x, prev_y)) => {
                self.next = Some((next_x, next_y));
                Some((next_x, prev_y))
            }
            None => {
                self.prev = Some((next_x, next_y));
                Some((next_x, next_y))
            }
        }
    }
}
