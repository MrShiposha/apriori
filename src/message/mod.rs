use {
    std::path::PathBuf,
    log::LevelFilter,
    super::{
        cli,
        r#type::{
            self,
            Color,
            Distance,
            Mass,
            GravityCoeff,
            SessionName,
            ObjectName,
            AttractorName,
            Vector,
        },
    }
};

#[macro_use]
mod messages_macro;

messages! {
    #[cli(name = "help", about = "print message list")]
    message GlobalHelp {}

    #[cli(name = "h", about = "print message list")]
    message GlobalHelpShort {}

    #[derive(Default)]
    #[cli(name = "shutdown", about = "shutdown the application")]
    message Shutdown {}

    #[cli(name = "run", about = "run the simulation")]
    message Run {}

    #[cli(name = "r", about = "run the simulation")]
    message RunShort {}

    #[cli(name = "continue", about = "continue the simulation")]
    message Continue {}

    #[cli(name = "c", about = "continue the simulation")]
    message ContinueShort {}

    #[cli(name = "pause", about = "pause the simulation")]
    message Pause {}

    #[cli(name = "p", about = "pause the simulation")]
    message PauseShort {}

    #[cli(name = "list-disabled-log-targets", about = "list all disabled log targets")]
    message ListDisabledLogTargets {}

    #[cli(name = "log-target", about = "enable/disable log target")]
    message LogTarget {
        /// Log target's regex
        #[structopt(short, long, required_unless = "deps")]
        pub target: Option<r#type::LogTarget>,

        /// Use dependencies log targets
        #[structopt(long, conflicts_with = "target")]
        pub deps: bool,

        /// Disable log target
        #[structopt(short, long)]
        pub disable: bool,
    }

    #[cli(name = "log-filter", about = "get/set logging filter")]
    message LogFilter {
        /// Max logging level
        #[structopt(short, long)]
        pub filter: Option<LevelFilter>
    }

    #[cli(name = "log-file", about = "get/set log file")]
    message LogFile {
        /// Log file path
        #[structopt(short, long, parse(from_os_str))]
        pub path: Option<PathBuf>
    }

    #[cli(name = "time-format", about = "input time format information (tells how to specify a time)")]
    message TimeFormat {}

    #[cli(name = "vtstep", about = "get/set virtual time step")]
    message VirtualTimeStep {
        /// New virtual time step.
        #[structopt(short, long, allow_hyphen_values = true, parse(try_from_str = cli::parse_time))]
        pub step: Option<chrono::Duration>,

        /// Reverse time step.
        #[structopt(short, long)]
        pub reverse: bool
    }

    #[cli(name = "vt", about = "get/set virtual time")]
    message VirtualTime {
        /// Set virtual time to origin
        #[structopt(short, long, conflicts_with_all = &["time", "reverse"])]
        pub origin: bool,

        /// New virtual time.
        #[structopt(short, long, allow_hyphen_values = true, parse(try_from_str = cli::parse_time))]
        pub time: Option<chrono::Duration>,

        /// Reverse time.
        #[structopt(short, long)]
        pub reverse: bool
    }

    #[cli(name = "frame-delta-time", about = "last frame delta time")]
    message GetFrameDeltaTime {}

    #[cli(name = "frames", about = "get current frame count")]
    message GetFrameCount {}

    #[cli(name = "fpms", about = "get frame per ms")]
    message GetFpms {}

    #[cli(name = "list-sessions", about = "list all sessions")]
    message ListSessions {}

    #[cli(name = "session", about = "current session name")]
    message GetSession {}

    #[cli(name = "new-session", about = "create new session")]
    message NewSession {
        /// New session's name.
        #[structopt(short, long)]
        pub name: Option<SessionName>
    }

    #[cli(name = "save-session-as", about = "save current session with new name")]
    message SaveSession {
        /// Session's name.
        #[structopt(short, long)]
        pub name: SessionName
    }

    #[cli(name = "load-session", about = "load existing session")]
    message LoadSession {
        /// Session's name.
        #[structopt(short, long)]
        pub name: SessionName
    }

    #[cli(name = "rename-session", about = "rename session")]
    message RenameSession {
        /// Old session's name.
        #[structopt(short, long)]
        pub old_name: SessionName,

        /// New session's name.
        #[structopt(short, long)]
        pub new_name: SessionName
    }

    #[cli(name = "delete-session", about = "delete session")]
    message DeleteSession {
        /// Session's name.
        #[structopt(short, long)]
        pub name: SessionName
    }

    #[cli(name = "add-obj", about = "add new object to the scene")]
    message AddObject {
        /// Object's name.
        #[structopt(short, long)]
        pub name: Option<ObjectName>,

        /// Object's location.
        #[structopt(short, long, allow_hyphen_values = true, parse(try_from_str = cli::parse_vector))]
        pub location: Vector,

        /// When the object have to appear.
        /// If this option have not specified, then the object will be added right now.
        #[structopt(short, long, allow_hyphen_values = true, parse(try_from_str = cli::parse_time))]
        pub time: Option<chrono::Duration>,

        /// Object's color.
        #[structopt(short, long, parse(try_from_str = cli::parse_color))]
        pub color: Option<Color>,

        /// Object's radius.
        #[structopt(short, long, default_value = "1")]
        pub radius: Distance,

        /// Object's mass.
        #[structopt(short, long, default_value = "1")]
        pub mass: Mass,

        /// Compute step
        #[structopt(short, long, default_value = "1s", parse(try_from_str = cli::parse_time))]
        pub step: chrono::Duration,

        /// Buffered track size
        #[structopt(long, default_value = "64")]
        pub track_size: usize,
    }

    #[cli(name = "rename-obj", about = "rename object on the scene")]
    message RenameObject {
        /// Old object's name.
        #[structopt(short, long)]
        pub old_name: ObjectName,

        /// New object's name.
        #[structopt(short, long)]
        pub new_name: ObjectName
    }

    #[cli(name = "add-attr", about = "add new attractor to the scene")]
    message AddAttractor {
        /// Attractor's name
        #[structopt(short, long)]
        pub name: Option<AttractorName>,

        /// Attractor's location
        #[structopt(short, long, allow_hyphen_values = true, parse(try_from_str = cli::parse_vector))]
        pub location: Vector,

        /// Attractor's mass
        #[structopt(short, long, default_value = "1")]
        pub mass: Mass,

        /// Attractor's gravity coefficient
        #[structopt(short, long, default_value = "1")]
        pub gravity_coeff: GravityCoeff,
    }

    #[cli(name = "list-objects", about = "list all objects in the current session")]
    message ListObjects {}

    #[cli(name = "names", about = "enable/disable scene's actors' names")]
    message Names {
        /// Disable names
        #[structopt(short, long)]
        pub disable: bool
    }

    #[cli(name = "tracks", about = "enable/disable objects' tracks")]
    message Tracks {
        /// Show tracks
        #[structopt(short, long)]
        pub disable: bool,

        /// Set track step
        #[structopt(long, parse(try_from_str = cli::parse_time))]
        pub step: Option<chrono::Duration>
    }
}
