//! The platform-agnostic core: a pure state machine plus the runtime that carries out
//! its decisions through a [`Facade`](crate::facade::Facade).
//!
//! [`next`] decides what should happen and returns it as data ([`Effect`]s); the
//! [`runtime`] is the only part that calls the facade.

pub mod effect;
pub mod runtime;
pub mod state;
pub mod transition;

pub use effect::Effect;
pub use runtime::{dispatch, run_app};
pub use state::{Event, State};
pub use transition::next;
