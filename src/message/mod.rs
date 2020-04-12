use super::{
    cli,
    r#type::{
        ObjectName,
        Coord,
        Vector,
        Color
    }
};


#[macro_use]
mod messages_macro;

messages! {
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

    #[cli(name = "vtstep", about = "get/set virtual time step")]
    message VirtualTimeStep {
        /// Virtual time step
        #[structopt(short, long, parse(try_from_str = cli::parse_time))]
        pub step: Option<chrono::Duration>,

        /// Reverse time step
        #[structopt(short, long)]
        pub reverse: bool
    }

    #[cli(name = "vt", about = "get/set new virtual time")]
    message VirtualTime {
        /// Set virtual time to origin
        #[structopt(long, conflicts_with_all(&[
            "week",
            "day",
            "hour",
            "min",
            "sec",
            "milli",
            "reserve"
        ]))]
        pub origin: bool,

        /// Weeks number
        #[structopt(short, long)]
        pub week: Option<i64>,

        /// Day number
        #[structopt(short, long)]
        pub day: Option<i64>,

        /// Hour number
        #[structopt(short, long)]
        pub hour: Option<i64>,

        /// Minute number
        #[structopt(long)]
        pub min: Option<i64>,

        /// Second number
        #[structopt(short, long)]
        pub sec: Option<i64>,

        /// Millisecond number
        #[structopt(long)]
        pub milli: Option<i64>,

        /// Reverse time
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

    #[cli(name = "save-session-as", about = "save current session with new name")]
    message SaveSession {
        /// New session's name
        #[structopt(short, long)]
        pub name: String
    }

    #[cli(name = "load-session", about = "load existing session")]
    message LoadSession {
        /// Session's name
        #[structopt(short, long)]
        pub name: String
    }

    #[cli(name = "rename-session", about = "rename session")]
    message RenameSession {
        /// Old session's name
        #[structopt(short, long)]
        pub old_name: String,

        /// New session's name
        #[structopt(short, long)]
        pub new_name: String
    }

    #[cli(name = "delete-session", about = "delete session")]
    message DeleteSession {
        /// Session's name
        #[structopt(short, long)]
        pub name: String
    }

    #[cli(name = "add-obj", about = "add new object to the scene")]
    message AddObject {
        /// Object's name
        #[structopt(short, long)]
        pub name: ObjectName,
        
        /// Object's location
        #[structopt(short, long, parse(try_from_str = cli::parse_vector))]
        pub location: Vector,

        /// When the object have to appear.
        /// If this value is None, then the object will be added right now.
        #[structopt(short, long)]
        pub t: Option<Coord>,

        /// Object's color
        #[structopt(short, long, parse(try_from_str = cli::parse_color))]
        pub color: Option<Color>
    }
}