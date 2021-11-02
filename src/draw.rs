use std::ops::Neg;

use plotters::coord::combinators::WithKeyPoints;
use plotters::coord::ranged1d::ValueFormatter;
use plotters::coord::types::RangedCoordf64;
use plotters::prelude::*;

use crate::utils::{ft_to_nm, m_to_ft, m_to_nm, nm_to_m};

const THEME_BG: RGBColor = RGBColor(30, 41, 49);
const THEME_FG: RGBColor = RGBColor(203, 213, 225);

const THEME_GUIDE_RED: RGBColor = RGBColor(248, 113, 113);
const THEME_GUIDE_YELLOW: RGBColor = RGBColor(250, 204, 21);
const THEME_GUIDE_GREEN: RGBColor = RGBColor(163, 230, 53);
const THEME_GUIDE_GRAY: RGBColor = RGBColor(100, 116, 139);

const THEME_TRACK_RED: RGBColor = RGBColor(248, 113, 113);
const THEME_TRACK_YELLOW: RGBColor = RGBColor(250, 204, 21);
const THEME_TRACK_GREEN: RGBColor = RGBColor(132, 230, 53);

#[derive(Debug)]
pub struct Datum {
    pub x: f64,
    pub y: f64,
    pub aoa: f64,
    pub alt: f64,
}

#[tracing::instrument(skip_all)]
pub fn draw_chart(track: Vec<Datum>) {
    let root_drawing_area = SVGBackend::new("test.svg", (1200, 800 + 500)).into_drawing_area();
    let (top, bottom) = root_drawing_area.split_vertically(800);

    top.fill(&THEME_BG).unwrap();

    let mut chart = ChartBuilder::on(&top)
        .margin(5)
        .x_label_area_size(100)
        .y_label_area_size(100)
        .build_cartesian_2d(
            CustomRange((0.0f64..1.5f64).with_key_points(vec![0.25f64, 0.75, 1.0])),
            -0.5f64..0.5f64,
        )
        .unwrap();

    // Then we can draw a mesh
    chart
        .configure_mesh()
        .disable_mesh()
        .disable_y_axis()
        .axis_style(THEME_FG)
        .x_label_style(TextStyle::from(("sans-serif", 20).into_font()).color(&THEME_FG))
        .draw()
        .unwrap();

    // draw centerline
    let lines = [
        // 0.25degree on center line
        (0.25f64, THEME_GUIDE_GRAY),
        // orange
        (0.75, THEME_GUIDE_GREEN),
        // red
        (4.0, THEME_GUIDE_YELLOW),
        // red
        (6.0, THEME_GUIDE_RED),
    ];

    for (deg, color) in lines {
        let y = deg.to_radians().tan() * 1.5;
        chart
            .draw_series(LineSeries::new([(0.0, 0.0), (1.5, y)], color.mix(0.4)))
            .unwrap();
        chart
            .draw_series(LineSeries::new(
                [(0.0, 0.0), (1.5, y.neg())],
                color.mix(0.4),
            ))
            .unwrap();
    }

    // draw approach shadow
    chart
        .draw_series(LineSeries::new(
            track.iter().map(|d| (m_to_nm(d.x), m_to_nm(d.y))),
            THEME_BG.stroke_width(4),
        ))
        .unwrap();

    // draw approach
    let mut points = Vec::new();
    let mut color = THEME_TRACK_GREEN;
    for datum in &track {
        // https://forums.vrsimulations.com/support/index.php/Navigation_Tutorial_Flight#Angle_of_Attack_Bracket
        let next_color = if datum.aoa <= 6.9 {
            // fast
            THEME_TRACK_RED
        } else if datum.aoa <= 7.4 {
            // slightly fast
            THEME_TRACK_YELLOW
        } else if datum.aoa < 8.8 {
            // on speed
            THEME_TRACK_GREEN
        } else if datum.aoa < 9.3 {
            // slighly slow
            THEME_TRACK_YELLOW
        } else {
            // slow
            THEME_TRACK_RED
        };

        let point = (m_to_nm(datum.x), m_to_nm(datum.y));

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

    bottom.fill(&THEME_BG).unwrap();

    let mut chart = ChartBuilder::on(&bottom)
        .margin(5)
        .x_label_area_size(100)
        .y_label_area_size(100)
        .build_cartesian_2d(
            CustomRange((0.0f64..1.5f64).with_key_points(vec![0.25f64, 0.75, 1.0])),
            0.0f64..500.0f64,
        )
        .unwrap();

    // Then we can draw a mesh
    chart
        .configure_mesh()
        .disable_mesh()
        .disable_y_axis()
        .axis_style(THEME_FG)
        .x_label_style(TextStyle::from(("sans-serif", 20).into_font()).color(&THEME_FG))
        .draw()
        .unwrap();

    // draw centerline
    let lines = [
        (3.5f64 - 0.9, THEME_GUIDE_RED),
        (3.5f64 - 0.6, THEME_GUIDE_YELLOW),
        (3.5f64 - 0.25, THEME_GUIDE_GREEN),
        (3.5f64, THEME_GUIDE_GRAY),
        (3.5f64 + 0.25, THEME_GUIDE_GREEN),
        (3.5f64 + 0.7, THEME_GUIDE_YELLOW),
        (3.5f64 + 1.5, THEME_GUIDE_RED),
    ];

    for (deg, color) in lines {
        let mut x = 1.5;
        let mut y = deg.to_radians().tan() * m_to_ft(nm_to_m(1.5));
        if y > 500.0 {
            x = ft_to_nm(500.0) / deg.to_radians().tan();
            y = 500.0;
        }
        chart
            .draw_series(LineSeries::new([(0.0, 0.0), (x, y)], color.mix(0.4)))
            .unwrap();
    }

    // draw approach shadow
    chart
        .draw_series(LineSeries::new(
            track.iter().map(|d| (m_to_nm(d.x), m_to_ft(d.alt))),
            THEME_BG.stroke_width(4),
        ))
        .unwrap();

    // draw approach
    let mut points = Vec::new();
    let mut color = THEME_TRACK_GREEN;
    for datum in &track {
        // https://forums.vrsimulations.com/support/index.php/Navigation_Tutorial_Flight#Angle_of_Attack_Bracket
        let next_color = if datum.aoa <= 6.9 {
            // fast
            THEME_TRACK_RED
        } else if datum.aoa <= 7.4 {
            // slightly fast
            THEME_TRACK_YELLOW
        } else if datum.aoa < 8.8 {
            // on speed
            THEME_TRACK_GREEN
        } else if datum.aoa < 9.3 {
            // slighly slow
            THEME_TRACK_YELLOW
        } else {
            // slow
            THEME_TRACK_RED
        };

        let point = (m_to_nm(datum.x), m_to_ft(datum.alt));

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
            v if (v - 0.75).abs() < f64::EPSILON => "¾nm".to_string(),
            _ => format!("{}nm", v),
        }
    }
}
