use super::{
    cli,
    engine::Engine,
    layer,
    logger::LOGGER,
    make_error,
    message::{self, Message},
    r#type::{Color, RawTime, SessionInfo, TimeFormat, TimeUnit},
    shared_access, Error, Result, Shared,
};
use kiss3d::{
    camera::FirstPerson,
    event::{Action, Key, Modifiers, WindowEvent},
    light::Light,
    scene::SceneNode,
    text::Font,
    window::{CanvasSetup, NumSamples, Window},
};
use layer::Layer;
use log::{error, info};
use nalgebra::{Point2, Point3 /*Vector2*/};
use ptree;
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
    engine: Engine,
    camera: FirstPerson,
    state: Shared<State>,
    new_layer: Option<Layer>,
    is_stats_enabled: bool,
    is_names_displayed: bool,
    is_tracks_displayed: bool,
    track_step: chrono::Duration,
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

        // let scene = window.scene().clone();

        let engine = Engine::init().expect("unable to initialize the engine");

        Self {
            window,
            engine,
            camera: camera,
            state: State::Paused.into(),
            new_layer: None,
            is_stats_enabled: true,
            is_names_displayed: false,
            is_tracks_displayed: false,
            track_step: chrono::Duration::milliseconds(500),
        }
    }

    pub fn run(&mut self, history: Option<PathBuf>) {
        let cli = cli::Observer::new(self.state.share(), history);

        loop {
            let loop_begin = time::precise_time_ns();

            self.handle_window_events();

            let state = *shared_access![self.state];
            let advance_vtime;
            match state {
                State::Simulating => {
                    self.engine.compute_locations();
                    advance_vtime = true;
                }
                State::Paused => advance_vtime = false,
                State::Completed => {
                    self.draw_text(CLOSE_MESSAGE, Point2::origin(), Color::new(1.0, 0.0, 0.0));
                    self.is_stats_enabled = false;
                    advance_vtime = false;
                }
                State::Off => break,
            }

            self.render_frame();
            self.process_console(&cli);

            let loop_end = time::precise_time_ns();
            let loop_time = loop_end - loop_begin;

            let frame_delta_ns = loop_time as RawTime;
            self.engine.advance_time(frame_delta_ns, advance_vtime);
        }

        cli.join();
    }

    pub fn handle_message(&mut self, message: Message) -> Result<()> {
        let state = *shared_access![self.state];
        assert_ne!(state, State::Off);

        match self.new_layer {
            Some(_) => self.handle_layer_msg(message),
            None => self.handle_common_msg(message, state),
        }
    }

    fn handle_layer_msg(&mut self, msg: Message) -> Result<()> {
        use message::layer::Message as LayerMsg;

        match msg {
            Message::Layer(layer_msg) => {
                match layer_msg {
                    LayerMsg::AddObject(_) => {
                        log::info! {
                            target: LOG_TARGET,
                            "[layer] add object"
                        }
                    }
                }
                Ok(())
            }
            Message::Submit(_) => self.submit_layer(),
            Message::Cancel(_) => {
                let layer = self.new_layer.take().unwrap();

                println!("--- cancel layer \"{}\" ---", layer.name());

                Ok(())
            }
            Message::Shutdown(_) | Message::ShutdownShort(_) => self.shutdown(),
            _ => Err(Error::UnexpectedMessage(msg)),
        }
    }

    fn handle_common_msg(&mut self, msg: Message, state: State) -> Result<()> {
        match msg {
            Message::NewLayer(msg) => self.new_layer(msg),
            Message::RemoveLayer(msg) => self.engine.remove_layer(&msg.name),
            Message::RenameLayer(msg) => self.engine.rename_layer(&msg.old_name, &msg.new_name),
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
            Message::Shutdown(_) | Message::ShutdownShort(_) => self.shutdown(),
            Message::ListDisabledLogTargets(_) => {
                println!("\n\t-- disabled log targets --");
                shared_access![LOGGER].print_disabled_targets();

                Ok(())
            }
            Message::LogTarget(msg) => {
                if msg.deps {
                    if msg.disable {
                        shared_access![mut LOGGER].disable_deps_targets();
                    } else {
                        shared_access![mut LOGGER].enable_deps_targets();
                    }
                } else {
                    if msg.disable {
                        shared_access![mut LOGGER].disable_target(msg.target.unwrap())?;
                    } else {
                        shared_access![mut LOGGER].enable_target(msg.target.unwrap());
                    }
                }

                Ok(())
            }
            Message::LogFilter(msg) => {
                match msg.filter {
                    Some(filter) => shared_access![mut LOGGER].set_max_level(filter),
                    None => {
                        let level = shared_access![LOGGER].get_max_level();

                        println!("{}", level);
                    }
                }

                Ok(())
            }
            Message::LogFile(msg) => match msg.path {
                Some(path) => shared_access![mut LOGGER].set_log_file_path(path),
                None => {
                    match shared_access![LOGGER].get_log_file_path() {
                        Some(path) => println!("{}", path.display()),
                        None => println!("/log file path is unset/"),
                    }

                    Ok(())
                }
            },
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
            Message::ActiveLayer(_) => {
                println!("{}", self.engine.active_layer_name()?);
                Ok(())
            }
            Message::CurrentLayer(_) => {
                println!("{}", self.engine.current_layer_name()?);
                Ok(())
            }
            Message::ListLayers(list_layers_msg) => self.list_layers(list_layers_msg),
            Message::SelectLayer(msg) => self.engine.select_layer(&msg.name),
            Message::ListSessions(_) => self.print_session_list(),
            Message::GetSession(_) => self.print_current_session_name(),
            Message::NewSession(msg) => self.engine.new_session(msg.name),
            Message::SaveSession(msg) => self.engine.save_session(msg.name),
            Message::LoadSession(msg) => self.engine.load_session(msg.name),
            Message::RenameSession(msg) => self.engine.rename_session(msg.old_name, msg.new_name),
            Message::DeleteSession(msg) => self.engine.delete_session(msg.name),
            // Message::AddObject(msg) if state.is_run() => self.handle_add_object(msg),
            // Message::RenameObject(msg) if state.is_run() => self.handle_rename_object(msg),
            // Message::ListObjects(_) => {
            //     println!("\n\t-- object list --");
            //     self.engine.print_object_list()
            // },
            Message::Names(msg) => {
                self.is_names_displayed = !msg.disable;

                Ok(())
            }
            Message::Tracks(msg) => {
                self.is_tracks_displayed = !msg.disable;

                if let Some(step) = msg.step {
                    if step >= chrono::Duration::zero() {
                        self.track_step = step;
                    } else {
                        return Err(Error::VirtualTime(
                            "track step must be greater than zero".into(),
                        ));
                    }
                }

                Ok(())
            }
            unexpected => return Err(Error::UnexpectedMessage(unexpected)),
        }
    }

    fn new_layer(&mut self, new_layer_msg: message::NewLayer) -> Result<()> {
        let layer_name = new_layer_msg.name;

        if self.engine.get_layer_id(&layer_name).is_ok() {
            return Err(make_error!(Error::Layer::LayerAlreadyExists(layer_name)));
        }

        println!("--- creating new layer \"{}\" ---", layer_name);

        self.new_layer = Some(Layer::new(layer_name));
        Ok(())
    }

    fn submit_layer(&mut self) -> Result<()> {
        let layer = self.new_layer.take().unwrap();

        println!("--- submit layer \"{}\" ---", layer.name());
        self.engine.add_layer(layer)
        // TODO ADD CHECKS
    }

    fn list_layers(&mut self, _: message::ListLayers) -> Result<()> {
        let layers_tree = self.engine.get_session_layers()?;

        ptree::print_tree(&layers_tree).map_err(|err| Error::Io(err))
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
        // self.engine.shutdown()
        Ok(())
    }

    fn continue_simulation(&mut self) -> Result<()> {
        *shared_access![mut self.state] = State::Simulating;
        Ok(())
    }

    fn pause_simulation(&mut self) -> Result<()> {
        *shared_access![mut self.state] = State::Paused;
        Ok(())
    }

    fn print_current_session_name(&mut self) -> Result<()> {
        let session_name = self.engine.get_session_name()?;

        println!("{}", session_name);

        Ok(())
    }

    fn print_session_list(&mut self) -> Result<()> {
        let infos = self.engine.get_sessions_info()?;

        println!("\n\t-- sessions list --");
        for info in infos.iter() {
            let SessionInfo {
                name,
                last_access,
                is_locked,
            } = info;

            let locked_text = "LOCKED:";
            println!(
                "{is_locked:<width$} {} [last access {}]",
                name,
                last_access,
                is_locked = if *is_locked { locked_text } else { "" },
                width = locked_text.len()
            );
        }

        Ok(())
    }

    // fn handle_add_object(&mut self, msg: message::AddObject) -> Result<()> {
    //     let object_index = self.object_index;
    //     self.object_index += 1;

    //     let default_name = format!("object-{}", object_index);

    //     self.scene_mgr.add_object(
    //         &mut self.engine,
    //         msg,
    //         self.virtual_time,
    //         default_name
    //     )?;

    //     Ok(())
    // }

    // fn handle_rename_object(&mut self, msg: message::RenameObject) -> Result<()> {
    // self.scene_mgr.rename_object(&mut self.engine, msg.old_name, msg.new_name)
    // Ok(())
    // }

    fn handle_virtual_time_step(
        &mut self,
        state: State,
        msg: message::VirtualTimeStep,
    ) -> Result<()> {
        match msg.step {
            Some(step) if state.is_run() => {
                let origin = chrono::Duration::zero();

                if step >= origin {
                    self.engine.set_virtual_step(step);
                } else {
                    return Err(Error::VirtualTime(
                        "setting virtual time step that lower than zero is forbidden".into(),
                    ));
                }
            }
            None => println!(
                "{}",
                TimeFormat::VirtualTimeShort(self.engine.virtual_step())
            ),
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
        let origin = chrono::Duration::zero();

        match msg.time {
            Some(time) if state.is_run() => {
                if time >= origin {
                    self.engine.set_virtual_time(time);
                } else {
                    return Err(Error::VirtualTime(
                        "setting virtual time that lower than zero is forbidden".into(),
                    ));
                }
            }
            None if state.is_run() && msg.origin => {
                self.engine.set_virtual_time(chrono::Duration::zero())
            }
            None if !msg.origin => println!(
                "{}",
                TimeFormat::VirtualTimeLong(self.engine.virtual_time())
            ),
            _ => {
                return Err(Error::VirtualTime(
                    "setting virtual time after the simulation has complete is forbidden".into(),
                ))
            }
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

    // fn simulate_frame(&mut self) {
    // self.scene_mgr.query_objects_by_time(
    //     &mut self.engine,
    //     &self.virtual_time,
    //     {
    //         let is_names_displayed = self.is_names_displayed;
    //         let is_tracks_displayed = self.is_tracks_displayed;
    //         let track_step = self.track_step;

    //         let window = &mut self.window;
    //         let window_size = Vector2::new(window.width() as f32, window.height() as f32);
    //         let hidpi_factor = window.hidpi_factor() as f32;
    //         let text_size = 85.0;
    //         let half_text_size = text_size / 2.0;
    //         let quarter_text_size = half_text_size / 2.0;
    //         let font = Font::default();

    //         let camera = &mut self.camera;

    //         move |object, location| {
    //             if is_names_displayed {
    //                 let mut text_location = camera.project(
    //                     &Point3::from(location),
    //                     &window_size
    //                 ).scale(hidpi_factor) - Vector2::new(quarter_text_size, -half_text_size);
    //                 text_location[1] = window_size[1] * hidpi_factor - text_location[1];

    //                 window.draw_text(
    //                     format!("+ {}", object.name()).as_ref(),
    //                     &Point2::from(text_location),
    //                     text_size,
    //                     &font,
    //                     &graphics::opposite_color(object.color())
    //                 );
    //             }

    //             if is_tracks_displayed {
    //                 Self::draw_track(
    //                     window,
    //                     &Point3::from(*object.color()),
    //                     object.track(),
    //                     track_step
    //                 );
    //             }
    //         }
    //     }
    // );
    // }

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
                }
                _ => {}
            }
        }
    }

    // fn update_session_access_time(&mut self) -> Result<()> {
    //     if self.real_time.num_milliseconds()
    //         >= (self.last_session_update_time.num_milliseconds()
    //             + ACCESS_UPDATE_TIME.num_milliseconds())
    //     {
    //         // self.engine.update_session_access_time()?;
    //         self.last_session_update_time = self.real_time;
    //     }

    //     Ok(())
    // }

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
                let vtime = self.engine.virtual_time() - self.engine.virtual_step();
                self.engine.set_virtual_time(vtime);
                Ok(())
            }
            Key::Right
                if matches![action, Action::Press] && shared_access![self.state].is_paused() =>
            {
                let vtime = self.engine.virtual_time() + self.engine.virtual_step();
                self.engine.set_virtual_time(vtime);
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

        writeln!(&mut stats_text, "frame #{}", self.engine.frame()).unwrap();

        writeln!(
            &mut stats_text,
            "virtual time: {}",
            TimeFormat::VirtualTimeLong(self.engine.virtual_time())
        )
        .unwrap();

        writeln!(
            &mut stats_text,
            "virtual time step: {}",
            TimeFormat::VirtualTimeShort(self.engine.virtual_step())
        )
        .unwrap();

        writeln!(
            &mut stats_text,
            "frame delta time: {}",
            TimeFormat::FrameDelta(self.engine.last_frame_delta())
        )
        .unwrap();

        writeln!(
            &mut stats_text,
            "frame avg ms: {}",
            self.engine.frame_avg_time_ms()
        )
        .unwrap();

        self.draw_text(&stats_text, pos, Color::new(1.0, 0.0, 1.0));
    }

    fn draw_text(&mut self, text: &str, pos: Point2<f32>, color: Color) {
        let scale = 75.0;
        let font = Font::default();

        self.window.draw_text(text, &pos, scale, &font, &color);
    }

    // fn draw_track(window: &mut Window, color: &Point3<f32>, track: &Track, step: chrono::Duration) {
    //     let mut next_time = track.time_start() + step;

    //     let mut last_location = *shared_access![track.node_start()].atom_start().location();
    //     while track.computed_range().contains(&next_time) {
    //         let new_location = track.interpolate(&next_time).unwrap();

    //         window.draw_line(
    //             &Point3::from(last_location),
    //             &Point3::from(new_location),
    //             color
    //         );

    //         next_time = next_time + step;
    //         last_location = new_location;
    //     }
    // }
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
