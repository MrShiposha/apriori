use std::{
    fmt::{self, Write},
    path::PathBuf,
    sync::mpsc::TryRecvError
};
use kiss3d::{
    window::{Window, CanvasSetup, NumSamples},
    scene::SceneNode,
    camera::FirstPerson,
    event::{WindowEvent, Key, Action, Modifiers},
    text::Font
};
use nalgebra::{Point3, Point2};
use log::{
    info,
    error
};
use structopt::StructOpt;
use lazy_static::lazy_static;
use super::{
    message::{self, Message},
    Shared,
    shared_access,
    Result,
    Error,
    cli,
    db,
    r#type::{Color, TimeFormat}
};

const LOG_TARGET: &'static str = "application";
pub const APP_NAME: &'static str = "apriori";

const CLOSE_MESSAGE: &'static str = "Simulation is completed.\nTo close the application, run `shutdown` message.";

const STORAGE_CONNECTION_STRING: &'static str = "host=localhost user=postgres";

lazy_static! {
    pub static ref APP_CLI_PROMPT: String = format!("{}> ", APP_NAME);
    static ref ACCESS_UPDATE_TIME: chrono::Duration = chrono::Duration::seconds(30);
    static ref SESSION_MAX_HANG_TIME: chrono::Duration = chrono::Duration::seconds(
        ACCESS_UPDATE_TIME.num_seconds() + 10
    );
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    Simulating,
    Paused,
    Completed,
    Off
}

impl State {
    pub fn is_run(&self) -> bool {
        !self.is_completed() && !self.is_off()
    }

    pub fn is_completed(&self) -> bool {
        matches![self, State::Completed]
    }

    pub fn is_off(&self) -> bool {
        matches![self, State::Off]
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Simulating => write!(f, "[SIMULATING]"),
            State::Paused => write!(f, "[PAUSED]"),
            State::Completed => write!(f, "[COMPLETED]"),
            State::Off => write!(f, "[OFF]")
        }
    }
}

pub struct App {
    window: Window,
    camera: FirstPerson,
    state: Shared<State>,
    virtual_time: chrono::Duration,
    virtual_time_step: chrono::Duration,
    frame_delta_time: chrono::Duration,
    storage_mgr: db::StorageManager,
    is_stats_enabled: bool,
    frame_deltas_ms_sum: usize, 
    frame_count: usize,
}

impl App {
    pub fn new(log_filter: log::LevelFilter) -> Self {
        super::logger::Logger::init(log_filter)
            .expect("unable to initialize logging system");

        Self {
            window: Window::new_with_setup(
                APP_NAME, 
                800, 
                600, 
                CanvasSetup {
                    vsync: true,
                    samples: NumSamples::Four
                }
            ),
            camera: FirstPerson::new(Point3::new(0.0, 1.0, 0.0), Point3::origin()),
            state: State::Paused.into(),
            virtual_time: chrono::Duration::zero(),
            virtual_time_step: chrono::Duration::seconds(1),
            frame_delta_time: chrono::Duration::milliseconds(0),
            storage_mgr: db::StorageManager::connect(STORAGE_CONNECTION_STRING)
                .expect("unable to connect to storage"),
            is_stats_enabled: true,
            frame_deltas_ms_sum: 0,
            frame_count: 0
        }
    }

    pub fn run(&mut self, history: Option<PathBuf>) {
        if let Err(err) = self.storage_mgr.setup_schema(*SESSION_MAX_HANG_TIME) {
            return error! {
                target: LOG_TARGET,
                "unable to setup apriori schema: {}", err
            };
        }

        let cli = cli::Observer::new(self.state.share(), history);

        loop {
            self.handle_window_events();

            let state = *shared_access![self.state];
            match state {
                State::Simulating => {
                    self.frame_delta_time = chrono::Duration::span(|| self.simulate_frame());
                    self.frame_deltas_ms_sum += self.frame_delta_time.num_milliseconds() as usize;
                    self.render_frame();
                    self.process_console(&cli);
                },
                State::Paused => {
                    self.render_frame();
                    self.process_console(&cli);
                },
                State::Completed => {
                    self.draw_text(CLOSE_MESSAGE, Point2::origin(), Color::new(1.0, 0.0, 0.0));
                    self.process_console(&cli);
                    
                    self.window.render_with_camera(&mut self.camera);
                },
                State::Off => break
            }
        }

        cli.join();
    }

    pub fn handle_message(&mut self, message: Message) -> Result<()> {
        let state = *shared_access![self.state];
        assert_ne!(state, State::Off);

        match message {
            Message::Run(_) 
            | Message::Continue(_)
            | Message::RunShort(_) 
            | Message::ContinueShort(_) if state.is_run() => self.continue_simulation(),
            Message::Pause(_)
            | Message::PauseShort(_) if state.is_run() => self.pause_simulation(),
            Message::Shutdown(_) => self.shutdown(),
            Message::VirtualTimeStep(msg) => self.handle_virtual_time_step(state, msg),
            Message::VirtualTime(msg) => self.handle_virtual_time(state, msg),
            Message::GetFrameDeltaTime(_) => {
                println!("{}", TimeFormat::FrameDelta(self.frame_delta_time));
            },
            Message::GetFrameCount(_) => {
                println!("{}", self.frame_count);
            },
            Message::GetFpms(_) => {
                println!("{}", self.frame_per_ms());
            },
            Message::ListSessions(_) => self.list_sessions(),
            Message::AddObject(msg) if state.is_run()  => self.add_obj(msg),
            unexpected => return Err(Error::UnexpectedMessage(unexpected))
        }

        Ok(())
    }

    fn check_window_opened(&mut self) {
        let state = *shared_access![self.state];
        if self.window.should_close() && matches![
            state, State::Simulating | State::Paused
        ] {
            self.close();
        } 
    }

    fn close(&mut self) {
        error! {
            target: LOG_TARGET,
            "{}", CLOSE_MESSAGE
        }

        *self.window.scene_mut() = SceneNode::new_empty();

        *shared_access![mut self.state] = State::Completed;
    }

    fn shutdown(&mut self) {
        *shared_access![mut self.state] = State::Off;
    }

    fn continue_simulation(&mut self) {
        *shared_access![mut self.state] = State::Simulating;
    }

    fn pause_simulation(&mut self) {
        *shared_access![mut self.state] = State::Paused;
    }

    fn add_obj(&mut self, msg: message::AddObject) {
        println!("new object: '{}'", msg.name);
        println!("location: {}", msg.location);
        
        if let Some(t) = msg.t {
            println!("t: {}", t);
        }

        if let Some(color) = msg.color {
            println!("color: {}", color);
        }
    }

    fn handle_virtual_time_step(&mut self, state: State, msg: message::VirtualTimeStep) {
        match msg.step {
            Some(step) => if state.is_run() { 
                self.virtual_time_step = if msg.reverse {
                    -step
                } else {
                    step
                }
            } else {
                error! {
                    target: LOG_TARGET,
                    "setting virtual time step after the simulation has complete is forbidden"
                }
            },
            None => if msg.reverse {
                println!("{}", TimeFormat::VirtualTimeStep(-self.virtual_time_step));
            } else {
                println!("{}", TimeFormat::VirtualTimeStep(self.virtual_time_step));
            }
        }
    }

    fn handle_virtual_time(&mut self, state: State, msg: message::VirtualTime) {
        let time = if msg.week.is_none()
                    && msg.day.is_none()
                    && msg.hour.is_none()
                    && msg.min.is_none()
                    && msg.sec.is_none()
                    && msg.milli.is_none() {
            if msg.origin {
                chrono::Duration::zero()
            } else if msg.reverse {
                return println!("{}", TimeFormat::VirtualTime(-self.virtual_time));
            } else {
                return println!("{}", TimeFormat::VirtualTime(self.virtual_time));
            }
        } else {
            chrono::Duration::weeks(msg.week.unwrap_or(0))
                + chrono::Duration::days(msg.day.unwrap_or(0))
                + chrono::Duration::hours(msg.hour.unwrap_or(0))
                + chrono::Duration::minutes(msg.min.unwrap_or(0))
                + chrono::Duration::seconds(msg.sec.unwrap_or(0))
                + chrono::Duration::milliseconds(msg.milli.unwrap_or(0))
        };

        if state.is_run() {
            self.virtual_time = if msg.reverse {
                -time
            } else {
                time
            }
        } else {
            error! {
                target: LOG_TARGET,
                "setting virtual time after the simulation has complete is forbidden"
            }
        }
    }

    fn list_sessions(&mut self) {
        println!("\t-- sessions list --");
        if let Err(err) = self.storage_mgr.list_sessions() {
            error! {
                target: LOG_TARGET,
                "unable to list sessions: {}", err
            }
        }
    }

    fn draw_stats(&mut self) {
        if self.is_stats_enabled {
            self.draw_state_text();
            self.draw_simulation_stats();
        }
    }

    fn render_frame(&mut self) {
        self.check_window_opened();

        self.window.render_with_camera(&mut self.camera);
        self.draw_stats();
    }

    fn simulate_frame(&mut self) {
        let vtime_step = self.virtual_time_step.num_milliseconds() as f32;
        let frame_step = vtime_step * self.frame_delta_time.num_milliseconds() as f32 / 1000.0;
        self.virtual_time = self.virtual_time + chrono::Duration::microseconds((frame_step * 1000.0) as i64);

        // TODO
        std::thread::sleep(std::time::Duration::from_millis(100));

        self.frame_count += 1;
    }

    fn handle_window_events(&mut self) {
        for event in self.window.events().iter() {
            match event.value {
                WindowEvent::Key(key, action, mods) => self.handle_key(key, action, mods),
                _ => {}
            }
        }
    }

    fn handle_key(&mut self, key: Key, action: Action, modifiers: Modifiers) {
        match key {
            Key::P if matches![action, Action::Press] => {
                let state = *shared_access![self.state];
                match state {
                    State::Simulating => self.pause_simulation(),
                    State::Paused => self.continue_simulation(),
                    _ => {}
                }
            },
            Key::C if modifiers.contains(Modifiers::Control) && matches![action, Action::Press] => {
                self.close();
            }
            _ => {}
        }
    }

    fn process_console(&mut self, cli: &cli::Observer) {
        match cli.try_recv() {
            Ok(message) => {
                let message_name = message.get_cli_name();

                match self.handle_message(message) {
                    Ok(()) => info! {
                        target: LOG_TARGET,
                        "`{}` succeeded", message_name
                    },
                    Err(err) => error! {
                        target: LOG_TARGET,
                        "{}", err
                    }
                }
            },
            Err(TryRecvError::Empty) => {},
            Err(err) => error! {
                target: LOG_TARGET,
                "{}", err
            }
        }
    }

    pub fn frame_per_ms(&self) -> f32 {
        self.frame_deltas_ms_sum as f32 / self.frame_count as f32
    }

    fn draw_state_text(&mut self) {
        let state = *shared_access![self.state];
        self.draw_text(
            format!("{}", state).as_ref(), 
            Point2::origin(),
            Color::new(1.0, 1.0, 1.0)
        );
    }

    fn draw_simulation_stats(&mut self) {
        let pos = Point2::new(0.0, 150.0);

        let mut stats_text = String::new();
        
        writeln!(
            &mut stats_text,
            "frame #{}", self.frame_count
        ).unwrap();

        writeln!(
            &mut stats_text, 
            "virtual time: {}", 
            TimeFormat::VirtualTime(self.virtual_time)
        ).unwrap();

        writeln!(
            &mut stats_text, 
            "virtual time step: {}",
            TimeFormat::VirtualTimeStep(self.virtual_time_step)
        ).unwrap();

        writeln!(
            &mut stats_text,
            "frame delta time: {}",
            TimeFormat::FrameDelta(self.frame_delta_time)
        ).unwrap();

        writeln!(
            &mut stats_text,
            "frame per ms: {}",
            self.frame_per_ms()
        ).unwrap();

        self.draw_text(
            &stats_text,
            pos,
            Color::new(1.0, 0.0, 1.0)
        );
    }

    fn draw_text(&mut self, text: &str, pos: Point2<f32>, color: Color) {
        let scale = 75.0;
        let font = Font::default();
    
        self.window.draw_text(
            text, 
            &pos, 
            scale, 
            &font, 
            &color
        );
    }
}

#[derive(StructOpt)]
pub struct Options {
    /// File with command history
    #[structopt(long)]
    pub history_file: Option<PathBuf>,

    /// Log level filter
    #[structopt(short, long, default_value = "warn")]
    pub log_filter: log::LevelFilter
}