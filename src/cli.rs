use std::{
    thread,
    sync::mpsc,
    str::FromStr,
    fmt,
    path::PathBuf
};
use rustyline::{
    Context,
    config::{self, Config},
    completion::Completer,
    hint::Hinter,
    highlight::Highlighter,
    validate::Validator,
    error::ReadlineError
};
use css_color_parser::{
    Color as CssColor,
    ColorParseError
};
use super::{
    Error,
    Shared,
    shared_access,
    app::{self, APP_CLI_PROMPT},
    error::Description,
    message::{self, Message},
    r#type::{
        Vector,
        Color
    }
};

const LOG_TARGET: &'static str = "CLI";

type Editor = rustyline::Editor<Helper>;

pub struct Observer {
    thread: thread::JoinHandle<()>,
    recv: mpsc::Receiver<Message>
}

impl Observer {
    pub fn new(app_state: Shared<app::State>, history: Option<PathBuf>) -> Self {
        let (sender, receiver) = mpsc::channel();
        
        Self {
            thread: thread::spawn(move || {
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
            }),
            recv: receiver
        }
    }

    pub fn join(self) {
        self.thread.join().unwrap();
    }

    pub fn try_recv(&self) -> Result<Message, mpsc::TryRecvError> {
        self.recv.try_recv()
    }
}

#[derive(Debug)]
pub enum ParseError {
    Vector(Description),
    Color(ColorParseError),
    Time(Description),
}

impl From<std::num::ParseFloatError> for ParseError {
    fn from(err: std::num::ParseFloatError) -> Self {
        Self::Vector(err.to_string())
    }
}

impl From<std::num::ParseIntError> for ParseError {
    fn from(err: std::num::ParseIntError) -> Self {
        Self::Time(err.to_string())
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vector(desc) => write!(f, "unable to parse vector: {}", desc),
            Self::Color(err) => write!(f, "unable to parse color: {}", err),
            Self::Time(desc) => write!(f, "unable to parse time unit: {}", desc)
        }
    }
}

pub struct Helper;

impl rustyline::Helper for Helper {}

impl Completer for Helper {
    type Candidate = String;

    fn complete(&self, line: &str, _pos: usize, _: &Context<'_>) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let candidates = Message::cli_autocomplete::<String>(line);

        Ok((0, candidates))
    }
}

impl Validator for Helper {}
impl Hinter for Helper {}
impl Highlighter for Helper {}

pub fn parse_vector(src: &str) -> Result<Vector, ParseError> {
    let components = src.split(",").collect::<Vec<_>>();
    if components.len() != 3 {
        return Err(ParseError::Vector("expected 3-dimensional vector".into()));
    }

    let components = [
        components[0].parse::<f32>()?,
        components[1].parse::<f32>()?,
        components[2].parse::<f32>()?
    ];

    Ok(Vector::from_row_slice(&components))   
}

pub fn parse_color(src: &str) -> Result<Color, ParseError> {
    let css_color = src.parse::<CssColor>().map_err(|err| ParseError::Color(err))?;

    let color = Color::new(
        css_color.r as f32 / 255.0, 
        css_color.g as f32 / 255.0, 
        css_color.b as f32 / 255.0
    );

    Ok(color)
}

pub fn parse_time(src: &str) -> Result<chrono::Duration, ParseError> {
    let unit_pos = src.chars().position(|c| c.is_ascii_alphabetic())
        .ok_or(ParseError::Time(format!("time unit not found")))?;

    let num = &src[..unit_pos];
    let unit = &src[unit_pos..];

    let num = num.parse()?;

    let time = match unit {
        "ms" => chrono::Duration::milliseconds(num),
        "s" => chrono::Duration::seconds(num),
        "min" => chrono::Duration::minutes(num),
        "h" => chrono::Duration::hours(num),
        "d" => chrono::Duration::days(num),
        "w" => chrono::Duration::weeks(num),
        _ => return Err(ParseError::Time(format!("`{}`: unknown time unit", src)))
    };

    Ok(time)
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
            Ok(line) if line.trim().is_empty() => {},
            Ok(line) => {
                let line = line.trim();
                editor.add_history_entry(line);

                return Message::from_str(line);
            },
            Err(ReadlineError::Interrupted) => {
                return Ok(message::Shutdown::default().into());
            },
            Err(err) => {
                return Error::CliRead(err).into();
            }
        }
    }
}

fn message_loop(app_state: Shared<app::State>, mut editor: Editor, sender: mpsc::Sender<Message>) {
    let mut is_completed = false;

    while !is_completed && !shared_access![app_state].is_off() {
        match read_message(&mut editor) {
            Ok(message) => {
                is_completed = matches![message, Message::Shutdown(_)];
                if let Err(err) = sender.send(message) {
                    log::error! {
                        target: LOG_TARGET,
                        "{}", err
                    }
                }
            },
            Err(err) => log::error! {
                target: LOG_TARGET,
                "{}", err
            }
        }
    }
}