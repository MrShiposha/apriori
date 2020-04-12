use std::fmt;
use nalgebra::{Vector3, Point3};

pub type ObjectName = String;
pub type Coord = f32;
pub type Vector = Vector3<Coord>;
pub type Color = Point3<Coord>;
// pub type Point = Point3<Coord>;

pub enum TimeFormat {
    VirtualTime(chrono::Duration),
    VirtualTimeStep(chrono::Duration),
    FrameDelta(chrono::Duration)
}

impl fmt::Display for TimeFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeFormat::VirtualTime(time) => {
                let days_in_week = 7;
                let hours_in_day = 24;
                let mins_in_hour = 60;
                let secs_in_min = 60;
                let millis_in_sec = 1000;

                if time.num_weeks() != 0 {
                    write!(f, "week #{}, ", time.num_weeks())?;
                }

                if time.num_days() != 0 {
                    write!(f, "day #{}, ", time.num_days() % days_in_week)?;
                }

                write!(
                    f, "{}:{}:{}:{}",
                    time.num_hours() % hours_in_day,
                    time.num_minutes() % mins_in_hour,
                    time.num_seconds() % secs_in_min,
                    time.num_milliseconds() % millis_in_sec
                )
            },
            TimeFormat::VirtualTimeStep(time) => {
                if time.num_weeks() != 0 {
                    write!(f, "{}w", time.num_weeks())
                } else if time.num_days() != 0 {
                    write!(f, "{}d", time.num_days())
                } else if time.num_hours() != 0 {
                    write!(f, "{}h", time.num_hours())
                } else if time.num_minutes() != 0 {
                    write!(f, "{}min", time.num_minutes())
                } else if time.num_seconds() != 0 {
                    write!(f, "{}s", time.num_seconds())
                } else if time.num_milliseconds() != 0 {
                    write!(f, "{}ms", time.num_milliseconds())
                } else {
                    unreachable!()
                }
            },
            TimeFormat::FrameDelta(time) => {
                write!(f, "{}ms", time.num_milliseconds())
            }
        }
    }
}