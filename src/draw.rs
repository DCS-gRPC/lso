use std::ops::{Neg, Range};

use image::imageops::FilterType;
use image::ImageFormat;
use plotters::coord::combinators::WithKeyPoints;
use plotters::coord::ranged1d::ValueFormatter;
use plotters::coord::types::RangedCoordf64;
use plotters::prelude::*;

use crate::data;
use crate::track::{Datum, TrackResult};
use crate::utils::{ft_to_nm, m_to_ft, m_to_nm, nm_to_ft, nm_to_m};

const THEME_BG: RGBColor = RGBColor(30, 41, 49);
const THEME_FG: RGBColor = RGBColor(203, 213, 225);

const THEME_GUIDE_RED: RGBColor = RGBColor(248, 113, 113);
const THEME_GUIDE_YELLOW: RGBColor = RGBColor(250, 204, 21);
const THEME_GUIDE_GREEN: RGBColor = RGBColor(163, 230, 53);
const THEME_GUIDE_GRAY: RGBColor = RGBColor(100, 116, 139);

const THEME_TRACK_RED: RGBColor = RGBColor(248, 113, 113);
const THEME_TRACK_YELLOW: RGBColor = RGBColor(250, 204, 21);
const THEME_TRACK_GREEN: RGBColor = RGBColor(132, 230, 53);

#[tracing::instrument(skip_all)]
pub fn draw_chart(track: TrackResult) {
    const WIDTH: u32 = 1000;
    const X_LABEL_AREA_SIZE: u32 = 30;
    const TOP_RANGE_X: Range<f64> = -0.02..0.76;
    const TOP_RANGE_Y: Range<f64> = -0.15..0.15;
    const SIDE_RANGE_X: Range<f64> = TOP_RANGE_X;
    const SIDE_RANGE_Y: Range<f64> = 0.0..500.0;

    let top_height = (((TOP_RANGE_Y.end - TOP_RANGE_Y.start)
        / (TOP_RANGE_X.end - TOP_RANGE_X.start))
        * (WIDTH as f64))
        .floor() as u32;

    let side_height = ((ft_to_nm(SIDE_RANGE_Y.end - SIDE_RANGE_Y.start) * 5.0
        / (SIDE_RANGE_X.end - SIDE_RANGE_X.start))
        * (WIDTH as f64))
        .floor() as u32;

    let text_style = TextStyle::from(("sans-serif", 20).into_font()).color(&THEME_FG);
    let root_drawing_area = BitMapBackend::new(
        "test.png",
        (WIDTH, top_height + side_height + X_LABEL_AREA_SIZE),
    )
    .into_drawing_area();
    root_drawing_area.fill(&THEME_BG).unwrap();
    let (top, bottom) = root_drawing_area.split_vertically(top_height);

    let mut chart = ChartBuilder::on(&top)
        .margin(5)
        .x_label_area_size(0)
        .y_label_area_size(0)
        .build_cartesian_2d(
            CustomRange(TOP_RANGE_X.with_key_points(vec![0.25f64, 0.5, 0.75, 1.0])),
            TOP_RANGE_Y,
        )
        .unwrap();

    // Then we can draw a mesh
    chart
        .configure_mesh()
        .disable_mesh()
        .disable_x_axis()
        .disable_y_axis()
        .axis_style(THEME_FG)
        .x_label_style(text_style.clone())
        .draw()
        .unwrap();

    // carrier top image is 300x300px which corresponds to 115x115m
    let (w, _h) = top.dim_in_pixel();
    let a = nm_to_m(TOP_RANGE_X.end - TOP_RANGE_X.start);
    let m2px = f64::from(w) / a;
    let img_size = ((115.0 * m2px) as u32, (115.0 * m2px) as u32);
    let img_carrier_top = image::load_from_memory_with_format(
        include_bytes!("../img/carrier-top.png"),
        ImageFormat::Png,
    )
    .unwrap()
    .resize_exact(img_size.0, img_size.1, FilterType::Nearest);
    let elem: BitMapElement<_> = (
        (-m_to_nm(115.0 * 1.0 / 3.0), m_to_nm(115.0 / 2.0)),
        img_carrier_top,
    )
        .into();
    chart.draw_series(std::iter::once(elem)).unwrap();

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
        let y = deg.to_radians().tan() * TOP_RANGE_X.end;
        chart
            .draw_series(LineSeries::new(
                [(0.0, 0.0), (TOP_RANGE_X.end, y)],
                color.mix(0.4),
            ))
            .unwrap();
        chart
            .draw_series(LineSeries::new(
                [(0.0, 0.0), (TOP_RANGE_X.end, y.neg())],
                color.mix(0.4),
            ))
            .unwrap();
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
        .filter(|d| TOP_RANGE_X.contains(&d.x) && TOP_RANGE_Y.contains(&d.y));

    // draw approach shadow
    chart
        .draw_series(LineSeries::new(
            track_in_nm.clone().map(|d| (d.x, d.y)),
            THEME_BG.stroke_width(4),
        ))
        .unwrap();

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

            chart
                .draw_series(LineSeries::new(
                    points.iter().cloned(),
                    color.stroke_width(2),
                ))
                .unwrap();

            points.clear();
            color = next_color;
        }

        points.push(point);
    }

    if !points.is_empty() {
        chart
            .draw_series(LineSeries::new(
                points.iter().cloned(),
                color.stroke_width(2),
            ))
            .unwrap();
    }

    //
    // --
    //

    let mut chart = ChartBuilder::on(&bottom)
        .margin(5)
        .x_label_area_size(X_LABEL_AREA_SIZE)
        .y_label_area_size(0)
        .build_cartesian_2d(
            CustomRange(SIDE_RANGE_X.with_key_points(vec![0.25f64, 0.5, 0.75, 1.0])),
            SIDE_RANGE_Y,
        )
        .unwrap();

    // Then we can draw a mesh
    chart
        .configure_mesh()
        .disable_mesh()
        .disable_y_axis()
        .axis_style(THEME_FG)
        .x_label_style(text_style.clone())
        .draw()
        .unwrap();

    // carrier side image is 300x150px which corresponds to 115x57.5m
    let img_size = ((115.0 * m2px) as u32, (57.5 * m2px) as u32);
    let img_carrier_side = image::load_from_memory_with_format(
        include_bytes!("../img/carrier-side.png"),
        ImageFormat::Png,
    )
    .unwrap()
    .resize_exact(img_size.0, img_size.1, FilterType::Nearest);
    let elem: BitMapElement<_> = ((-m_to_nm(115.0 * 1.0 / 3.0), 24.0), img_carrier_side).into();
    chart.draw_series(std::iter::once(elem)).unwrap();

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
        let mut x = SIDE_RANGE_X.end;
        let mut y = nm_to_ft(deg.to_radians().tan() * SIDE_RANGE_X.end);
        if y > SIDE_RANGE_Y.end {
            x = ft_to_nm(SIDE_RANGE_Y.end) / deg.to_radians().tan();
            y = SIDE_RANGE_Y.end;
        }
        chart
            .draw_series(LineSeries::new([(0.0, 0.0), (x, y)], color.mix(0.4)))
            .unwrap();
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
        .filter(|d| SIDE_RANGE_X.contains(&d.x) && SIDE_RANGE_Y.contains(&d.alt));

    // draw approach shadow
    chart
        .draw_series(LineSeries::new(
            track_descent.clone().map(|d| (d.x, d.alt)),
            THEME_BG.stroke_width(4),
        ))
        .unwrap();

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

            chart
                .draw_series(LineSeries::new(
                    points.iter().cloned(),
                    color.stroke_width(2),
                ))
                .unwrap();

            points.clear();
            color = next_color;
        }

        points.push(point);
    }

    if !points.is_empty() {
        chart
            .draw_series(LineSeries::new(
                points.iter().cloned(),
                color.stroke_width(2),
            ))
            .unwrap();
    }

    if let Some(grading) = track.grading {
        if let Some(cable) = grading.cable {
            bottom
                .draw_text(&format!("Cable: {}", cable), &text_style, (8, 8))
                .unwrap();
        }
    }
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
            v if (v - 0.25).abs() < f64::EPSILON => "¼nm".to_string(),
            v if (v - 0.50).abs() < f64::EPSILON => "½nm".to_string(),
            v if (v - 0.75).abs() < f64::EPSILON => "¾nm".to_string(),
            _ => format!("{}nm", v),
        }
    }
}
