use structopt::StructOpt;

mod app;
mod cli;
mod storage;
mod message;
mod graphics;
mod scene;
mod r#type;
mod error;
mod logger;

#[macro_use]
mod shared;

pub use error::Error;
pub type Result<T> = std::result::Result<T, Error>;

pub use shared::Shared;

fn main() {
    let options = app::Options::from_args();

    let mut app = app::App::new(options.log_filter);
    
    app.run(options.history_file);
}