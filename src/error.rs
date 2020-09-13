use super::{
    message,
    r#type::{LayerName, ObjectName, TimeFormat, Vector},
    engine::context::TimeRange,
};
use std::fmt;

pub type Description = String;

#[macro_export]
macro_rules! make_error {
    ($($path:ident)::+$(($($value:expr),+))?) => {
        $crate::make_error![@_impl $($path)::+$(($($value),+))?]
    };

    (@_impl $err_enum:ident::$case:ident$(($($value:expr),+))?) => {
        $crate::error::$err_enum::$case$(($($value),+))?
    };

    (@_impl $err_enum:ident::$sub_err_enum:ident::$($err_tail:ident)::+$(($($value:expr),+))?) => {
        $crate::error::$err_enum::$sub_err_enum(
            $crate::make_error![@_impl $sub_err_enum::$($err_tail)::+$(($($value),+))?]
        )
    };
}

#[derive(Debug)]
pub enum Error {
    Sync(Description),
    Io(std::io::Error),
    ConnectionPool(r2d2::Error),
    Layer(Layer),
    ContextUpdateInterrupted,
    ObjectsNotComputed(TimeRange),
    MissingMessage,
    UnknownMessage(String),
    UnexpectedMessage(super::message::Message),
    MessageHelp(clap::Error),
    MessageVersion(clap::Error),
    Parse(Parse),
    CliRead(rustyline::error::ReadlineError),
    VirtualTime(Description),
    Storage(Storage),
    SerializeCSV(csv::Error),
    WriterCSV(String),
    Interpolation(Interpolation)
}

#[derive(Debug)]
pub enum Layer {
    LayerAlreadyExists(LayerName),
    LayerNotFound(LayerName),
    ObjectNotFound(ObjectName),
    ObjectAlreadyAdded(ObjectName),
    ObjectAlreadyExists(ObjectName),
}

#[derive(Debug)]
pub enum Interpolation {
    FutureObject,
    ObjectIsNotComputed(Vector),
    NoTrackParts,
}

#[derive(Debug)]
pub enum Parse {
    Message(clap::Error),
    Vector(Description),
    Color(css_color_parser::ColorParseError),
    Time(Description),
    Regex(regex::Error),
}

#[derive(Debug)]
pub enum Storage {
    Raw(postgres::Error),
    SetupSchema(postgres::Error),
    Transaction(postgres::Error),
    Session(postgres::Error),
    Layer(postgres::Error),
    Object(postgres::Error),
    Location(postgres::Error),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<clap::Error> for Error {
    fn from(err: clap::Error) -> Self {
        match &err.kind {
            clap::ErrorKind::HelpDisplayed => Self::MessageHelp(err),
            clap::ErrorKind::VersionDisplayed => Self::MessageVersion(err),
            _ => Self::Parse(err.into()),
        }
    }
}

impl From<Parse> for Error {
    fn from(err: Parse) -> Self {
        Self::Parse(err)
    }
}

impl From<Layer> for Error {
    fn from(err: Layer) -> Self {
        Self::Layer(err)
    }
}

impl From<rustyline::error::ReadlineError> for Error {
    fn from(err: rustyline::error::ReadlineError) -> Self {
        Self::CliRead(err)
    }
}

impl From<r2d2::Error> for Error {
    fn from(err: r2d2::Error) -> Self {
        Self::ConnectionPool(err)
    }
}

impl<T, E: From<Error>> Into<::std::result::Result<T, E>> for Error {
    fn into(self) -> ::std::result::Result<T, E> {
        Err(self.into())
    }
}

impl Into<()> for Error {
    fn into(self) {}
}

impl Into<bool> for Error {
    fn into(self) -> bool {
        false
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Sync(desc) => write!(f, "[sync] {}", desc),
            Error::Io(err) => write!(f, "[io] {}", err),
            Error::ConnectionPool(err) => write!(f, "[connection pool] {}", err),
            Error::Layer(err) => write!(f, "[layer] {}", err),
            Error::Interpolation(err) => write!(f, "[interpolation]: {}", err),
            Error::ContextUpdateInterrupted => write!(f, "context update was interrupted"),
            Error::ObjectsNotComputed(time_range) => {
                write!(
                    f, "some objects are not computed in the range [{}; {}]",
                    TimeFormat::VirtualTimeShort(time_range.start()),
                    TimeFormat::VirtualTimeShort(time_range.end())
                )
            }
            Error::MissingMessage => write!(f, "[missing message]"),
            Error::UnknownMessage(msg) => write!(f, "[unknown message] {}", msg),
            Error::UnexpectedMessage(msg) => {
                write!(f, "[unexpected message] {}", msg.get_cli_name())
            }
            Error::MessageHelp(help) => write!(f, "[message help] {}", help),
            Error::MessageVersion(version) => write!(f, "[message version] {}", version),
            Error::Parse(err) => write!(f, "[parse] {}", err),
            Error::CliRead(err) => write!(f, "[cli] {}", err),
            Error::VirtualTime(desc) => write!(f, "[virtual time] {}", desc),
            Error::Storage(err) => write!(f, "[storage] {}", err),
            Error::SerializeCSV(err) => write!(f, "[serialization csv] {}", err),
            Error::WriterCSV(err) => write!(f, "[write csv] {}", err),
        }
    }
}

impl From<clap::Error> for Parse {
    fn from(err: clap::Error) -> Self {
        Self::Message(err)
    }
}

impl From<regex::Error> for Parse {
    fn from(err: regex::Error) -> Self {
        Self::Regex(err)
    }
}

impl From<css_color_parser::ColorParseError> for Parse {
    fn from(err: css_color_parser::ColorParseError) -> Self {
        Self::Color(err)
    }
}

impl From<css_color_parser::ColorParseError> for Error {
    fn from(err: css_color_parser::ColorParseError) -> Self {
        Self::Parse(err.into())
    }
}

impl From<regex::Error> for Error {
    fn from(err: regex::Error) -> Self {
        Self::Parse(err.into())
    }
}

impl fmt::Display for Layer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LayerAlreadyExists(name) => write!(f, "layer \"{}\" already exists", name),
            Self::ObjectNotFound(name) => write!(f, "object \"{}\" is not found in the layer", name),
            Self::LayerNotFound(name) => write!(f, "layer \"{}\" is not found", name),
            Self::ObjectAlreadyAdded(name) => write!(f, "object \"{}\" already added into the layer", name),
            Self::ObjectAlreadyExists(name) => write!(f, "pbject \"{}\" alredy exists in the session", name),
        }
    }
}

impl fmt::Display for Interpolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FutureObject => write!(f, "object has not appeared yet"),
            Self::ObjectIsNotComputed(vector) => write!(
                f,
                "object is not yet computed, last computed location: {{{}, {}, {}}}",
                vector[0], vector[1], vector[2],
            ),
            Self::NoTrackParts => write!(f, "object has no track parts"),
        }
    }
}

impl fmt::Display for Parse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(err) => write!(f, "{}", err),
            Self::Vector(desc) => write!(f, "unable to parse vector: {}\nHINT: vector format is {{x}},{{y}},{{z}}", desc),
            Self::Color(err) => write!(f, "unable to parse color: {}\nHINT: this app uses CSS color format", err),
            Self::Time(desc) => write!(
                f, "unable to parse time: {}\nHINT: type `{}` to achieve information on how to input a time data.",
                desc,
                message::TimeFormat::get_cli_name()
            ),
            Self::Regex(err) => write!(f, "unable to compile regex: {}", err),
        }
    }
}

impl From<postgres::Error> for Storage {
    fn from(err: postgres::Error) -> Self {
        Self::Raw(err)
    }
}

impl From<postgres::Error> for Error {
    fn from(err: postgres::Error) -> Self {
        Self::Storage(err.into())
    }
}

impl fmt::Display for Storage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Raw(err) => write!(f, "[storage] {}", err),
            Self::SetupSchema(err) => write!(f, "unable to setup schema: {}", err),
            Self::Transaction(err) => write!(f, "unable to start the transaction: {}", err),
            Self::Session(err) => write!(f, "session error: {}", err),
            Self::Layer(err) => write!(f, "layer error: {}", err),
            Self::Object(err) => write!(f, "object error: {}", err),
            Self::Location(err) => write!(f, "location error: {}", err),
        }
    }
}
