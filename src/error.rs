use std::fmt;

pub type Description = String;

#[derive(Debug)]
pub enum Error {
    Sync(Description),
    MissingMessage,
    UnknownMessage(String),
    UnexpectedMessage(super::message::Message),
    MessageParse(clap::Error),
    MessgaeHelp(clap::Error),
    MessageVersion(clap::Error),
    CliParse(super::cli::ParseError),
    CliRead(rustyline::error::ReadlineError),
    VirtualTime(Description),
    VirtualTimeStep(Description),
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

impl From<clap::Error> for Error {
    fn from(err: clap::Error) -> Self {
        match &err.kind {
            clap::ErrorKind::HelpDisplayed => Self::MessgaeHelp(err),
            clap::ErrorKind::VersionDisplayed => Self::MessageVersion(err),
            _ => Self::MessageParse(err)
        }
    }
}

impl From<super::cli::ParseError> for Error {
    fn from(err: super::cli::ParseError) -> Self {
        Self::CliParse(err)
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
            Error::MessageParse(err) => write!(f, "[message parse] {}", err),
            Error::MessgaeHelp(help) => write!(f, "[message help] {}", help),
            Error::MessageVersion(version) => write!(f, "[message version] {}", version),
            Error::CliParse(err) => write!(f, "[cli parse] {}", err),
            Error::CliRead(err) => write!(f, "[cli] {}", err),
            Error::VirtualTime(desc) => write!(f, "[message] unable to set virtual time: {}", desc),
            Error::VirtualTimeStep(desc) => write!(f, "[message] unable to set virtual time step: {}", desc),
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