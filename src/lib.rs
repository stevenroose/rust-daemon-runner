
#![allow(unused)]

pub extern crate bitcoincore_rpc;
pub extern crate bitcoin;
pub extern crate liquid_rpc;

#[macro_use]
extern crate log;
extern crate rand;
extern crate regex;
#[macro_use]
extern crate serde;


pub mod bitcoind;
pub mod elementsd;
pub mod runner;
pub mod utils;
mod error;

pub use error::Error;
pub use runner::{DaemonRunner, Status};


use std::{ops, process};

/// An wrapper for child that is killed when it's dropped.
pub(crate) struct KillOnDropChild(process::Child);

impl KillOnDropChild {
	pub fn get(&self) -> &process::Child {
		&self.0
	}
	pub fn get_mut(&mut self) -> &mut process::Child {
		&mut self.0
	}
}

impl ops::Drop for KillOnDropChild {
	fn drop(&mut self) {
		// We don't care about the error here because we probably
		// already safely stopped the process.
		let _ = self.0.kill();
	}
}
