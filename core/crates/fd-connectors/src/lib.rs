//! Cloud connections, connector routes, and CloudRemote clients.

pub mod client;
pub mod connection;
pub mod rclone;
pub mod routes;
pub mod test;

pub use client::{CloudRemoteClient, Session, TrailerError};
pub use connection::{ConnectionError, validate_connection};
pub use routes::{RouteError, RouteResolver, resolve};
pub use test::{ConnectionTester, StoredTestResult};

pub mod ownership;

pub mod object_io;

pub mod oauth;

pub mod route_change;

pub mod sweep;
