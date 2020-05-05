use super::{
    message,
    r#type::{ObjectName, AttractorName, TimeFormat},
};
use std::fmt;

pub type Description = String;

#[macro_export]
macro_rules! make_error {
    ($($path:ident)::+$(($value:expr))?) => {
        $crate::make_error![@_impl $($path)::+$(($value))?]
    };

    (@_impl $err_enum:ident::$case:ident$(($value:expr))?) => {
        $crate::error::$err_enum::$case$(($value))?
    };

    (@_impl $err_enum:ident::$sub_err_enum:ident::$($err_tail:ident)::+$(($value:expr))?) => {
        $crate::error::$err_enum::$sub_err_enum(
            $crate::make_error![@_impl $sub_err_enum::$($err_tail)::+$(($value))?]
        )
    };
}

#[derive(Debug)]
pub enum Error {
    Sync(Description),
    Io(std::io::Error),
    MissingMessage,
    UnknownMessage(String),
    UnexpectedMessage(super::message::Message),
    MessageHelp(clap::Error),
    MessageVersion(clap::Error),
    Parse(Parse),
    CliRead(rustyline::error::ReadlineError),
    VirtualTime(Description),
    Storage(Storage),
    Scene(Scene),
    Physics(Physics),
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
    MasterStorageRaw(postgres::Error),
    OccupiedSpacesRaw(rusqlite::Error),
    SetupSchema(postgres::Error),
    SessionCreate(postgres::Error),
    SessionUpdateAccessTime(postgres::Error),
    SessionSave(postgres::Error),
    SessionLoad(postgres::Error),
    SessionRename(postgres::Error),
    SessionUnlock(postgres::Error),
    SessionList(postgres::Error),
    SessionGet(postgres::Error),
    SessionDelete(postgres::Error),
    AddObject(postgres::Error),
    RenameObject(postgres::Error),
    ObjectList(postgres::Error),
    OccupiedSpacesStorageInit(rusqlite::Error),
    AddOccupiedSpace(rusqlite::Error),
    CheckPossibleCollisions(rusqlite::Error),
    ReadOccupiedSpace(rusqlite::Error),
}

#[derive(Debug)]
pub enum Scene {
    UncomputedTrackPart(chrono::Duration),
    ObjectAlreadyExists(ObjectName),
    ObjectNotFound(ObjectName),
    AttractorAlreadyExists(AttractorName),
}

#[derive(Debug)]
pub enum Physics {
    Init(rusqlite::Error),
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

impl From<rustyline::error::ReadlineError> for Error {
    fn from(err: rustyline::error::ReadlineError) -> Self {
        Self::CliRead(err)
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
            Error::Scene(err) => write!(f, "[scene] {}", err),
            Error::Physics(err) => write!(f, "[physics] {}", err),
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
        Self::MasterStorageRaw(err)
    }
}

impl From<rusqlite::Error> for Storage {
    fn from(err: rusqlite::Error) -> Self {
        Self::OccupiedSpacesRaw(err)
    }
}

impl From<postgres::Error> for Error {
    fn from(err: postgres::Error) -> Self {
        Self::Storage(err.into())
    }
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Self::Storage(err.into())
    }
}

impl fmt::Display for Storage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MasterStorageRaw(err) => write!(f, "[master] {}", err),
            Self::OccupiedSpacesRaw(err) => write!(f, "[oss] {}", err),
            Self::SetupSchema(err) => write!(f, "unable to setup schema: {}", err),
            Self::SessionCreate(err) => write!(f, "unable to create new session: {}", err),
            Self::SessionUpdateAccessTime(err) => {
                write!(f, "unable to update session access time: {}", err)
            }
            Self::SessionSave(err) => write!(f, "unable to save the session: {}", err),
            Self::SessionLoad(err) => write!(f, "unable to load the session: {}", err),
            Self::SessionRename(err) => write!(f, "unable to find the session: {}", err),
            Self::SessionUnlock(err) => write!(f, "unable to unlock the session: {}", err),
            Self::SessionList(err) => write!(f, "unable to display session list: {}", err),
            Self::SessionGet(err) => write!(f, "unable to display current session: {}", err),
            Self::SessionDelete(err) => write!(f, "unable to delete the session: {}", err),
            Self::AddObject(err) => write!(f, "unable to add object to the scene: {}", err),
            Self::RenameObject(err) => write!(f, "unable to rename object to the scene: {}", err),
            Self::ObjectList(err) => write!(f, "unable to display object list: {}", err),
            Self::OccupiedSpacesStorageInit(err) => write!(f, "unable to init OSS: {}", err),
            Self::AddOccupiedSpace(err) => write!(f, "unable to add occupied space: {}", err),
            Self::CheckPossibleCollisions(err) => write!(f, "unable to check possible collisions: {}", err),
            Self::ReadOccupiedSpace(err) => write!(f, "unable to read occupied space: {}", err),
        }
    }
}

impl From<Scene> for Error {
    fn from(err: Scene) -> Self {
        Self::Scene(err)
    }
}

impl fmt::Display for Scene {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UncomputedTrackPart(when) => write!(
                f,
                "`{}`: uncomputed track part",
                TimeFormat::VirtualTimeShort(*when)
            ),
            Self::ObjectAlreadyExists(obj_name) => {
                write!(f, "`{}`: object already exists", obj_name)
            },
            Self::ObjectNotFound(obj_name) => {
                write!(f, "`{}`: object not found", obj_name)
            },
            Self::AttractorAlreadyExists(attr_name) => {
                write!(f, "`{}`: attractor already exists", attr_name)
            },
        }
    }
}

impl From<Physics> for Error {
    fn from(err: Physics) -> Self {
        Self::Physics(err)
    }
}

impl fmt::Display for Physics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Init(err) => write!(f, "unable to init physics: {}", err),
        }
    }
}
