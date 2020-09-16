use {
    super::{
        cli,
        r#type::{self, LayerName, ObjectName, SessionName},
    },
    log::LevelFilter,
    std::path::PathBuf,
};

#[macro_use]
mod messages_macro;

pub mod layer;

messages! {
    #[cli(name = "help", about = "print message list")]
    message GlobalHelp {}

    #[cli(name = "h", about = "print message list")]
    message GlobalHelpShort {}

    #[derive(Default)]
    #[cli(name = "shutdown", about = "shutdown the application")]
    message Shutdown {}

    #[cli(name = "q", about = "shutdown the application")]
    message ShutdownShort {}

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
    }

    #[cli(name = "vt", about = "get/set virtual time")]
    message VirtualTime {
        /// Set virtual time to origin
        #[structopt(short, long, conflicts_with = "time")]
        pub origin: bool,

        /// New virtual time.
        #[structopt(short, long, allow_hyphen_values = true, parse(try_from_str = cli::parse_time))]
        pub time: Option<chrono::Duration>,
    }

    #[cli(name = "new-layer", about = "create new layer")]
    message NewLayer {
        /// New layer's name.
        #[structopt(short, long)]
        pub name: LayerName
    }

    #[cli(name = "rm-layer", about = "remove layer")]
    message RemoveLayer {
        /// Layer's name to remove.
        #[structopt(short, long)]
        pub name: LayerName
    }

    #[cli(name = "rename-layer", about = "rename layer")]
    message RenameLayer {
        /// Layer's name to rename.
        #[structopt(short, long)]
        pub old_name: LayerName,

        /// New layer's name.
        #[structopt(short, long)]
        pub new_name: LayerName
    }

    #[cli(name = "active-layer", about = "show active layer name")]
    message ActiveLayer {}

    #[cli(name = "current-layer", about = "show current layer name depending on the current virtual time")]
    message CurrentLayer {}

    #[cli(name = "list-layers", about = "list layers in the current session")]
    message ListLayers {}

    #[cli(name = "select-layer", about = "select new active layer")]
    message SelectLayer {
        /// Name of the layer to select.
        #[structopt(short, long)]
        pub name: LayerName
    }

    #[cli(name = "submit", about = "submit edition")]
    message Submit {}

    #[cli(name = "cancel", about = "cancel edition")]
    message Cancel {}

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

    #[cli(name = "rename-obj", about = "rename object on the scene")]
    message RenameObject {
        /// Old object's name.
        #[structopt(short, long)]
        pub old_name: ObjectName,

        /// New object's name.
        #[structopt(short, long)]
        pub new_name: ObjectName
    }

    #[cli(name = "list-objects", about = "list all objects in the current session (or in the new layer)")]
    message ListObjects {}

    #[cli(name = "object-info", about = "print object's info")]
    message ObjectInfo {
        /// Object's name
        #[structopt(short, long)]
        pub name: ObjectName
    }

    #[cli(name = "names", about = "show/hide scene's actors' names visualization")]
    message Names {}

    #[cli(name = "tracks", about = "show/hide objects' tracks visualization")]
    message Tracks {
        /// Set track step
        #[structopt(short, long, parse(try_from_str = cli::parse_time))]
        pub step: Option<chrono::Duration>
    }

    #[cli(name = "rtree", about = "show/hide R-tree visualization")]
    message RTree {}

    #[cli(name = "stats", about = "show/hide statistics rendering")]
    message Stats {}

    submessages {
        Layer(layer::Message)
    }
}
