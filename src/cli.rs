use super::{
    app::{self, APP_NAME},
    make_error,
    message::{self, Message},
    r#type::{Color, ColorChannel, Coord, RawTime, TimeUnit, Vector},
    shared_access, Error, Result, Shared,
};
use css_color_parser::Color as CssColor;
use lazy_static::lazy_static;
use rustyline::{
    completion::Completer,
    config::{self, Config},
    error::ReadlineError,
    highlight::Highlighter,
    hint::Hinter,
    validate::Validator,
    Context,
};
use std::{path::PathBuf, str::FromStr, sync::mpsc, thread};

const LOG_TARGET: &'static str = "CLI";

lazy_static! {
    pub static ref APP_CLI_PROMPT: String = format!("{}> ", APP_NAME);
}

type Editor = rustyline::Editor<Helper>;

pub struct Observer {
    recv: mpsc::Receiver<Message>,
}

impl Observer {
    pub fn new(app_state: Shared<app::State>, history: Option<PathBuf>) -> Self {
        let (sender, receiver) = mpsc::channel();

        thread::spawn(move || {
            let mut editor = default_editor();
            if let Some(history) = history {
                if let Err(err) = editor.load_history(&history) {
                    log::error! {
                        target: LOG_TARGET,
                        "unable to open history file `{}`: {}", history.display(), err
                    }
                }
            }

            message_loop(app_state, editor, sender);
        });

        Self { recv: receiver }
    }

    pub fn try_recv(&self) -> std::result::Result<Message, mpsc::TryRecvError> {
        self.recv.try_recv()
    }
}

pub struct Helper;

impl rustyline::Helper for Helper {}

impl Completer for Helper {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        _pos: usize,
        _: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let candidates = Message::cli_autocomplete::<String>(line);

        Ok((0, candidates))
    }
}

impl Validator for Helper {}
impl Hinter for Helper {}
impl Highlighter for Helper {}

pub fn parse_vector(src: &str) -> Result<Vector> {
    let components = src.split(",").collect::<Vec<_>>();
    if components.len() != 3 {
        return Err(make_error![Error::Parse::Vector(
            "expected 3-dimensional vector".into()
        )]);
    }

    let map_float_parse_err =
        |err: std::num::ParseFloatError| make_error![Error::Parse::Vector(err.to_string())];

    let components = [
        components[0]
            .parse::<Coord>()
            .map_err(map_float_parse_err)?,
        components[1]
            .parse::<Coord>()
            .map_err(map_float_parse_err)?,
        components[2]
            .parse::<Coord>()
            .map_err(map_float_parse_err)?,
    ];

    Ok(Vector::from_row_slice(&components))
}

pub fn parse_color(src: &str) -> Result<Color> {
    let css_color = src
        .parse::<CssColor>()
        .map_err(|err| make_error![Error::Parse::Color(err)])?;

    let color = Color::new(
        css_color.r as ColorChannel / 255.0,
        css_color.g as ColorChannel / 255.0,
        css_color.b as ColorChannel / 255.0,
    );

    Ok(color)
}

pub fn parse_time(src: &str) -> Result<chrono::Duration> {
    macro_rules! parse_error {
        ($fmt:literal $($tt:tt)*) => {
            make_error![Error::Parse::Time(format!($fmt $($tt)*))]
        };
    }

    macro_rules! scan_error {
        ($fmt:literal $($tt:tt)*) => {
            Some(Err(parse_error!($fmt $($tt)*)))
        };

        ($expr:expr) => (Some(Err($expr)));
    }

    macro_rules! scan_ok {
        ($expr:expr) => {
            Some(Ok($expr))
        };
    }

    src.split(':')
        .map(|component| {
            if component.is_empty() {
                return Err(parse_error!("time contain an empty component"));
            }

            component
                .chars()
                .position(|c| c.is_ascii_alphabetic())
                .map(|unit_pos| (&component[..unit_pos], &component[unit_pos..]))
                .ok_or(match component.parse::<RawTime>() {
                    Ok(_) => parse_error!("expected time unit after `{}`", component),
                    Err(err) => parse_error!("`{}`: {}", component, err),
                })
        })
        .scan(
            None,
            |last_time_unit, unit_split_result| match unit_split_result {
                Ok((value, unit)) => {
                    let value = match value.parse::<RawTime>() {
                        Ok(value) => value,
                        Err(err) => return scan_error!("`{}`: {}", value, err),
                    };

                    let time_component = match TimeUnit::from_str(unit) {
                        Ok(time_unit) => {
                            match last_time_unit {
                                Some(last_time_unit) if time_unit >= *last_time_unit => {
                                    return scan_error!("`{}`: unexpected time unit", time_unit);
                                }
                                _ => *last_time_unit = Some(time_unit),
                            }

                            match time_unit {
                                TimeUnit::Millisecond => chrono::Duration::milliseconds(value),
                                TimeUnit::Second => chrono::Duration::seconds(value),
                                TimeUnit::Minute => chrono::Duration::minutes(value),
                                TimeUnit::Hour => chrono::Duration::hours(value),
                                TimeUnit::Day => chrono::Duration::days(value),
                                TimeUnit::Week => chrono::Duration::weeks(value),
                            }
                        }
                        Err(err) => return scan_error!(err),
                    };

                    scan_ok!(time_component)
                }
                Err(err) => scan_error!(err),
            },
        )
        .try_fold(
            chrono::Duration::zero(),
            |time, time_component| -> Result<chrono::Duration> { Ok(time + time_component?) },
        )
}

pub fn default_editor() -> Editor {
    let editor_config = Config::builder()
        .history_ignore_space(true)
        .completion_type(config::CompletionType::List)
        .edit_mode(config::EditMode::Emacs)
        .output_stream(config::OutputStreamType::Stderr)
        .build();

    let mut editor = Editor::with_config(editor_config);
    editor.set_helper(Some(Helper));

    editor
}

fn read_message(editor: &mut Editor) -> super::Result<Message> {
    loop {
        match editor.readline(&APP_CLI_PROMPT) {
            Ok(line) if line.trim().is_empty() => {}
            Ok(line) => {
                let line = line.trim();
                editor.add_history_entry(line);

                return Message::from_str(line);
            }
            Err(ReadlineError::Interrupted) => {
                return Ok(message::Shutdown::default().into());
            }
            Err(err) => {
                return Error::CliRead(err).into();
            }
        }
    }
}

fn message_loop(app_state: Shared<app::State>, mut editor: Editor, sender: mpsc::Sender<Message>) {
    let mut is_completed = false;

    while !is_completed && shared_access![app_state].is_run() {
        match read_message(&mut editor) {
            Ok(message) => {
                is_completed = matches![message, Message::Shutdown(_) | Message::ShutdownShort(_)];
                if let Err(err) = sender.send(message) {
                    log::error! {
                        target: LOG_TARGET,
                        "{}", err
                    }
                }
            }
            Err(Error::MessageHelp(help)) => println!("{}", help),
            Err(Error::MessageVersion(version)) => println!("{}", version),
            Err(err) => log::error! {
                target: LOG_TARGET,
                "{}", err
            },
        }
    }
}
