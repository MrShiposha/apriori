use std::fmt;

pub type Description = String;

#[derive(Debug)]
pub enum Error {
    Sync(Description),
    MissingMessage,
    UnknownMessage(String),
    UnexpectedMessage(super::message::Message),
    MessageHelp(clap::Error),
    MessageVersion(clap::Error),
    Parse(ParseError),
    CliRead(rustyline::error::ReadlineError),
    VirtualTime(Description),
    Storage(postgres::Error),
    SetupSchema(postgres::Error),
    SessionCreate(postgres::Error),
    SessionUpdateAccessTime(postgres::Error),
    SessionSave(Description),
    SessionLoad(postgres::Error),
    SessionRename(postgres::Error),
    SessionUnlock(postgres::Error),
    SessionList(postgres::Error),
    SessionGet(postgres::Error),
    SessionDelete(postgres::Error),
}

#[derive(Debug)]
pub enum ParseError {
    Message(clap::Error),
    Vector(Description),
    Color(css_color_parser::ColorParseError),
    Time(Description),
}

impl From<clap::Error> for Error {
    fn from(err: clap::Error) -> Self {
        match &err.kind {
            clap::ErrorKind::HelpDisplayed => Self::MessageHelp(err),
            clap::ErrorKind::VersionDisplayed => Self::MessageVersion(err),
            _ => Self::Parse(err.into())
        }
    }
}

impl From<ParseError> for Error {
    fn from(err: ParseError) -> Self {
        Self::Parse(err)
    }
}

impl From<rustyline::error::ReadlineError> for Error {
    fn from(err: rustyline::error::ReadlineError) -> Self {
        Self::CliRead(err)
    }
}

impl From<postgres::Error> for Error {
    fn from(err: postgres::Error) -> Self {
        Self::Storage(err)
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
            Error::MissingMessage => write!(f, "[missing message]"),
            Error::UnknownMessage(msg) => write!(f, "[unknown message] {}", msg),
            Error::UnexpectedMessage(msg) => write!(f, "[unexpected message] {}", msg.get_cli_name()),
            Error::MessageHelp(help) => write!(f, "[message help] {}", help),
            Error::MessageVersion(version) => write!(f, "[message version] {}", version),
            Error::Parse(err) => write!(f, "[parse] {}", err),
            Error::CliRead(err) => write!(f, "[cli] {}", err),
            Error::VirtualTime(desc) => write!(f, "[virtual time] {}", desc),
            Error::Storage(err) => write!(f, "[storage] {}", err),
            Error::SetupSchema(err) => write!(f, "[storage] unable to setup schema: {}", err),
            Error::SessionCreate(err) => write!(f, "[stirage] unable to create new session: {}", err),
            Error::SessionUpdateAccessTime(err) => write!(f, "[storage] unable to update session access time: {}", err),
            Error::SessionSave(desc) => write!(f, "[storage] unable to save the session: {}", desc),
            Error::SessionLoad(desc) => write!(f, "[storage] unable to load the session: {}", desc),
            Error::SessionRename(err) => write!(f, "[storage] unable to find the session: {}", err),
            Error::SessionUnlock(err) => write!(f, "[storage] unable to unlock the session: {}", err),
            Error::SessionList(err) => write!(f, "[storage] unable to display session list: {}", err),
            Error::SessionGet(err) => write!(f, "[storage] unable to display current session: {}", err),
            Error::SessionDelete(err) => write!(f, "[storage] unable to delete the session: {}", err),
        }
    }
}

impl From<clap::Error> for ParseError {
    fn from(err: clap::Error) -> Self {
        Self::Message(err)
    }
}

impl From<css_color_parser::ColorParseError> for ParseError {
    fn from(err: css_color_parser::ColorParseError) -> Self {
        Self::Color(err)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(err) => write!(f, "unable to parse message: {}", err),
            Self::Vector(desc) => write!(f, "unable to parse vector: {}", desc),
            Self::Color(err) => write!(f, "unable to parse color: {}", err),
            Self::Time(desc) => write!(f, "unable to parse time unit: {}", desc)
        }
    }
}