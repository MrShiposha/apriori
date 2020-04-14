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
    trace,
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
    storage::StorageManager,
    graphics,
    r#type::{
        Color, 
        RawTime,
        TimeFormat, 
        SessionId
    }
};

const LOG_TARGET: &'static str = "application";
pub const APP_NAME: &'static str = "apriori";

const CLOSE_MESSAGE: &'static str = "Simulation is completed.\nTo close the application, run `shutdown` message.";

const STORAGE_CONNECTION_STRING: &'static str = "host=localhost user=postgres";

lazy_static! {
    pub static ref APP_CLI_PROMPT: String = format!("{}> ", APP_NAME);
    pub static ref ACCESS_UPDATE_TIME: chrono::Duration = chrono::Duration::seconds(30);
    pub static ref SESSION_MAX_HANG_TIME: chrono::Duration = chrono::Duration::seconds(
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
    real_time: chrono::Duration,
    last_session_update_time: chrono::Duration,
    virtual_time: chrono::Duration,
    virtual_time_step: chrono::Duration,
    frame_delta_time: chrono::Duration,
    is_stats_enabled: bool,
    frame_deltas_ms_sum: usize, 
    frame_count: usize,
    storage_mgr: StorageManager,
    session_id: SessionId,
    unnamed_object_index: usize,
}

impl App {
    pub fn new(log_filter: log::LevelFilter) -> Self {
        super::logger::Logger::init(log_filter)
            .expect("unable to initialize logging system");

        let mut storage_mgr = StorageManager::setup(STORAGE_CONNECTION_STRING)
            .expect("unable to connect to storage");

        let default_name = None;
        let session_id = storage_mgr.session().new(default_name)
            .expect("unable to create new session");

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
            real_time: chrono::Duration::zero(),
            last_session_update_time: chrono::Duration::zero(),
            virtual_time: chrono::Duration::zero(),
            virtual_time_step: chrono::Duration::seconds(1),
            frame_delta_time: chrono::Duration::milliseconds(0),
            is_stats_enabled: true,
            frame_deltas_ms_sum: 0,
            frame_count: 0,
            storage_mgr,
            session_id,
            unnamed_object_index: 0
        }
    }

    pub fn run(&mut self, history: Option<PathBuf>) {
        let cli = cli::Observer::new(self.state.share(), history);

        loop {
            let loop_begin = time::precise_time_ns();

            if let Err(err) = self.update_session_access_time() {
                error! {
                    target: LOG_TARGET,
                    "{}", err
                }
            }

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

            let loop_end = time::precise_time_ns();
            let loop_time = loop_end - loop_begin;
            self.real_time = self.real_time + chrono::Duration::nanoseconds(loop_time as RawTime);
        }

        cli.join();
    }

    pub fn handle_message(&mut self, message: Message) -> Result<()> {
        let state = *shared_access![self.state];
        assert_ne!(state, State::Off);

        match message {
            Message::GlobalHelp(_)
            | Message::GlobalHelpShort(_) => {
                let max_name = Message::cli_list().iter()
                    .map(|(name, _)| name.len())
                    .max()
                    .unwrap();

                for (name, about) in Message::cli_list() {
                    print!("\t{:<width$}", name, width = max_name);
                    match about {
                        Some(about) => println!("  // {}", about),
                        None => println!()
                    }
                }

                Ok(())
            },
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
                Ok(())
            },
            Message::GetFrameCount(_) => {
                println!("{}", self.frame_count);
                Ok(())
            },
            Message::GetFpms(_) => {
                println!("{}", self.frame_per_ms());
                Ok(())
            },
            Message::ListSessions(_) => {
                println!("\t-- sessions list --");
                self.storage_mgr.session().print_list()
            },
            Message::GetSession(_) => {
                self.storage_mgr.session().get(self.session_id)
            }
            Message::NewSession(msg) => {
                self.handle_new_session(msg)
            }
            Message::SaveSession(msg) => {
                self.storage_mgr.session().save(self.session_id, &msg.name)
            },
            Message::LoadSession(msg) => {
                self.handle_load_session(msg)
            },
            Message::RenameSession(msg) => {
                self.storage_mgr.session().rename(&msg.old_name, &msg.new_name)
            },
            Message::DeleteSession(msg) => {
                self.storage_mgr.session().delete(&msg.name)
            },
            Message::AddObject(msg) if state.is_run()  => self.add_obj(msg),
            unexpected => return Err(Error::UnexpectedMessage(unexpected))
        }
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

    fn shutdown(&mut self) -> Result<()> {
        *shared_access![mut self.state] = State::Off;

        self.storage_mgr.session().unlock(self.session_id)
    }

    fn continue_simulation(&mut self) -> Result<()> {
        *shared_access![mut self.state] = State::Simulating;
        Ok(())
    }

    fn pause_simulation(&mut self) -> Result<()> {
        *shared_access![mut self.state] = State::Paused;
        Ok(())
    }

    fn add_obj(&mut self, msg: message::AddObject) -> Result<()> {
        let name = match msg.name {
            Some(name) => name,
            None => {
                let old_index = self.unnamed_object_index;
                self.unnamed_object_index += 1;

                format!("object-{}", old_index)
            }
        };

        println!("new object: '{}'", name);
        println!("location: {}", msg.location);
        println!("first appearance: {}", TimeFormat::VirtualTimeShort(msg.t.unwrap_or(self.virtual_time)));
        println!("color: {}", msg.color.unwrap_or(graphics::random_color()));
        println!("radius: {}", msg.radius);
        println!("mass: {}", msg.mass);
        println!("lower time border: {:?}", msg.min_t);
        println!("upper time border: {:?}", msg.max_t);

        Ok(())
    }

    fn handle_virtual_time_step(&mut self, state: State, msg: message::VirtualTimeStep) -> Result<()> {
        match msg.step {
            Some(step) if state.is_run() => if msg.reverse {
                self.virtual_time_step = -step;
            } else {
                self.virtual_time_step = step;
            },
            None => if msg.reverse {
                println!("{}", TimeFormat::VirtualTimeShort(-self.virtual_time_step));
            } else {
                println!("{}", TimeFormat::VirtualTimeShort(self.virtual_time_step));
            },
            _ => return Err(Error::VirtualTime(
                "setting virtual time step after the simulation has complete is forbidden".into()
            ))
        }

        Ok(())
    }

    fn handle_virtual_time(&mut self, state: State, msg: message::VirtualTime) -> Result<()> {
        match msg.time {
            Some(time) if state.is_run() => if msg.reverse {
                self.virtual_time = -time;
            } else {
                self.virtual_time = time;
            },
            None if state.is_run() && msg.origin => self.virtual_time = chrono::Duration::zero(),
            None if !msg.origin => if msg.reverse {
                println!("{}", TimeFormat::VirtualTimeShort(-self.virtual_time));
            } else {
                println!("{}", TimeFormat::VirtualTimeShort(self.virtual_time));
            }
            _ => return Err(Error::VirtualTime(
                "setting virtual time after the simulation has complete is forbidden".into()
            )),
            
        }

        Ok(())
    }

    fn handle_new_session(&mut self, msg: message::NewSession) -> Result<()> {
        let new_session_id = self.storage_mgr.session().new(msg.name)?;
        self.storage_mgr.session().unlock(self.session_id)?;
        self.session_id = new_session_id;
        
        Ok(())
    }

    fn handle_load_session(&mut self, msg: message::LoadSession) -> Result<()> {
        let new_session_id = self.storage_mgr.session().load(&msg.name)?;
        self.storage_mgr.session().unlock(self.session_id)?;
        self.session_id = new_session_id;

        Ok(())
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
        self.virtual_time = self.virtual_time + chrono::Duration::microseconds(
            (frame_step * 1000.0) as RawTime
        );

        // TODO
        std::thread::sleep(std::time::Duration::from_millis(100));

        self.frame_count += 1;
    }

    fn handle_window_events(&mut self) {
        for event in self.window.events().iter() {
            match event.value {
                WindowEvent::Key(key, action, mods) => if let Err(err) = self.handle_key(key, action, mods) {
                    error! {
                        target: LOG_TARGET,
                        "{}", err
                    }
                },
                _ => {}
            }
        }
    }

    fn update_session_access_time(&mut self) -> Result<()> {
        if self.real_time.num_milliseconds() >= (
            self.last_session_update_time.num_milliseconds()
            + ACCESS_UPDATE_TIME.num_milliseconds()
        ) {
            trace! {
                target: LOG_TARGET,
                "update session access time"
            };

            self.storage_mgr.session().update_access_time(self.session_id)?;
            self.last_session_update_time = self.real_time;
        }

        Ok(())
    }

    fn handle_key(&mut self, key: Key, action: Action, modifiers: Modifiers) -> Result<()> {
        match key {
            Key::Space | Key::P if matches![action, Action::Press] => {
                let state = *shared_access![self.state];
                match state {
                    State::Simulating => self.pause_simulation(),
                    State::Paused => self.continue_simulation(),
                    _ => Ok(())
                }
            },
            Key::C if modifiers.contains(Modifiers::Control) && matches![action, Action::Press] => {
                self.close();
                Ok(())
            }
            _ => Ok(())
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
            TimeFormat::VirtualTimeLong(self.virtual_time)
        ).unwrap();

        writeln!(
            &mut stats_text, 
            "virtual time step: {}",
            TimeFormat::VirtualTimeShort(self.virtual_time_step)
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