#![allow(unused)]

pub extern crate bitcoin;
pub extern crate bitcoincore_rpc;
pub extern crate liquid_rpc;

#[macro_use]
extern crate log;
extern crate rand;
extern crate regex;
#[macro_use]
extern crate serde;
#[macro_use]
extern crate lazy_static;

pub mod bitcoind;
pub mod elementsd;
mod error;
pub mod runner;
pub mod utils;

pub use error::Error;
pub use runner::{DaemonRunner, Status};
