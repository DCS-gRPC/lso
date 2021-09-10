use std::collections::HashMap;
use std::fs::File;
use std::ops::Neg;
use std::time::Instant;

use chrono::{DateTime, Duration, FixedOffset, Utc};
use lso::data;
use plotters::coord::combinators::WithKeyPoints;
use plotters::coord::ranged1d::ValueFormatter;
use plotters::coord::types::RangedCoordf64;
use plotters::prelude::*;
use tacview::record::{Coords, GlobalProperty, Property, Record, Tag};
use tacview::TacviewParser;
use ultraviolet::{DRotor3, DVec3};

const THEME_BG: RGBColor = RGBColor(30, 41, 49);
const THEME_FG: RGBColor = RGBColor(203, 213, 225);

const THEME_GUIDE_RED: RGBColor = RGBColor(248, 113, 113);
const THEME_GUIDE_YELLOW: RGBColor = RGBColor(250, 204, 21);
const THEME_GUIDE_GREEN: RGBColor = RGBColor(163, 230, 53);
const THEME_GUIDE_GRAY: RGBColor = RGBColor(100, 116, 139);

const THEME_TRACK_RED: RGBColor = RGBColor(248, 113, 113);
const THEME_TRACK_YELLOW: RGBColor = RGBColor(250, 204, 21);
const THEME_TRACK_GREEN: RGBColor = RGBColor(132, 230, 53);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // let filename = std::env::args().nth(1).expect("missing filename");
    let filename = "Tacview-20210906-195356-DCS-2021-09-06 Carrier Qualification.zip.acmi";

    let start = Instant::now();

    let file = File::open(filename)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let file = zip.by_index(0)?;
    let parser = TacviewParser::new(file)?;

    let mut reference_time: DateTime<FixedOffset> = Utc::now().into();
    let mut time = reference_time;
    let mut carriers: HashMap<u64, State> = HashMap::new();
    let mut planes: HashMap<u64, State> = HashMap::new();
    let mut tracks: HashMap<u64, Track> = HashMap::new();

    for record in parser {
        // println!("{:?}", record?);
        match record? {
            // forward time
            Record::Frame(secs) => {
                for (plane_id, plane) in &mut planes {
                    if !plane.dirty {
                        continue;
                    }

                    plane.dirty = false;

                    if let Some(track) = tracks.get_mut(plane_id) {
                        let carrier = match carriers.get(&track.carrier_id) {
                            Some(carrier) => carrier,
                            None => {
                                tracks.remove(plane_id);
                                continue;
                            }
                        };

                        // TODO: don't re-calc everytime
                        let mut landing_pos_offset =
                            data::NIMITZ.optimal_landing_offset(&data::FA18C, 3.5);
                        let carrier_rot =
                            DRotor3::from_rotation_xz((carrier.yaw).neg().to_radians());
                        landing_pos_offset.rotate_by(carrier_rot);

                        let landing_pos = carrier.interpolate_position(time) + landing_pos_offset;

                        let ray_from_plane_to_carrier = DVec3::new(
                            landing_pos.x - plane.pos.x,
                            0.0, // ignore altitude
                            landing_pos.z - plane.pos.z,
                        );

                        let distance = ray_from_plane_to_carrier.mag();
                        // println!("{} - {}", time, distance);

                        // stop tracking
                        let mut fb = DVec3::unit_z();
                        fb.rotate_by(carrier_rot);
                        if ray_from_plane_to_carrier.dot(fb) < 0.0 {
                            println!(
                                "Stopped tracking at {} (distance = {}nm)",
                                time,
                                m_to_nm(distance)
                            );
                            if let Some(track) = tracks.remove(plane_id) {
                                eprint!("Drawing");
                                draw_chart(track);
                            }

                            return Ok(());
                            continue;
                        }

                        // construct the x axis, which is aligned to the angeld deck
                        // TODO: fix origin of angled deck
                        let mut fb = DVec3::unit_z();
                        let fb_rot = DRotor3::from_rotation_xz(
                            (carrier.yaw - data::NIMITZ.deck_angle).neg().to_radians(),
                        );
                        fb.rotate_by(fb_rot);

                        let x = ray_from_plane_to_carrier.dot(fb);
                        let mut y = (distance.powi(2) - x.powi(2)).sqrt();

                        // determine whether plane is left or right of the glideslope
                        let mut a = DVec3::unit_x();
                        a.rotate_by(fb_rot);
                        if ray_from_plane_to_carrier.dot(a) > 0.0 {
                            y = y.neg();
                        }

                        if let Some(last) = track.datums.last() {
                            // ignore data point if position hasn't changed
                            let epsilon = 0.01;
                            if (last.x - x).abs() < epsilon && (last.y - y).abs() < epsilon {
                                continue;
                            }
                        }

                        // println!("{} - {}, {}", time, x, y);

                        dbg!(plane.aoa);

                        track.datums.push(Datum {
                            time,
                            x,
                            y,
                            aoa: plane.aoa,
                            alt: plane.pos.y - data::NIMITZ.deck_altitude,
                        });
                    } else {
                        // check whether they seem to start a recovery attempt
                        for (carrier_id, carrier) in &carriers {
                            if plane.pilot.as_deref() != Some("Binary") {
                                continue;
                            }

                            // ignore planes above 500ft
                            if m_to_ft(plane.pos.y) > 500.0 {
                                continue;
                            }

                            let carrier_pos = carrier.interpolate_position(time);
                            let mut ray_from_plane_to_carrier = DVec3::new(
                                carrier_pos.x - plane.pos.x,
                                carrier_pos.y - plane.pos.y,
                                carrier_pos.z - plane.pos.z,
                            );

                            let distance = ray_from_plane_to_carrier.mag();

                            // ignore planes farther away than 1.5nm
                            if m_to_nm(distance) > 1.5 {
                                continue;
                            }

                            // ignore takeoffs
                            if m_to_nm(distance) < 0.3 {
                                continue;
                            }

                            ray_from_plane_to_carrier.normalize();

                            // Does the nose of the plane roughly point towards the carrier?
                            let dot = plane.dir.dot(ray_from_plane_to_carrier);
                            if dot < 0.65 {
                                continue;
                            }

                            println!(
                                "Found recovery attempt at {} (dot = {}, distance = {}nm)",
                                time,
                                dot,
                                m_to_nm(distance)
                            );

                            tracks.insert(
                                *plane_id,
                                Track {
                                    carrier_id: *carrier_id,
                                    datums: Vec::new(),
                                },
                            );
                        }
                    }
                }

                time = reference_time + Duration::milliseconds((secs * 1000.0) as i64);
            }

            // handle global properties
            Record::GlobalProperty(GlobalProperty::ReferenceTime(s)) => {
                reference_time = DateTime::parse_from_rfc3339(&s)?;
                time = reference_time.clone();
            }

            Record::Update(update) => {
                if !carriers.contains_key(&update.id) {
                    for p in &update.props {
                        if let Property::Type(tags) = p {
                            if tags.contains(&Tag::AircraftCarrier) {
                                carriers.insert(update.id, Default::default());
                            } else if tags.contains(&Tag::FixedWing) {
                                planes.insert(update.id, Default::default());
                            }
                        }
                    }
                }

                if let Some(carrier) = carriers.get_mut(&update.id) {
                    carrier.update_from_props(update.props, time, false);
                } else if let Some(plane) = planes.get_mut(&update.id) {
                    plane.update_from_props(update.props, time, tracks.contains_key(&update.id));
                }
            }
            _ => {}
        }
    }

    dbg!(carriers);
    // dbg!(planes);

    println!("Took: {:.4}s", start.elapsed().as_secs_f64());

    Ok(())
}

fn m_to_nm(m: f64) -> f64 {
    m / 1852.0
}

fn nm_to_m(nm: f64) -> f64 {
    nm * 1852.0
}

fn m_to_ft(m: f64) -> f64 {
    m * 3.28084
}

fn ft_to_m(ft: f64) -> f64 {
    ft / 3.28084
}

fn ft_to_nm(ft: f64) -> f64 {
    ft / 6076.118
}

#[derive(Debug, Default)]
struct State {
    name: Option<String>,
    pilot: Option<String>,
    pos: DVec3,
    /// Unit: m/s
    speed: f64,
    dir: DVec3,
    roll: f64,
    pitch: f64,
    yaw: f64,
    last_updated: Option<DateTime<FixedOffset>>,
    dirty: bool,
    aoa: f64,
    velocity: DVec3,
}

#[derive(Debug)]
struct Track {
    carrier_id: u64,
    datums: Vec<Datum>,
}

#[derive(Debug)]
struct Datum {
    time: DateTime<FixedOffset>,
    x: f64,
    y: f64,
    aoa: f64,
    alt: f64,
}

impl State {
    fn update_from_props(&mut self, props: Vec<Property>, time: DateTime<FixedOffset>, a: bool) {
        for p in props {
            match p {
                Property::T(coords) => {
                    self.update(&coords, time, a);
                    self.dirty = true;
                }
                Property::Name(name) => {
                    self.name = Some(name);
                    self.dirty = true;
                }
                Property::Pilot(pilot) => {
                    self.pilot = Some(pilot);
                    self.dirty = true;
                }
                // Notes: AOA only updated for recording player (which is whe we calculate it instead)
                // Property::AOA(aoa) => {
                //     self.aoa = aoa;
                // }
                _ => {}
            }
        }
    }

    pub fn update(&mut self, other: &Coords, time: DateTime<FixedOffset>, a: bool) {
        if let Some(roll) = other.roll {
            self.roll = -roll;
        }
        if let Some(pitch) = other.pitch {
            self.pitch = pitch;
        }
        if let Some(heading) = other.heading {
            self.yaw = heading;
        }

        // self.rot = DRotor3::from_euler_angles(self.roll, self.pitch, self.yaw);
        self.dir = DVec3::new(
            self.yaw.to_radians().sin() * self.pitch.to_radians().cos(),
            self.pitch.to_radians().sin(),
            self.yaw.to_radians().cos() * self.pitch.to_radians().cos(),
        );

        let mut new_pos = self.pos;

        if let Some(altitude) = other.altitude {
            new_pos.y = altitude;
        }
        if let Some(u) = other.u {
            new_pos.x = u;
        }
        if let Some(v) = other.v {
            new_pos.z = v;
        }

        if let Some(last_updated) = self.last_updated {
            // update speed if position changed
            if new_pos != self.pos {
                let elapsed = (time - last_updated)
                    .to_std()
                    .unwrap_or(std::time::Duration::ZERO);
                let mut velocity_vector = new_pos - self.pos;
                let distance = velocity_vector.mag();

                self.speed = distance / elapsed.as_secs_f64();

                let mut wind_velocity = DVec3::new(-0.5851, -0.039, -4.384);

                // ---
                if a {
                    wind_velocity *= elapsed.as_secs_f64();

                    dbg!(velocity_vector);
                    dbg!(wind_velocity);
                    velocity_vector -= wind_velocity;
                    dbg!(velocity_vector);

                    velocity_vector.normalize();

                    fn angle_between(a: DVec3, b: DVec3) -> f64 {
                        (a.dot(b) / (a.mag() * b.mag())).acos()
                    }

                    let aoa = angle_between(self.dir, velocity_vector);

                    // let mut nine = DVec3::unit_y();
                    let mut three_line = DVec3::unit_x();
                    three_line.rotate_by(DRotor3::from_euler_angles(
                        //
                        self.roll.to_radians(),        //
                        self.pitch.to_radians().neg(), //
                        self.yaw.to_radians().neg(),
                    ));
                    dbg!(three_line);

                    //     dbg!(self.pos);
                    //     dbg!(new_pos);
                    dbg!(self.dir);
                    dbg!(velocity_vector);
                    dbg!(velocity_vector.dot(three_line));
                    let mut projected_velocity_vector =
                        velocity_vector - (three_line * velocity_vector.dot(three_line));
                    projected_velocity_vector.normalize();
                    dbg!(projected_velocity_vector);

                    dbg!(angle_between(self.dir, projected_velocity_vector));
                    dbg!(angle_between(self.dir, projected_velocity_vector).to_degrees());

                    dbg!(aoa);
                    dbg!(aoa.to_degrees());

                    todo!()
                }

                // self.aoa = aoa.to_degrees();

                // TODO: remove as already done above
                velocity_vector.normalize();

                self.velocity = velocity_vector;
                self.pos = new_pos;
            }
        } else {
            self.pos = new_pos;
        }

        self.last_updated = Some(time);
    }

    fn interpolate_position(&self, time: DateTime<FixedOffset>) -> DVec3 {
        if self.speed == 0.0 {
            return self.pos;
        }

        let last_updated = if let Some(last_updated) = self.last_updated {
            last_updated
        } else {
            return self.pos;
        };

        let elapsed = (time - last_updated)
            .to_std()
            .unwrap_or(std::time::Duration::ZERO);

        if elapsed == std::time::Duration::ZERO {
            return self.pos;
        }

        let delta = self.velocity * (self.speed * elapsed.as_secs_f64());
        // dbg!(self.speed);
        // dbg!(elapsed.as_secs_f64());
        // dbg!(delta);

        // todo!();

        self.pos + delta
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to parse reference time")]
    DateTime(#[from] chrono::ParseError),
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

fn draw_chart(track: Track) {
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
            track.datums.iter().map(|d| (m_to_nm(d.x), m_to_nm(d.y))),
            THEME_BG.stroke_width(4),
        ))
        .unwrap();

    // draw approach
    let mut points = Vec::new();
    let mut color = THEME_TRACK_GREEN;
    for datum in &track.datums {
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
            track.datums.iter().map(|d| (m_to_nm(d.x), m_to_ft(d.alt))),
            THEME_BG.stroke_width(4),
        ))
        .unwrap();

    // draw approach
    let mut points = Vec::new();
    let mut color = THEME_TRACK_GREEN;
    for datum in &track.datums {
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
