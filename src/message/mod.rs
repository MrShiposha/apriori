use super::{
    cli,
    r#type::{
        ObjectName,
        Vector,
        Color,
        Distance,
        Mass,
        GravityCoeff
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
        pub name: Option<String>
    }

    #[cli(name = "save-session-as", about = "save current session with new name")]
    message SaveSession {
        /// Session's name.
        #[structopt(short, long)]
        pub name: String
    }

    #[cli(name = "load-session", about = "load existing session")]
    message LoadSession {
        /// Session's name.
        #[structopt(short, long)]
        pub name: String
    }

    #[cli(name = "rename-session", about = "rename session")]
    message RenameSession {
        /// Old session's name.
        #[structopt(short, long)]
        pub old_name: String,

        /// New session's name.
        #[structopt(short, long)]
        pub new_name: String
    }

    #[cli(name = "delete-session", about = "delete session")]
    message DeleteSession {
        /// Session's name.
        #[structopt(short, long)]
        pub name: String
    }

    #[cli(name = "add-obj", about = "add new object to the scene")]
    message AddObject {
        /// Object's name.
        #[structopt(short, long)]
        pub name: Option<ObjectName>,
        
        /// Object's location.
        #[structopt(short, long, parse(try_from_str = cli::parse_vector))]
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

        /// Gravity coefficient
        #[structopt(short, long, default_value = "1")]
        pub gravity: GravityCoeff,

        /// Compute step
        #[structopt(short, long, default_value = "1s", parse(try_from_str = cli::parse_time))]
        pub step: chrono::Duration,

        /// The lower border of time, 
        /// when an object allowed to be on the scene.
        #[structopt(long, allow_hyphen_values = true, parse(try_from_str = cli::parse_time))]
        pub min_t: Option<chrono::Duration>,

        /// The upper border of time, 
        /// when an object allowed to be on the scene.
        #[structopt(long, allow_hyphen_values = true, parse(try_from_str = cli::parse_time))]
        pub max_t: Option<chrono::Duration>,
    }

    #[cli(name = "rename-obj", about = "rename object on the scene")]
    message RenameObject {
        /// Old object's name.
        #[structopt(short, long)]
        pub old_name: String,

        /// New object's name.
        #[structopt(short, long)]
        pub new_name: String
    }

    #[cli(name = "list-objects", about = "list all objects in the current session")]
    message ListObjects {}
}