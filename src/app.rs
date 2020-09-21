use super::{
    cli,
    engine::Engine,
    graphics, layer,
    logger::LOGGER,
    make_error,
    message::{self, Message},
    object,
    r#type::{Color, LayerId, SessionInfo, TimeFormat, TimeUnit},
    shared_access, Error, Result, Shared,
};
use kiss3d::{
    camera::FirstPerson,
    event::{Action, Key, Modifiers, WindowEvent},
    light::Light,
    scene::SceneNode,
    window::{CanvasSetup, NumSamples, Window},
};
use layer::Layer;
use log::{error, info};
use nalgebra::{Point2, Point3 /*Vector2*/};
use object::{GenCoord, Object};
use ptree;
use std::{
    fmt,
    path::PathBuf,
    sync::mpsc::TryRecvError,
};
use structopt::StructOpt;

const LOG_TARGET: &'static str = "application";
pub const APP_NAME: &'static str = "apriori";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum State {
    Simulating,
    Paused,
    Completed,
}

impl State {
    pub fn is_run(&self) -> bool {
        !self.is_completed()
    }

    pub fn is_paused(&self) -> bool {
        matches![self, State::Paused]
    }

    pub fn is_completed(&self) -> bool {
        matches![self, State::Completed]
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::Simulating => write!(f, "[SIMULATING]"),
            State::Paused => write!(f, "[PAUSED]"),
            State::Completed => write!(f, "[COMPLETED]"),
        }
    }
}

pub struct App {
    window: Window,
    engine: Engine,
    camera: FirstPerson,
    state: Shared<State>,
    new_layer: Option<Layer>,
    new_default_obj_index: usize,
}

impl App {
    pub fn new(log_filter: log::LevelFilter) -> Self {
        super::logger::Logger::init(log_filter).expect("unable to initialize logging system");

        let mut window = Window::new_with_setup(
            APP_NAME,
            1024,
            768,
            CanvasSetup {
                vsync: true,
                samples: NumSamples::Four,
            },
        );
        window.set_light(Light::StickToCamera);
        window.set_framerate_limit(Some(40));

        let mut camera = FirstPerson::new(Point3::new(0.0, 0.0, -10.0), Point3::origin());
        camera.rebind_up_key(Some(Key::W));
        camera.rebind_down_key(Some(Key::S));
        camera.rebind_left_key(Some(Key::A));
        camera.rebind_right_key(Some(Key::D));

        let root_scene_node = window.scene().clone();
        let engine = Engine::init(root_scene_node).expect("unable to initialize the engine");

        Self {
            window,
            engine,
            camera: camera,
            state: State::Paused.into(),
            new_layer: None,
            new_default_obj_index: 0,
        }
    }

    pub fn run(&mut self, history: Option<PathBuf>) -> Result<()> {
        let cli = cli::Observer::new(self.state.share(), history);

        loop {
            let loop_begin = epoch_offset_ns();

            self.handle_window_events();

            let state = *shared_access![self.state];
            let advance_vtime;
            match state {
                State::Simulating => {
                    self.engine.compute_locations();
                    advance_vtime = true;
                }
                State::Paused => advance_vtime = false,
                State::Completed => break,
            }

            self.render_frame();
            self.process_console(&cli);

            let loop_end = epoch_offset_ns();
            let loop_time = loop_end - loop_begin;

            let frame_delta_ns = loop_time;
            self.engine.advance_time(frame_delta_ns, advance_vtime)?;
        }

        Ok(())
    }

    pub fn handle_message(&mut self, message: Message) -> Result<()> {
        let state = *shared_access![self.state];

        match self.new_layer {
            Some(_) => self.handle_layer_msg(message),
            None => self.handle_common_msg(message, state),
        }
    }

    fn handle_layer_msg(&mut self, msg: Message) -> Result<()> {
        use message::layer::Message as LayerMsg;

        match msg {
            Message::Layer(layer_msg) => match layer_msg {
                LayerMsg::AddObject(msg) => self.add_object_into_layer(msg),
            },
            Message::Submit(_) => self.submit_layer(),
            Message::Cancel(_) => {
                let layer = self.new_layer.take().unwrap();

                println!("--- cancel layer \"{}\" ---", layer.name());

                Ok(())
            }
            Message::ListObjects(_) => {
                self.list_new_layer_objects()?;
                self.list_current_objects()?;

                Ok(())
            },
            Message::ObjectInfo(msg) => {
                let layer = self.new_layer.as_ref().unwrap();

                let (object, coord) = layer
                    .get_object(&msg.name)
                    .ok_or(make_error![Error::Layer::ObjectNotFound(msg.name)])?;

                print_object_info(object, coord);

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
            Message::LoadSession(msg) => {
                self.new_default_obj_index = (self.engine.load_session(msg.name)? + 1) as usize;

                Ok(())
            },
            Message::RenameSession(msg) => self.engine.rename_session(msg.old_name, msg.new_name),
            Message::DeleteSession(msg) => self.engine.delete_session(msg.name),
            // Message::RenameObject(msg) if state.is_run() => self.handle_rename_object(msg),
            Message::ListObjects(_) => self.list_current_objects(),
            Message::Names(_) => {
                self.engine.toggle_names();

                Ok(())
            }
            Message::Tracks(msg) => self.handle_tracks_msg(msg),
            Message::RTree(_) => {
                self.engine.toggle_rtree();

                Ok(())
            },
            Message::Stats(_) => {
                self.engine.toggle_stats();

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

        self.new_layer = Some(Layer::new(layer_name, self.engine.context().session_id()));
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

    fn list_new_layer_objects(&self) -> Result<()> {
        let layer = self.new_layer.as_ref().unwrap();

        println!();
        println!(" --- layer \"{}\" objects ---", layer.name());
        for (object, _) in layer.iter_objects() {
            println!("\t{}", object.name());
        }

        Ok(())
    }

    fn list_current_objects(&self) -> Result<()> {
        println!();
        println!(" --- current objects ---");

        for (_, actor) in self.engine.context().actors() {
            println!("\t{}", actor.object().name());
        }

        Ok(())
    }

    fn handle_tracks_msg(&mut self, msg: message::Tracks) -> Result<()> {
        match msg.step {
            Some(step) => self.engine.show_tracks(step),
            None => self.engine.hide_tracks(),
        }

        Ok(())
    }

    fn add_object_into_layer(&mut self, msg: message::layer::AddObject) -> Result<()> {
        let object_name = msg.name.unwrap_or_else(|| {
            let default_name = format!("object-{}", self.new_default_obj_index);

            self.new_default_obj_index += 1;

            default_name
        });

        log::info! {
            target: LOG_TARGET,
            "[layer] add object \"{}\"",
            object_name
        }

        // TODO check collisions
        if self.engine.is_object_exists(&object_name)? {
            Err(make_error!(Error::Layer::ObjectAlreadyExists(object_name)))
        } else {
            let layer = self.new_layer.as_mut().unwrap();

            let object = Object::new(
                LayerId::default(),
                object_name,
                msg.radius,
                msg.color.unwrap_or(graphics::random_color()),
                msg.mass,
                msg.step,
            );

            let coord = GenCoord::new(self.engine.virtual_time(), msg.location, msg.velocity);

            layer.add_object(object, coord)
        }
    }

    fn check_window_opened(&mut self) {
        let state = *shared_access![self.state];
        if self.window.should_close() && matches![state, State::Simulating | State::Paused] {
            self.close();
        }
    }

    fn close(&mut self) {
        *self.window.scene_mut() = SceneNode::new_empty();

        *shared_access![mut self.state] = State::Completed;
    }

    fn shutdown(&mut self) -> Result<()> {
        *shared_access![mut self.state] = State::Completed;
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
                    self.engine.set_virtual_time(time, false)?;
                } else {
                    return Err(Error::VirtualTime(
                        "setting virtual time that lower than zero is forbidden".into(),
                    ));
                }
            }
            None if state.is_run() && msg.origin => {
                self.engine
                    .set_virtual_time(chrono::Duration::zero(), false)?;
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

    fn render_frame(&mut self) {
        self.check_window_opened();

        self.window.render_with_camera(&mut self.camera);

        self.draw_state_text();

        self.engine.draw_debug_info(&mut self.window, &mut self.camera);
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
                }
                _ => {}
            }
        }
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
                let vtime = self.engine.virtual_time() - self.engine.virtual_step();
                self.engine.set_virtual_time(vtime, true)
            }
            Key::Right
                if matches![action, Action::Press] && shared_access![self.state].is_paused() =>
            {
                let vtime = self.engine.virtual_time() + self.engine.virtual_step();
                self.engine.set_virtual_time(vtime, true)
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
        self.engine.scene_mut().draw_text(
            &mut self.window,
            format!("{}", state).as_ref(),
            Point2::origin(),
            Color::new(1.0, 1.0, 1.0),
        );
    }
}

fn print_object_info(object: &Object, coord: &GenCoord) {
    println!();
    println!("\"{}\": {{", object.name());
    println!("\ttime = {}", TimeFormat::VirtualTimeShort(coord.time()));

    let location = coord.location();
    println!(
        "\tlocation = {{{}, {}, {}}}",
        location[0], location[1], location[2]
    );

    let velocty = coord.velocity();
    println!(
        "\tvelocity = {{{}, {}, {}}}",
        velocty[0], velocty[1], velocty[2]
    );

    println!("\tradius = {}", object.radius());

    let color = object.color();
    println!("\tcolor = {{{}, {}, {}}}", color[0], color[1], color[2]);
    println!("\tmass = {}", object.mass());
    println!(
        "\tcompute_step = {}",
        TimeFormat::VirtualTimeShort(object.compute_step())
    );
    println!("}}");
}

fn epoch_offset_ns() -> i128 {
    (time::OffsetDateTime::now_utc() - time::OffsetDateTime::unix_epoch()).whole_nanoseconds()
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
