use {
    crate::{
        messages,
        cli,
        r#type::{ObjectName, Vector, Color, Distance, Mass},
    },
};

messages! {
    #[cli(name = "add-obj", about = "add new object into the layer")]
    message AddObject {
        /// Object's name.
        #[structopt(short, long)]
        pub name: Option<ObjectName>,

        /// Object's location.
        #[structopt(short, long, allow_hyphen_values = true, parse(try_from_str = cli::parse_vector))]
        pub location: Vector,

        /// Object's velocity
        #[structopt(short, long, allow_hyphen_values = true, parse(try_from_str = cli::parse_vector))]
        pub velocity: Vector,

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
    }
}
