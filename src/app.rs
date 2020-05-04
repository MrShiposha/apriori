use super::{
    cli, graphics,
    message::{self, Message},
    r#type::{Color, RawTime, TimeFormat, TimeUnit},
    scene::{physics::Engine, SceneManager},
    shared_access,
    Error, Result, Shared,
};
use kiss3d::{
    camera::{
        Camera,
        FirstPerson,
    },
    light::Light,
    event::{Action, Key, Modifiers, WindowEvent},
    scene::SceneNode,
    text::Font,
    window::{CanvasSetup, NumSamples, Window},
};
use lazy_static::lazy_static;
use log::{info, error};
use nalgebra::{Point2, Point3, Vector2};
use std::{
    fmt::{self, Write},
    path::PathBuf,
    sync::mpsc::TryRecvError,
};
use structopt::StructOpt;

const LOG_TARGET: &'static str = "application";
pub const APP_NAME: &'static str = "apriori";

const CLOSE_MESSAGE: &'static str =
    "Simulation is completed.\nTo close the application, run `shutdown` message.";

lazy_static! {
    pub static ref APP_CLI_PROMPT: String = format!("{}> ", APP_NAME);
    pub static ref ACCESS_UPDATE_TIME: chrono::Duration = chrono::Duration::seconds(30);
    pub static ref SESSION_MAX_HANG_TIME: chrono::Duration =
        chrono::Duration::seconds(ACCESS_UPDATE_TIME.num_seconds() + 10);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    Simulating,
    Paused,
    Completed,
    Off,
}

impl State {
    pub fn is_run(&self) -> bool {
        !self.is_completed() && !self.is_off()
    }

    pub fn is_paused(&self) -> bool {
        matches![self, State::Paused]
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
            State::Off => write!(f, "[OFF]"),
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
    scene_mgr: SceneManager,
    engine: Engine,
    object_index: usize,
    attractor_index: usize,
    is_names_displayed: bool,
}

impl App {
    pub fn new(log_filter: log::LevelFilter) -> Self {
        super::logger::Logger::init(log_filter).expect("unable to initialize logging system");

        let mut window = Window::new_with_setup(
            APP_NAME,
            800,
            600,
            CanvasSetup {
                vsync: true,
                samples: NumSamples::Four,
            },
        );
        window.set_light(Light::StickToCamera);

        let mut camera = FirstPerson::new(Point3::new(0.0, 0.0, -10.0), Point3::origin());
        camera.rebind_up_key(Some(Key::W));
        camera.rebind_down_key(Some(Key::S));
        camera.rebind_left_key(Some(Key::A));
        camera.rebind_right_key(Some(Key::D));

        let scene = window.scene().clone();

        Self {
            window,
            camera: camera,
            state: State::Paused.into(),
            real_time: chrono::Duration::zero(),
            last_session_update_time: chrono::Duration::zero(),
            virtual_time: chrono::Duration::zero(),
            virtual_time_step: chrono::Duration::seconds(1),
            frame_delta_time: chrono::Duration::milliseconds(0),
            is_stats_enabled: true,
            frame_deltas_ms_sum: 0,
            frame_count: 0,
            scene_mgr: SceneManager::new(scene),
            engine: Engine::new().expect("unable to initialize physics engine"),
            object_index: 0,
            attractor_index: 0,
            is_names_displayed: false,
        }
    }

    pub fn run(&mut self, history: Option<PathBuf>) {
        let cli = cli::Observer::new(self.state.share(), history);
        let one_second_nanos = chrono::Duration::seconds(1).num_nanoseconds().unwrap();

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
                    let delta_nanos = self.frame_delta_time.num_nanoseconds().unwrap();
                    let vt_step_nanos = self.virtual_time_step.num_nanoseconds().unwrap();
                    let step = delta_nanos * vt_step_nanos / one_second_nanos;
                    self.virtual_time = self.virtual_time + chrono::Duration::nanoseconds(step);

                    self.simulate_frame();
                    self.render_frame();
                    self.process_console(&cli);
                }
                State::Paused => {
                    self.simulate_frame();
                    self.render_frame();
                    self.process_console(&cli);
                }
                State::Completed => {
                    self.draw_text(CLOSE_MESSAGE, Point2::origin(), Color::new(1.0, 0.0, 0.0));
                    self.process_console(&cli);

                    self.window.render_with_camera(&mut self.camera);
                }
                State::Off => break,
            }

            let loop_end = time::precise_time_ns();
            let loop_time = loop_end - loop_begin;

            self.frame_delta_time = chrono::Duration::nanoseconds(loop_time as RawTime);
            self.frame_deltas_ms_sum += self.frame_delta_time.num_milliseconds() as usize;
            self.frame_count += 1;

            self.real_time = self.real_time + self.frame_delta_time;
        }

        cli.join();
    }

    pub fn handle_message(&mut self, message: Message) -> Result<()> {
        let state = *shared_access![self.state];
        assert_ne!(state, State::Off);

        match message {
            Message::GlobalHelp(_) | Message::GlobalHelpShort(_) => {
                let max_name = Message::cli_list()
                    .iter()
                    .map(|(name, _)| name.len())
                    .max()
                    .unwrap();

                println!();
                for (name, about) in Message::cli_list() {
                    print!("\t{:<width$}", name, width = max_name);
                    match about {
                        Some(about) => println!("  // {}", about),
                        None => println!(),
                    }
                }

                Ok(())
            }
            Message::Run(_)
            | Message::Continue(_)
            | Message::RunShort(_)
            | Message::ContinueShort(_)
                if state.is_run() =>
            {
                self.continue_simulation()
            }
            Message::Pause(_) | Message::PauseShort(_) if state.is_run() => self.pause_simulation(),
            Message::Shutdown(_) => self.shutdown(),
            Message::TimeFormat(_) => {
                println!();
                println!("\tDigit: 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9");
                println!("\tTimeUnit:");

                let units = TimeUnit::variants_and_aliases();
                let unit_max_width = units
                    .iter()
                    .map(|aliases| aliases.iter().map(|alias| alias.len()).max().unwrap())
                    .max()
                    .unwrap();

                for unit_aliases in units {
                    print!("\t");
                    for alias in *unit_aliases {
                        print!(" {:^width$} |", alias, width = unit_max_width);
                    }
                    println!();
                }
                println!("\tTimeComponent: [-]{{Digit}}{{TimeUnit}}");
                println!("\tTimeInputFormat: {{TimeComponent}}[:{{TimeComponent}}]*");
                println!();

                Ok(())
            }
            Message::VirtualTimeStep(msg) => self.handle_virtual_time_step(state, msg),
            Message::VirtualTime(msg) => self.handle_virtual_time(state, msg),
            Message::GetFrameDeltaTime(_) => {
                println!("{}", TimeFormat::FrameDelta(self.frame_delta_time));
                Ok(())
            }
            Message::GetFrameCount(_) => {
                println!("{}", self.frame_count);
                Ok(())
            }
            Message::GetFpms(_) => {
                println!("{}", self.frame_per_ms());
                Ok(())
            }
            Message::ListSessions(_) => {
                println!("\n\t-- sessions list --");
                self.engine.print_session_list()
            }
            Message::GetSession(_) => self.engine.print_current_session_name(),
            Message::NewSession(msg) => self.engine.new_session(msg.name),
            Message::SaveSession(msg) => self.engine.save_current_session(msg.name),
            Message::LoadSession(msg) => self.engine.load_session(msg.name),
            Message::RenameSession(msg) => self.engine.rename_session(&msg.old_name, &msg.new_name),
            Message::DeleteSession(msg) => self.engine.delete_session(&msg.name),
            Message::AddObject(msg) if state.is_run() => self.handle_add_object(msg),
            Message::RenameObject(msg) if state.is_run() => self.handle_rename_object(msg),
            Message::ListObjects(_) => {
                println!("\n\t-- object list --");
                self.engine.print_object_list()
            },
            Message::AddAttractor(msg) if state.is_run() => self.handler_add_attractor(msg),
            Message::ShowNames(_) => self.handle_show_names(),
            Message::HideNames(_) => self.handle_hide_names(),
            unexpected => return Err(Error::UnexpectedMessage(unexpected)),
        }
    }

    fn check_window_opened(&mut self) {
        let state = *shared_access![self.state];
        if self.window.should_close() && matches![state, State::Simulating | State::Paused] {
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
        self.engine.shutdown()
    }

    fn continue_simulation(&mut self) -> Result<()> {
        *shared_access![mut self.state] = State::Simulating;
        Ok(())
    }

    fn pause_simulation(&mut self) -> Result<()> {
        *shared_access![mut self.state] = State::Paused;
        Ok(())
    }

    fn handle_add_object(&mut self, msg: message::AddObject) -> Result<()> {
        let object_index = self.object_index;
        self.object_index += 1;

        let default_name = format!("object-{}", object_index);

        self.scene_mgr.add_object(
            &mut self.engine, 
            msg, 
            default_name
        )?;

        Ok(())
    }

    fn handler_add_attractor(&mut self,  msg: message::AddAttractor) -> Result<()> {
        let attractor_index = self.attractor_index;
        self.attractor_index += 1;

        let default_name = format!("attractor-{}", attractor_index);

        self.scene_mgr.add_attractor(
            &mut self.engine,
            msg, 
            default_name
        )?;

        Ok(())
    }

    fn handle_rename_object(&mut self, msg: message::RenameObject) -> Result<()> {
        self.scene_mgr.rename_object(&mut self.engine, msg.old_name, msg.new_name)
    }

    fn handle_virtual_time_step(
        &mut self,
        state: State,
        msg: message::VirtualTimeStep,
    ) -> Result<()> {
        match msg.step {
            Some(step) if state.is_run() => {
                if msg.reverse {
                    self.virtual_time_step = -step;
                } else {
                    self.virtual_time_step = step;
                }
            }
            None => {
                if msg.reverse {
                    println!("{}", TimeFormat::VirtualTimeShort(-self.virtual_time_step));
                } else {
                    println!("{}", TimeFormat::VirtualTimeShort(self.virtual_time_step));
                }
            }
            _ => {
                return Err(Error::VirtualTime(
                    "setting virtual time step after the simulation has complete is forbidden"
                        .into(),
                ))
            }
        }

        Ok(())
    }

    fn handle_virtual_time(&mut self, state: State, msg: message::VirtualTime) -> Result<()> {
        match msg.time {
            Some(time) if state.is_run() => {
                if msg.reverse {
                    self.virtual_time = -time;
                } else {
                    self.virtual_time = time;
                }
            }
            None if state.is_run() && msg.origin => self.virtual_time = chrono::Duration::zero(),
            None if !msg.origin => {
                if msg.reverse {
                    println!("{}", TimeFormat::VirtualTimeShort(-self.virtual_time));
                } else {
                    println!("{}", TimeFormat::VirtualTimeShort(self.virtual_time));
                }
            }
            _ => {
                return Err(Error::VirtualTime(
                    "setting virtual time after the simulation has complete is forbidden".into(),
                ))
            }
        }

        Ok(())
    }

    fn handle_show_names(&mut self) -> Result<()> {
        self.is_names_displayed = true;

        info! {
            target: LOG_TARGET,
            "objects' names are shown"
        }

        Ok(())
    }

    fn handle_hide_names(&mut self) -> Result<()> {
        self.is_names_displayed = false;

        info! {
            target: LOG_TARGET,
            "objects' names are hided"
        }

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
        self.scene_mgr.query_objects_by_time(
            &mut self.engine, 
            &self.virtual_time, 
            {
                let is_names_displayed = self.is_names_displayed;
                let window = &mut self.window;
                let window_size = Vector2::new(window.width() as f32, window.height() as f32);
                let hidpi_factor = window.hidpi_factor() as f32;
                let text_size = 85.0;
                let half_text_size = text_size / 2.0;
                let quarter_text_size = half_text_size / 2.0;
                let font = Font::default();

                let camera = &mut self.camera;

                move |object, location| {
                    if is_names_displayed {
                        let mut text_location = camera.project(
                            &Point3::from(location), 
                            &window_size
                        ).scale(hidpi_factor) - Vector2::new(quarter_text_size, -half_text_size);
                        text_location[1] = window_size[1] * hidpi_factor - text_location[1];

                        window.draw_text(
                            format!("+ {}", object.name()).as_ref(),
                            &Point2::from(text_location), 
                            text_size, 
                            &font, 
                            &graphics::opposite_color(object.color())
                        );
                    }
                }
            }
        );
    }

    fn handle_window_events(&mut self) {
        for event in self.window.events().iter() {
            match event.value {
                WindowEvent::Key(key, action, mods) => {
                    if let Err(err) = self.handle_key(key, action, mods) {
                        error! {
                            target: LOG_TARGET,
                            "{}", err
                        }
                    }
                },
                _ => {}
            }
        }
    }

    fn update_session_access_time(&mut self) -> Result<()> {
        if self.real_time.num_milliseconds()
            >= (self.last_session_update_time.num_milliseconds()
                + ACCESS_UPDATE_TIME.num_milliseconds())
        {
            self.engine.update_session_access_time()?;
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
                    _ => Ok(()),
                }
            }
            Key::Left
                if matches![action, Action::Press] && shared_access![self.state].is_paused() =>
            {
                self.virtual_time = self.virtual_time - self.virtual_time_step;
                self.simulate_frame();
                Ok(())
            }
            Key::Right
                if matches![action, Action::Press] && shared_access![self.state].is_paused() =>
            {
                self.virtual_time = self.virtual_time + self.virtual_time_step;
                self.simulate_frame();
                Ok(())
            }
            Key::C if modifiers.contains(Modifiers::Control) && matches![action, Action::Press] => {
                self.close();
                Ok(())
            }
            _ => Ok(()),
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
                    },
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(err) => error! {
                target: LOG_TARGET,
                "{}", err
            },
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
            Color::new(1.0, 1.0, 1.0),
        );
    }

    fn draw_simulation_stats(&mut self) {
        let pos = Point2::new(0.0, 150.0);

        let mut stats_text = String::new();

        writeln!(&mut stats_text, "frame #{}", self.frame_count).unwrap();

        writeln!(
            &mut stats_text,
            "virtual time: {}",
            TimeFormat::VirtualTimeLong(self.virtual_time)
        )
        .unwrap();

        writeln!(
            &mut stats_text,
            "virtual time step: {}",
            TimeFormat::VirtualTimeShort(self.virtual_time_step)
        )
        .unwrap();

        writeln!(
            &mut stats_text,
            "frame delta time: {}",
            TimeFormat::FrameDelta(self.frame_delta_time)
        )
        .unwrap();

        writeln!(&mut stats_text, "frame per ms: {}", self.frame_per_ms()).unwrap();

        self.draw_text(&stats_text, pos, Color::new(1.0, 0.0, 1.0));
    }

    fn draw_text(&mut self, text: &str, pos: Point2<f32>, color: Color) {
        let scale = 75.0;
        let font = Font::default();

        self.window.draw_text(text, &pos, scale, &font, &color);
    }
}

#[derive(StructOpt)]
pub struct Options {
    /// File with command history
    #[structopt(long)]
    pub history_file: Option<PathBuf>,

    /// Log level filter
    #[structopt(short, long, default_value = "warn")]
    pub log_filter: log::LevelFilter,
}
