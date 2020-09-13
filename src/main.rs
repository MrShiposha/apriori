use structopt::StructOpt;

mod app;
mod cli;
mod engine;
mod error;
mod graphics;
mod layer;
mod object;
mod logger;
mod message;
mod storage;
mod r#type;

#[macro_use]
mod shared;

pub use error::Error;
pub type Result<T> = std::result::Result<T, Error>;

pub use shared::Shared;

fn main() {
    let options = app::Options::from_args();

    let mut app = app::App::new(options.log_filter);

    app.run(options.history_file).unwrap();
}
