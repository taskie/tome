pub mod error;
pub mod openapi;
pub mod routes;
pub mod server;

#[cfg(feature = "lambda")]
pub mod lambda;

#[cfg(feature = "lambda")]
pub use lambda::run_lambda;
pub use server::{build_router, serve};
