use std::fmt::Display;

use cfg_if::cfg_if;
use worker::Error;

cfg_if! {
	if #[cfg(feature = "console_error_panic_hook")] {
		pub(crate) use console_error_panic_hook::set_once as set_panic_hook;
	} else {
		pub(crate) fn set_panic_hook() {}
	}
}

pub(crate) trait Context<T> {
	fn context(self, msg: impl Display) -> Result<T, Error>;
}

impl<T> Context<T> for Option<T> {
	fn context(self, msg: impl Display) -> Result<T, Error> {
		self.ok_or_else(|| Error::RustError(msg.to_string()))
	}
}

impl<T, E: Display> Context<T> for Result<T, E> {
	fn context(self, msg: impl Display) -> Result<T, Error> {
		self.map_err(|e| Error::RustError(format!("{msg}: {e}")))
	}
}
