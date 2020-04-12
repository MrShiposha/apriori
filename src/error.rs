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
    PostgreSQL(postgres::Error),
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
        Self::PostgreSQL(err)
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
            Error::PostgreSQL(err) => write!(f, "[postgresql] {}", err),
        }
    }
}