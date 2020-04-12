use std::{
    sync::{
        Arc,
        RwLock,
        RwLockReadGuard,
        RwLockWriteGuard,
        LockResult
    },
    hash::{Hash, Hasher},
    fmt
};

const LOG_TARGET: &'static str = "shared";

#[derive(Default)]
pub struct Shared<T: ?Sized> {
    inner: Arc<RwLock<T>>
}

impl<T> Shared<T> {
    pub fn new() -> Self
    where T: Default {
        Self {
            inner: Arc::new(RwLock::new(T::default()))
        }
    }

    pub fn share(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner)
        }
    }

    pub fn read(&self) -> LockResult<RwLockReadGuard<T>> {
        self.inner.read()
    }

    pub fn write(&self) -> LockResult<RwLockWriteGuard<T>> {
        self.inner.write()
    } 
}

#[macro_export]
macro_rules! shared_access {
    ($secured_obj:expr $(, $err:expr)?) => {
        shared_access![@_action read, $secured_obj, $($err)?]
    };

    (mut $secured_obj:expr $(, $err:expr)?) => {
        shared_access![@_action write, $secured_obj, $($err)?]
    };

    (@_action $action:ident, $secured_obj:expr, $($err_handler:expr)?) => {
        match $secured_obj.$action() {
            Ok(secured_obj) => secured_obj,
            Err(err) => return shared_access![@_unwrap_err err $(, $err_handler)?]
        }
    };

    (@_unwrap_err $err:expr) => {{
        log::error! {
            target: LOG_TARGET,
            "unable to shared_access secured object: {}", $err
        };

        $crate::Error::Sync($err.to_string()).into()
    }};
    (@_unwrap_err $err:expr, $handler:expr) => {{
        $crate::shared::handle_access_error($err, $handler)
    }};
}

pub fn handle_access_error<E, R, H: FnOnce(E) -> R>(err: E, handler: H) -> R {
    handler(err)
}

impl<T> From<T> for Shared<T> {
    fn from(from: T) -> Self {
        Self {
            inner: Arc::new(RwLock::new(from))
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        shared_access![self, |err| write!(f, "<unable to debug display secured object> [{}]", err)]
            .fmt(f)
    }
}

impl<T: fmt::Display> fmt::Display for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        shared_access![self, |err| write!(f, "<unable to display secured object> [{}]", err)]
            .fmt(f)
    }
}

impl<T: PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        (*shared_access![self]) == (*shared_access![other])
    }
}

impl<T: Eq> Eq for Shared<T> {}

impl<T: Hash> Hash for Shared<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        shared_access![self, |err| panic!("unable to hash secured object: {}", err)]
            .hash(state);
    }
}