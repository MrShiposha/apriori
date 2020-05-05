use {
    std::{
        path::{
            Path,
            PathBuf,
        },
        fs::File,
        io::prelude::*,
        ops::Deref,
        cmp::{PartialEq, Eq},
        borrow::Borrow,
        hash::{Hash, Hasher},
        collections::hash_set::HashSet,
    },
    log::{set_logger, set_max_level, max_level, LevelFilter, Log, Metadata, Record, SetLoggerError},
    lazy_static::lazy_static,
    regex::Regex,
    crate::{
        make_error,
        Result,
        shared::Shared,
    }
};

lazy_static! {
    pub static ref LOGGER: Shared<Logger> = Shared::from(Logger::new());
    
    static ref DEPS_TARGETS: RegexWrapper = {
        let targets = [
            "rustyline",
            "mio*",
            "tokio*"
        ];

        let regex = targets.iter()
            .map(|target| format!("({})", target))
            .collect::<Vec<_>>()
            .join("|");

        RegexWrapper::new(regex.as_str()).unwrap()
    };
}

pub struct Logger {
    disabled_targets: HashSet<RegexWrapper>,
    log_file: Option<File>,
    log_file_path: Option<PathBuf>,
}

impl Logger {
    pub fn init(filter: LevelFilter) -> std::result::Result<(), SetLoggerError> {
        set_logger(&*LOGGER).map(|()| set_max_level(filter))
    }

    pub fn disable_target<T: AsRef<str>>(&mut self, target: T) -> Result<()> {
        let target = RegexWrapper::new(target.as_ref())?;
        self.disabled_targets.insert(target);

        Ok(())
    }

    pub fn enable_target<T: AsRef<str>>(&mut self, target: T) {
        self.disabled_targets.remove(target.as_ref());
    }

    pub fn print_disabled_targets(&self) {
        for target in self.disabled_targets.iter() {
            println!("\t{}", target.as_str());
        }
    }

    pub fn disable_deps_targets(&mut self) {
        self.disabled_targets.insert(DEPS_TARGETS.clone());
    }

    pub fn enable_deps_targets(&mut self) {
        self.disabled_targets.remove(&*DEPS_TARGETS);
    }

    pub fn enable_all_targets(&mut self) {
        self.disabled_targets.clear();
    }

    pub fn set_max_level(&mut self, filter: LevelFilter) {
        set_max_level(filter);
    }

    pub fn get_max_level(&self) -> LevelFilter {
        max_level()
    }

    pub fn set_log_file_path<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();

        let log_file = File::create(path)?;

        self.log_file = Some(log_file);
        self.log_file_path = Some(path.to_path_buf());

        Ok(())
    }

    pub fn clear_log_file(&mut self) {
        self.log_file = None;
        self.log_file_path = None;
    }

    pub fn get_log_file_path(&self) -> Option<&Path> {
        self.log_file_path.as_ref().map(|buf| buf.as_path())
    }

    fn new() -> Self {
        Self {
            disabled_targets: HashSet::new(),
            log_file: None,
            log_file_path: None,
        }
    }
}

impl Log for Shared<Logger> {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let logger = self.read().expect("unable to access the logger");

            for target in logger.disabled_targets.iter() {
                if target.is_match(record.target()) {
                    return;
                }
            }

            let log_msg = format!(
                "|{}| {} -- {}\n",
                record.level(),
                record.target(),
                record.args()
            );

            print!("{}", log_msg);
            std::io::stdout().flush().unwrap();

            if logger.log_file.is_some() {
                std::mem::drop(logger);
                
                let mut logger = self.write().expect("unable to access the logger");
                let log_file = logger.log_file.as_mut().unwrap();

                log_file.write_all(log_msg.as_bytes())
                    .expect("unable to write to the log file");
            }
        }
    }

    fn flush(&self) {}
}

#[derive(Clone)]
struct RegexWrapper(Regex);

impl RegexWrapper {
    fn new(pattern: &str) -> Result<Self> {
        let regex = Regex::new(pattern).map_err(|err| make_error![Error::Parse::Regex(err)])?;

        Ok(Self(regex))
    }
}

impl PartialEq for RegexWrapper {
    fn eq(&self, other: &RegexWrapper) -> bool {
        self.0.as_str() == other.0.as_str()
    }
}

impl PartialEq<str> for RegexWrapper {
    fn eq(&self, other: &str) -> bool {
        self.0.as_str() == other
    }
}

impl Eq for RegexWrapper {}

impl Borrow<str> for RegexWrapper {
    fn borrow(&self) -> &str {
        self.0.as_str()
    }
}

impl Hash for RegexWrapper {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state);
    }
}

impl Deref for RegexWrapper {
    type Target = Regex;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}