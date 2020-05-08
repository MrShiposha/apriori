use std::{
    fmt,
    hash::{Hash, Hasher},
    sync::{Arc, Weak, LockResult, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

const LOG_TARGET: &'static str = "shared";

#[derive(Default)]
pub struct Shared<T: ?Sized>(Arc<RwLock<T>>);

impl<T: ?Sized> Shared<T> {
    pub fn new() -> Self
    where
        T: Default,
    {
        Self(Arc::new(RwLock::new(T::default())))
    }

    pub fn share(&self) -> Self {
        Self(Arc::clone(&self.0))
    }

    pub fn share_weak(&self) -> SharedWeak<T> {
        SharedWeak(Arc::downgrade(&self.0))
    }

    pub fn read(&self) -> LockResult<RwLockReadGuard<T>> {
        self.0.read()
    }

    pub fn write(&self) -> LockResult<RwLockWriteGuard<T>> {
        self.0.write()
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

pub struct SharedWeak<T: ?Sized>(Weak<RwLock<T>>);

impl<T: ?Sized> SharedWeak<T> {
    pub fn upgrade(&self) -> Option<Shared<T>> {
        self.0.upgrade().map(|inner| Shared(inner))
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
        Self(Arc::new(RwLock::new(from)))
    }
}

impl<T: fmt::Debug> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        shared_access![self, |err| write!(
            f,
            "<unable to debug display secured object> [{}]",
            err
        )]
        .fmt(f)
    }
}

impl<T: fmt::Display> fmt::Display for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        shared_access![self, |err| write!(
            f,
            "<unable to display secured object> [{}]",
            err
        )]
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
        shared_access![self, |err| panic!("unable to hash secured object: {}", err)].hash(state);
    }
}
