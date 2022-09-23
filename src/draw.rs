use std::borrow::Cow;
use std::ops::{Neg, Range};
use std::path::PathBuf;

use image::imageops::FilterType;
use image::ImageFormat;
use plotters::coord::combinators::WithKeyPoints;
use plotters::coord::ranged1d::ValueFormatter;
use plotters::coord::types::RangedCoordf64;
use plotters::coord::Shift;
use plotters::prelude::*;
use plotters::style::{Color, IntoFont, RGBColor, TextStyle};
use plotters_bitmap::bitmap_pixel::RGBPixel;
use plotters_bitmap::BitMapBackend;

use crate::data;
use crate::track::{Datum, Grading, TrackResult};
use crate::utils::{ft_to_nm, m_to_ft, m_to_nm, nm_to_ft, nm_to_m};

const THEME_BG: RGBColor = RGBColor(31, 41, 55); // 1F2937
const THEME_FG: RGBColor = RGBColor(156, 163, 175); // 9CA3AF

const THEME_GUIDE_RED: RGBColor = RGBColor(239, 68, 68); // EF4444
const THEME_GUIDE_YELLOW: RGBColor = RGBColor(254, 240, 138); // FEF08A
const THEME_GUIDE_GREEN: RGBColor = RGBColor(34, 197, 94); // 22C55E
const THEME_GUIDE_GRAY: RGBColor = RGBColor(100, 116, 139); // 64748B

const THEME_TRACK_RED: RGBColor = RGBColor(239, 68, 68); // EF4444
const THEME_TRACK_YELLOW: RGBColor = RGBColor(254, 240, 138); // FEF08A
const THEME_TRACK_GREEN: RGBColor = RGBColor(34, 197, 94); // 22C55E

const WIDTH: u32 = 1000;
const X_LABEL_AREA_SIZE: u32 = 30;
const RANGE_X: Range<f64> = -0.02..0.78;
const TOP_RANGE_Y: Range<f64> = -0.15..0.15;
const SIDE_RANGE_Y: Range<f64> = 0.0..350.0;
const OVERLAP_OFFSET: u32 = 130;

#[tracing::instrument(skip_all)]
pub fn draw_chart(
    out_dir: &std::path::Path,
    filename: &str,
    track: &TrackResult,
) -> Result<PathBuf, DrawError> {
    let side_height = ((ft_to_nm(SIDE_RANGE_Y.end - SIDE_RANGE_Y.start) * 5.0
        / (RANGE_X.end - RANGE_X.start))
        * (WIDTH as f64))
        .floor() as u32;

    let top_height = (((TOP_RANGE_Y.end - TOP_RANGE_Y.start) / (RANGE_X.end - RANGE_X.start))
        * (WIDTH as f64))
        .floor() as u32
        - OVERLAP_OFFSET;

    let path = out_dir.join(filename).with_extension("png");
    let root_drawing_area =
        BitMapBackend::new(&path, (WIDTH, top_height + side_height + X_LABEL_AREA_SIZE))
            .into_drawing_area();
    root_drawing_area.fill(&THEME_BG)?;

    let (side, _) = root_drawing_area.split_vertically(side_height);
    let (_, top) = root_drawing_area.split_vertically(side_height - OVERLAP_OFFSET);

    draw_side_view(track, side)?;
    draw_top_view(track, top)?;

    let text_style = TextStyle::from(("sans-serif", 24).into_font()).color(&THEME_FG);

    root_drawing_area.draw_text(
        &format!("Pilot: {}", track.pilot_name),
        &text_style,
        (16, 16),
    )?;

    root_drawing_area.draw_text(
        &format!(
            "Grading: {}",
            match track.grading {
                Grading::Unknown => Cow::Borrowed("unknown"),
                Grading::Bolter => Cow::Borrowed("Bolter"),
                Grading::Recovered { cable, .. } => cable
                    .map(|c| Cow::Owned(format!("#{}", c)))
                    .unwrap_or(Cow::Borrowed("-")),
            }
        ),
        &text_style,
        (16, 48),
    )?;

    std::mem::drop(root_drawing_area);

    Ok(path)
}

#[tracing::instrument(skip_all)]
pub fn draw_top_view(
    track: &TrackResult,
    canvas: DrawingArea<BitMapBackend<'_, RGBPixel>, Shift>,
) -> Result<(), DrawError> {
    let mut chart = ChartBuilder::on(&canvas)
        .margin(0u32)
        .x_label_area_size(X_LABEL_AREA_SIZE)
        .y_label_area_size(0u32)
        .build_cartesian_2d(
            CustomRange(RANGE_X.with_key_points(vec![0.25f64, 0.5, 0.75, 1.0])),
            TOP_RANGE_Y,
        )?;

    // Then we can draw a mesh
    chart
        .configure_mesh()
        .disable_mesh()
        .disable_y_axis()
        .axis_style(THEME_FG)
        .x_label_style(text_style())
        .draw()?;

    // carrier top image is 300x300px which corresponds to 115x115m
    let (w, _h) = canvas.dim_in_pixel();
    let a = nm_to_m(RANGE_X.end - RANGE_X.start);
    let m2px = f64::from(w) / a;
    let img_size = ((115.0 * m2px) as u32, (115.0 * m2px) as u32);
    let img_carrier_top = image::load_from_memory_with_format(
        include_bytes!("../img/carrier-top.png"),
        ImageFormat::Png,
    )?
    .resize_exact(img_size.0, img_size.1, FilterType::Nearest);
    let elem: BitMapElement<_> = (
        (-m_to_nm(115.0 * 1.0 / 3.0), m_to_nm(115.0 / 2.0)),
        img_carrier_top,
    )
        .into();
    chart.draw_series(std::iter::once(elem))?;

    // draw centerline
    // Source: A Review and Analysis of Precision Approach and Landing System (PALS) Certifification
    // Procedures, Figure 5
    let lines = [
        // 0.25degree on center line
        (0.25f64, THEME_GUIDE_GRAY),
        // orange
        (0.75, THEME_GUIDE_GREEN),
        // red
        (3.0, THEME_GUIDE_YELLOW),
        // red
        (6.0, THEME_GUIDE_RED),
    ];

    for (deg, color) in lines {
        let y = deg.to_radians().tan() * RANGE_X.end;
        chart.draw_series(LineSeries::new(
            [(0.0, 0.0), (RANGE_X.end, y)],
            color.mix(0.4),
        ))?;
        chart.draw_series(LineSeries::new(
            [(0.0, 0.0), (RANGE_X.end, y.neg())],
            color.mix(0.4),
        ))?;
    }

    let track_in_nm = track
        .datums
        .iter()
        .map(|d| Datum {
            x: m_to_nm(d.x),
            y: m_to_nm(d.y),
            aoa: d.aoa,
            alt: d.alt,
        })
        .filter(|d| RANGE_X.contains(&d.x) && TOP_RANGE_Y.contains(&d.y));

    // draw approach shadow
    chart.draw_series(LineSeries::new(
        track_in_nm.clone().map(|d| (d.x, d.y)),
        THEME_BG.stroke_width(4),
    ))?;

    // draw approach
    let mut points = Vec::new();
    let mut color = THEME_TRACK_GREEN;
    for datum in track_in_nm {
        let next_color = aoa_color(datum.aoa);
        let point = (datum.x, datum.y);

        if points.is_empty() {
            color = next_color;
        }

        if next_color != color {
            points.push(point);

            chart.draw_series(LineSeries::new(
                points.iter().cloned(),
                color.stroke_width(2),
            ))?;

            points.clear();
            color = next_color;
        }

        points.push(point);
    }

    if !points.is_empty() {
        chart.draw_series(LineSeries::new(
            points.iter().cloned(),
            color.stroke_width(2),
        ))?;
    }
    Ok(())
}

#[tracing::instrument(skip_all)]
pub fn draw_side_view(
    track: &TrackResult,
    canvas: DrawingArea<BitMapBackend<'_, RGBPixel>, Shift>,
) -> Result<(), DrawError> {
    let mut chart = ChartBuilder::on(&canvas)
        .margin(0u32)
        .x_label_area_size(0u32)
        .y_label_area_size(0u32)
        .build_cartesian_2d(
            CustomRange(RANGE_X.with_key_points(vec![0.25f64, 0.5, 0.75, 1.0])),
            SIDE_RANGE_Y,
        )?;

    // Then we can draw a mesh
    chart
        .configure_mesh()
        .disable_mesh()
        .disable_x_axis()
        .disable_y_axis()
        .axis_style(THEME_FG)
        .x_label_style(text_style())
        .draw()?;

    // carrier side image is 300x150px which corresponds to 115x57.5m
    let (w, _h) = canvas.dim_in_pixel();
    let a = nm_to_m(RANGE_X.end - RANGE_X.start);
    let m2px = f64::from(w) / a;
    let img_size = ((115.0 * m2px) as u32, (57.5 * m2px) as u32);
    let img_carrier_side = image::load_from_memory_with_format(
        include_bytes!("../img/carrier-side.png"),
        ImageFormat::Png,
    )?
    .resize_exact(img_size.0, img_size.1, FilterType::Nearest);
    let elem: BitMapElement<_> = ((-m_to_nm(115.0 * 1.0 / 3.0), 24.0), img_carrier_side).into();
    chart.draw_series(std::iter::once(elem))?;

    // draw centerline
    let lines = [
        (data::FA18C.glide_slope - 0.9, THEME_GUIDE_RED),
        (data::FA18C.glide_slope - 0.6, THEME_GUIDE_YELLOW),
        (data::FA18C.glide_slope - 0.25, THEME_GUIDE_GREEN),
        (data::FA18C.glide_slope, THEME_GUIDE_GRAY),
        (data::FA18C.glide_slope + 0.25, THEME_GUIDE_GREEN),
        (data::FA18C.glide_slope + 0.7, THEME_GUIDE_YELLOW),
        (data::FA18C.glide_slope + 1.5, THEME_GUIDE_RED),
    ];

    for (deg, color) in lines {
        let mut x = RANGE_X.end;
        let mut y = nm_to_ft(deg.to_radians().tan() * RANGE_X.end);
        if y > SIDE_RANGE_Y.end {
            x = ft_to_nm(SIDE_RANGE_Y.end) / deg.to_radians().tan();
            y = SIDE_RANGE_Y.end;
        }
        chart.draw_series(LineSeries::new([(0.0, 0.0), (x, y)], color.mix(0.4)))?;
    }

    let track_descent = track
        .datums
        .iter()
        .map(|d| Datum {
            x: m_to_nm(d.x),
            y: d.y,
            aoa: d.aoa,
            alt: m_to_ft(d.alt),
        })
        .filter(|d| RANGE_X.contains(&d.x) && SIDE_RANGE_Y.contains(&d.alt));

    // draw approach shadow
    chart.draw_series(LineSeries::new(
        track_descent.clone().map(|d| (d.x, d.alt)),
        THEME_BG.stroke_width(4),
    ))?;

    // draw approach
    let mut points = Vec::new();
    let mut color = THEME_TRACK_GREEN;
    for datum in track_descent {
        let next_color = aoa_color(datum.aoa);

        let point = (datum.x, datum.alt);

        if points.is_empty() {
            color = next_color;
        }

        if next_color != color {
            points.push(point);

            chart.draw_series(LineSeries::new(
                points.iter().cloned(),
                color.stroke_width(2),
            ))?;

            points.clear();
            color = next_color;
        }

        points.push(point);
    }

    if !points.is_empty() {
        chart.draw_series(LineSeries::new(
            points.iter().cloned(),
            color.stroke_width(2),
        ))?;
    }

    Ok(())
}

fn text_style() -> TextStyle<'static> {
    TextStyle::from(("sans-serif", 20).into_font()).color(&THEME_FG)
}

fn aoa_color(aoa: f64) -> RGBColor {
    // https://forums.vrsimulations.com/support/index.php/Navigation_Tutorial_Flight#Angle_of_Attack_Bracket
    if aoa <= 6.9 {
        // fast
        THEME_TRACK_RED
    } else if aoa <= 7.4 {
        // slightly fast
        THEME_TRACK_YELLOW
    } else if aoa < 8.8 {
        // on speed
        THEME_TRACK_GREEN
    } else if aoa < 9.3 {
        // slightly slow
        THEME_TRACK_YELLOW
    } else {
        // slow
        THEME_TRACK_RED
    }
}

struct CustomRange(WithKeyPoints<RangedCoordf64>);

impl Ranged for CustomRange {
    type ValueType = <plotters::coord::types::RangedCoordf64 as Ranged>::ValueType;
    type FormatOption = plotters::coord::ranged1d::NoDefaultFormatting;

    fn map(&self, value: &Self::ValueType, limit: (i32, i32)) -> i32 {
        self.0.map(value, limit)
    }

    fn key_points<Hint: plotters::coord::ranged1d::KeyPointHint>(
        &self,
        hint: Hint,
    ) -> Vec<Self::ValueType> {
        self.0.key_points(hint)
    }

    fn range(&self) -> std::ops::Range<Self::ValueType> {
        self.0.range()
    }

    fn axis_pixel_range(&self, limit: (i32, i32)) -> std::ops::Range<i32> {
        self.0.axis_pixel_range(limit)
    }
}

impl ValueFormatter<f64> for CustomRange {
    fn format(v: &f64) -> String {
        match *v {
            v if (v - 0.25).abs() < f64::EPSILON => "¼ nm".to_string(),
            v if (v - 0.50).abs() < f64::EPSILON => "½ nm".to_string(),
            v if (v - 0.75).abs() < f64::EPSILON => "¾ nm".to_string(),
            _ => format!("{}nm", v),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DrawError {
    #[error(transparent)]
    Plotter(#[from] DrawingAreaErrorKind<<BitMapBackend<'static> as DrawingBackend>::ErrorType>),
    #[error(transparent)]
    Image(#[from] image::ImageError),
}
