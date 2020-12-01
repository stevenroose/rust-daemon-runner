use std::{error, fmt, io, process};

use bitcoincore_rpc;

#[derive(Debug)]
pub enum Error {
	/// An I/O error.
	Io(io::Error),
	/// Invalid configuration provided.
	Config(&'static str),
	/// A Bitcoin Core RPC error.
	BitcoinRpc(bitcoincore_rpc::Error),
	/// A Liquid RPC error.
	LiquidRpc(liquid_rpc::Error),
	/// Any other error.
	Custom(&'static str),
	/// The daemon is not in the appropriate state for this action.
	InvalidState(::Status),
	/// Error running a command.
	RunCommand(io::Error, process::Command),
}

impl From<io::Error> for Error {
	fn from(e: io::Error) -> Error {
		Error::Io(e)
	}
}

impl From<bitcoincore_rpc::Error> for Error {
	fn from(e: bitcoincore_rpc::Error) -> Error {
		Error::BitcoinRpc(e)
	}
}

impl From<liquid_rpc::Error> for Error {
	fn from(e: liquid_rpc::Error) -> Error {
		Error::LiquidRpc(e)
	}
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		fmt::Debug::fmt(self, f)
	}
}

impl error::Error for Error {
	fn source(&self) -> Option<&(dyn error::Error + 'static)> {
		match *self {
			Error::Io(ref e) => Some(e),
			Error::BitcoinRpc(ref e) => Some(e),
			Error::LiquidRpc(ref e) => Some(e),
			Error::RunCommand(ref e, ..) => Some(e),
			Error::Config(_) | Error::Custom(_) | Error::InvalidState(_) => None,
		}
	}
}
