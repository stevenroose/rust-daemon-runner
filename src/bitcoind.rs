use std::fmt::Write;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::{fmt, fs, io, mem, process};

use bitcoin;
use bitcoincore_rpc::{self as rpc, RpcApi};
use regex::Regex;

use error::Error;
use runner::{DaemonRunner, RunnerHelper, RuntimeData};
use utils;

pub const CONFIG_FILENAME: &str = "bitcoin.conf";

pub const DEFAULT_VERSION: u64 = 21_00_00;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
	/// This field is not present in the config but is necessary to
	/// know the config file format that needs to be written.
	/// Two digits per section, 4 sections: 0.18.1.0 => 18_01_00
	pub version: u64,

	pub datadir: PathBuf,
	pub network: Option<bitcoin::Network>,
	pub fdefaultconsistencychecks: Option<bool>,
	pub debug: bool,
	pub printtoconsole: bool,
	pub daemon: bool,
	pub listen: bool,
	pub listenonion: bool,
	pub discover: bool,
	pub port: Option<u16>,
	pub proxy: Option<String>,
	pub txindex: bool,
	pub connect: Vec<String>,
	pub addnodes: Vec<String>,

	pub rpccookie: Option<String>,
	pub rpcport: Option<u16>,
	pub rpcuser: Option<String>,
	pub rpcpass: Option<String>,

	pub disablewallet: Option<bool>,
	pub dbcache: Option<u32>,
	//TODO(stevenroose) enum?
	pub addresstype: Option<String>,
	pub blockmintxfee: Option<f64>,
	pub minrelaytxfee: Option<f64>,
	pub fallbackfee: Option<f64>,
}
impl Config {
	pub fn write_into<W: io::Write>(&self, mut w: W) -> Result<(), io::Error> {
		let version = if self.version > 0 {
			self.version
		} else {
			DEFAULT_VERSION
		};

		let datadir = self.datadir.as_path().to_str().unwrap_or("");
		if datadir.len() > 0 {
			writeln!(w, "datadir={}", datadir)?;
		}

		match self.network {
			Some(bitcoin::Network::Bitcoin) | None => {}
			Some(bitcoin::Network::Testnet) => {
				writeln!(w, "testnet=1")?;
				if version > 17_00_00 {
					writeln!(w, "[testnet]")?;
				}
			}
			Some(bitcoin::Network::Regtest) => {
				writeln!(w, "regtest=1")?;
				if version > 17_00_00 {
					writeln!(w, "[regtest]")?;
				}
			}
		}

		if let Some(v) = self.fdefaultconsistencychecks {
			//fdefaultconsistencychecks manages the default for checkblockindex and checkmempool
			//elements reads fdefaultconsistencychecks in chainparams, but bitcoin doesn't
			//so we expand the contents
			writeln!(w, "checkblockindex={}", v as u8)?;
			writeln!(w, "checkmempool={}", v as u8)?;
		}

		writeln!(w, "debug={}", self.debug as u8)?;
		writeln!(w, "printtoconsole={}", self.printtoconsole as u8)?;
		writeln!(w, "daemon={}", self.daemon as u8)?;
		writeln!(w, "listen={}", self.listen as u8)?;
		writeln!(w, "listenonion={}", self.listenonion as u8)?;
		writeln!(w, "discover={}", self.discover as u8)?;

		if let Some(p) = self.port {
			writeln!(w, "port={}", p)?;
		}
		if let Some(ref v) = self.proxy {
			writeln!(w, "proxy={}", v)?;
		}
		writeln!(w, "txindex={}", self.txindex as u8)?;

		for connect in &self.connect {
			writeln!(w, "connect={}", connect)?;
		}
		for addnode in &self.addnodes {
			writeln!(w, "addnode={}", addnode)?;
		}

		// RPC details
		if self.rpccookie.is_some() || self.rpcuser.is_some() {
			writeln!(w, "server=1")?;
		}
		if let Some(ref cf) = self.rpccookie {
			writeln!(w, "rpccookiefile={}", cf)?;
		}
		if let Some(p) = self.rpcport {
			writeln!(w, "rpcallowip=127.0.0.1")?;
			writeln!(w, "rpcbind=127.0.0.1")?;
			writeln!(w, "rpcport={}", p)?;
		}
		if let Some(ref u) = self.rpcuser {
			writeln!(w, "rpcuser={}", u)?;
		}
		if let Some(ref p) = self.rpcpass {
			writeln!(w, "rpcpassword={}", p)?;
		}

		if let Some(p) = self.disablewallet {
			writeln!(w, "disablewallet={}", p as u8)?;
		}

		if let Some(p) = self.dbcache {
			writeln!(w, "dbcache={}", p)?;
		}

		if let Some(ref v) = self.addresstype {
			writeln!(w, "addresstype={}", v)?;
		}
		if let Some(v) = self.blockmintxfee {
			writeln!(w, "blockmintxfee={:.8}", v)?;
		}
		if let Some(v) = self.minrelaytxfee {
			writeln!(w, "minrelaytxfee={:.8}", v)?;
		}
		if let Some(v) = self.fallbackfee {
			writeln!(w, "fallbackfee={:.8}", v)?;
		}
		Ok(())
	}
}

#[derive(Default)]
pub struct State {
	/// Buffer holding all stderr output.
	pub stderr: String,

	/// For older versions, write stdout to this file.
	pub stdout_file: Option<File>,

	/// Error messages produced during runtime.
	error_msgs: Vec<String>,
}

pub struct Daemon {
	name: String,
	executable: PathBuf,
	config: Config,

	/// The path of the written config file.
	/// [None] before it has been written.
	config_file: Option<PathBuf>,

	runtime_data: Option<Arc<Mutex<RuntimeData<State>>>>,
}

impl Daemon {
	pub fn new<P: Into<PathBuf>>(executable: P, config: Config) -> Result<Daemon, Error> {
		if !config.datadir.is_absolute() {
			return Err(Error::Config("datadir should be an absolute path"));
		}

		Ok(Daemon {
			name: "".into(),
			executable: executable.into(),
			config: config,

			config_file: None,
			runtime_data: None,
		})
	}

	pub fn set_name(&mut self, name: String) {
		self.name = name;
	}

	pub fn datadir(&self) -> &Path {
		self.config.datadir.as_path()
	}

	/// Get the RPC info.
	///
	/// Don't call this method before calling [start].
	pub fn rpc_info(&self) -> Option<(String, rpc::Auth)> {
		let url = format!("http://127.0.0.1:{}", self.config.rpcport?);
		let auth = if let Some(ref c) = self.config.rpccookie {
			rpc::Auth::CookieFile(c.clone().into())
		} else if let Some(ref u) = self.config.rpcuser {
			let pass = self.config.rpcpass.as_ref()?.clone();
			rpc::Auth::UserPass(u.clone(), pass)
		} else {
			return None;
		};
		Some((url, auth))
	}

	/// Get an RPC client.
	///
	/// Don't call this method before calling [start].
	pub fn rpc_client(&self) -> Option<Result<rpc::Client, rpc::Error>> {
		let (url, port) = self.rpc_info()?;
		Some(rpc::Client::new(url, port))
	}

	pub fn take_stderr(&self) -> String {
		self.runtime_data
			.as_ref()
			.map(|rt| mem::replace(&mut rt.lock().unwrap().state.stderr, String::new()))
			.unwrap_or_default()
	}

	pub fn take_error_msgs(&self) -> Vec<String> {
		self.runtime_data
			.as_ref()
			.map(|rt| mem::replace(&mut rt.lock().unwrap().state.error_msgs, Vec::new()))
			.unwrap_or_default()
	}
}

impl RunnerHelper for Daemon {
	type State = State;

	fn _prepare(&mut self) -> Result<(), Error> {
		if self.config_file.is_some() {
			return Ok(());
		}

		// Make sure the datadir exists.
		fs::create_dir_all(&self.config.datadir)?;

		// Write the config file once and store the path.
		let mut path: PathBuf = self.config.datadir.clone().into();
		path.push(CONFIG_FILENAME);
		let mut file = File::create(&path)?;
		self.config.write_into(&mut file)?;
		self.config_file = Some(path);
		Ok(())
	}

	fn _command(&self) -> process::Command {
		let mut cmd = process::Command::new(self.executable.clone());
		cmd.args(&[
			&format!("-conf={}", self.config_file.as_ref().unwrap().as_path().display()),
			"-printtoconsole=1",
		]);
		cmd
	}

	fn _init_state(&self) -> Self::State {
		State {
			stderr: String::new(),

			stdout_file: if self.config.version < 18_00_00 {
				let mut path = self.config.datadir.clone();
				path.push("stdout.log");
				debug!("Writing bitcoind stdout to {}", path.display());
				Some(File::create(&path).expect("failed to create stdout log file"))
			} else {
				None
			},
			error_msgs: Vec::new(),
		}
	}

	fn _notif_started(&mut self, runtime_data: Arc<Mutex<RuntimeData<Self::State>>>) {
		self.runtime_data.replace(runtime_data);
	}

	fn _get_runtime(&self) -> Option<Arc<Mutex<RuntimeData<Self::State>>>> {
		self.runtime_data.clone()
	}

	fn _process_stdout(name: &str, state: &mut Self::State, line: &str) {
		use std::io::Write;

		if let Some(ref mut file) = state.stdout_file {
			writeln!(file, "{}", line).unwrap();
		}

		lazy_static! {
			/// Regular expression to match for error messages.
			static ref ERROR_REGEX: Regex = Regex::new(r"(?i)ERROR").unwrap();
		}
		if ERROR_REGEX.is_match(line) {
			debug!("{}: found error: {}", name, line);
			state.error_msgs.push(line.to_string());
		}
	}

	fn _process_stderr(state: &mut Self::State, line: &str) {
		writeln!(&mut state.stderr, "{}", line).unwrap();
	}
}

impl DaemonRunner for Daemon {}

impl fmt::Debug for Daemon {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		if self.name.is_empty() {
			write!(f, "<unnamed> bitcoind")
		} else {
			write!(f, "bitcoind \"{}\"", self.name)
		}
	}
}
