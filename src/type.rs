use super::{make_error, Error, Result};
use nalgebra::{Point3, Vector3};
use phf::phf_map;
use std::fmt;

pub type Coord = f32;
pub type ColorChannel = f32;
pub type Vector = Vector3<Coord>;
pub type Color = Point3<ColorChannel>;
pub type PackedColor = i32;
pub type Distance = f32;
pub type Mass = f32;
pub type RawTime = i64;
pub type GravityCoeff = f32;

pub type SessionName = String;
pub type SessionId = i32;
pub type ObjectName = String;
pub type ObjectId = i32;
pub type AttractorName = String;
pub type AttractorId = i32;

const DAYS_IN_WEEK: RawTime = 7;
const HOURS_IN_DAY: RawTime = 24;
const MINS_IN_HOUR: RawTime = 60;
const SECS_IN_MIN: RawTime = 60;
const MILLIS_IN_SEC: RawTime = 1000;

pub enum TimeFormat {
    VirtualTimeLong(chrono::Duration),
    VirtualTimeShort(chrono::Duration),
    FrameDelta(chrono::Duration),
}

impl fmt::Display for TimeFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeFormat::VirtualTimeLong(time) => {
                if time.num_weeks() != 0 {
                    write!(f, "week #{}, ", time.num_weeks())?;
                }

                let days = time.num_days() % DAYS_IN_WEEK;

                if days != 0 {
                    write!(f, "day #{}, ", days)?;
                }

                write!(
                    f,
                    "{}:{}:{}:{}",
                    time.num_hours() % HOURS_IN_DAY,
                    time.num_minutes() % MINS_IN_HOUR,
                    time.num_seconds() % SECS_IN_MIN,
                    time.num_milliseconds() % MILLIS_IN_SEC
                )
            }
            TimeFormat::VirtualTimeShort(time) => {
                if time.is_zero() {
                    return write!(f, "0s");
                }

                let mut prefix = "";

                macro_rules! write_component {
                    ($unit:ident: $component:expr) => {{
                        #![allow(unused_assignments)]

                        let component = $component;

                        if component != 0 {
                            write!(f, concat!["{}{}", stringify![$unit]], prefix, component)?;
                            prefix = ":";
                        }
                    }};
                }

                write_component!(weeks: time.num_weeks());
                write_component!(days: time.num_days() % DAYS_IN_WEEK);
                write_component!(h: time.num_hours() % HOURS_IN_DAY);
                write_component!(min: time.num_minutes() % MINS_IN_HOUR);
                write_component!(s: time.num_seconds() % SECS_IN_MIN);
                write_component!(ms: time.num_milliseconds() % MILLIS_IN_SEC);

                Ok(())
            }
            TimeFormat::FrameDelta(time) => write!(f, "{}ms", time.num_milliseconds()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimeUnit {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Week,
}

macro_rules! register_time_units {
    (
        $($($unit_str:literal)|+ => $unit:ident),+
    ) => {
        static TIME_UNITS: phf::Map<&'static str, TimeUnit> = phf_map! {
            $(
                $(
                    $unit_str => TimeUnit::$unit
                ),+
            ),+
        };

        fn time_units_variants_with_aliases() -> &'static [&'static [&'static str]] {
            &[
                $(
                    &[
                        $($unit_str),+
                    ]
                ),+
            ]
        }
    };
}

register_time_units! {
    "ms" | "milli" | "millis" | "millisecond" | "milliseconds" => Millisecond,
    "s" | "sec" | "secs" | "second" | "seconds" => Second,
    "min" | "mins" | "minute" | "minutes" => Minute,
    "h" | "hour" | "hours" => Hour,
    "d" | "day" | "days" => Day,
    "w" | "week" | "weeks" => Week
}

impl TimeUnit {
    pub fn variants_and_aliases() -> &'static [&'static [&'static str]] {
        time_units_variants_with_aliases()
    }
}

impl std::str::FromStr for TimeUnit {
    type Err = Error;

    fn from_str(time: &str) -> Result<Self> {
        TIME_UNITS
            .get(time)
            .cloned()
            .ok_or(make_error![Error::Parse::Time(format!(
                "`{}`: unexpected time unit",
                time
            ))])
    }
}

impl fmt::Display for TimeUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeUnit::Millisecond => write!(f, "ms"),
            TimeUnit::Second => write!(f, "s"),
            TimeUnit::Minute => write!(f, "min"),
            TimeUnit::Hour => write!(f, "h"),
            TimeUnit::Day => write!(f, "day"),
            TimeUnit::Week => write!(f, "week"),
        }
    }
}
