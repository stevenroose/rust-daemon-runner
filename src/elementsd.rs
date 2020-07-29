
use std::{io, process, fmt, fs, mem};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::fmt::Write;
use std::str::FromStr;

use bitcoin::{PublicKey, Script};
use bitcoin::hashes::hex::FromHex;
use liquid_rpc as rpc;

use error::Error;
use utils::{self, RegexUtils};
use runner::{DaemonRunner, RunnerHelper, RuntimeData};

pub const CONFIG_FILENAME: &str = "elements.conf";

/// Older liquidd nodes were released as 2.x.x and 3.x.x versions.
pub const OLD_LIQUID_VERSION: u64 = 2_00_00_00;
/// The dynafed activation version.
pub const DYNAFED_VERSION: u64 = 18_01_00;

pub const DEFAULT_VERSION: u64 = 18_01_00;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
	/// This field is not present in the config but is necessary to
	/// know the config file format that needs to be written.
	/// Two digits per section, 4 sections: 0.18.1.0 => 18_01_00
	pub version: u64,

	pub datadir: PathBuf,
	pub debug: bool,
	pub printtoconsole: bool,
	pub daemon: bool,
	pub listen: bool,
	pub port: Option<u16>,
	pub txindex: bool,
	pub connect: Vec<String>,

	pub rpccookie: Option<String>,
	pub rpcport: Option<u16>,
	pub rpcuser: Option<String>,
	pub rpcpass: Option<String>,

	//TODO(stevenroose) enum?
	pub addresstype: Option<String>,
	pub blockmintxfee: Option<f64>,
	pub minrelaytxfee: Option<f64>,

	// Elements stuff:
	pub chain: String,
	pub validatepegin: bool,
	pub signblockscript: Option<Script>,
	pub con_max_block_sig_size: Option<usize>,
	pub fedpegscript: Option<Script>,
	#[serde(default)]
	pub pak_pubkeys: Vec<(PublicKey, PublicKey)>,
	pub con_dyna_deploy_start: Option<u32>,
	pub con_nminerconfirmationwindow: Option<u32>,
	pub con_nrulechangeactivationthreshold: Option<u32>,
	pub mainchain_rpchost: Option<String>,
	pub mainchain_rpcport: Option<u16>,
	pub mainchain_rpcuser: Option<String>,
	pub mainchain_rpcpass: Option<String>,
}
impl Config {
	pub fn write_into<W: io::Write>(&self, mut w: W) -> Result<(), io::Error> {
		//TODO(stevenroose) error?
		assert!(!self.chain.is_empty());

		let version = if self.version > 0 {
			self.version
		} else {
			DEFAULT_VERSION
		};

		writeln!(w, "datadir={}", self.datadir.as_path().to_str().unwrap_or("<INVALID>"))?;

		writeln!(w, "chain={}", self.chain)?;
		if version >= 17_00_00 && version < OLD_LIQUID_VERSION {
			writeln!(w, "[{}]", self.chain)?;
		}

		writeln!(w, "debug={}", self.debug as u8)?;
		writeln!(w, "printtoconsole={}", self.printtoconsole as u8)?;
		writeln!(w, "daemon={}", self.daemon as u8)?;
		writeln!(w, "listen={}", self.listen as u8)?;
		if let Some(p) = self.port {
			writeln!(w, "port={}", p)?;
		}
		writeln!(w, "txindex={}", self.txindex as u8)?;
		if let Some(ref v) = self.signblockscript {
			writeln!(w, "signblockscript={:x}", v)?;
		}
		if let Some(v) = self.con_max_block_sig_size {
			writeln!(w, "con_max_block_sig_size={}", v)?;
		}
		if let Some(ref v) = self.fedpegscript {
			writeln!(w, "fedpegscript={:x}", v)?;
		}
		for pair in &self.pak_pubkeys {
			if version >= DYNAFED_VERSION && version < OLD_LIQUID_VERSION {
				writeln!(w, "pak={}{}", pair.0, pair.1)?;
			} else {
				writeln!(w, "pak={}:{}", pair.0, pair.1)?;
			}
		}
		if let Some(v) = self.con_dyna_deploy_start {
			writeln!(w, "con_dyna_deploy_start={}", v)?;
		}
		if let Some(v) = self.con_nminerconfirmationwindow {
			writeln!(w, "con_nminerconfirmationwindow={}", v)?;
		}
		if let Some(v) = self.con_nrulechangeactivationthreshold {
			writeln!(w, "con_nrulechangeactivationthreshold={}", v)?;
		}

		for connect in &self.connect {
			writeln!(w, "connect={}", connect)?;
		}

		// RPC details
		if self.rpccookie.is_some() || self.rpcuser.is_some() {
			writeln!(w, "server=1")?;
		}
		if let Some(ref cf) = self.rpccookie {
			writeln!(w, "rpccookiefile={}", cf)?;
		}
		if let Some(p) = self.rpcport {
			writeln!(w, "rpcport={}", p)?;
		}
		if let Some(ref u) = self.rpcuser {
			writeln!(w, "rpcuser={}", u)?;
		}
		if let Some(ref p) = self.rpcpass {
			writeln!(w, "rpcpassword={}", p)?;
		}

		writeln!(w, "validatepegin={}", self.validatepegin as u8)?;
		if self.validatepegin {
			if let Some(ref v) = self.mainchain_rpchost {
				writeln!(w, "mainchainrpchost={}", v)?;
			}
			if let Some(ref v) = self.mainchain_rpcport {
				writeln!(w, "mainchainrpcport={}", v)?;
			}
			if let Some(ref v) = self.mainchain_rpcuser {
				writeln!(w, "mainchainrpcuser={}", v)?;
			}
			if let Some(ref v) = self.mainchain_rpcpass {
				writeln!(w, "mainchainrpcpassword={}", v)?;
			}
		}

		if let Some(ref v) = self.addresstype {
			writeln!(w, "addresstype={}", v)?;
		}
		if let Some(v) = self.blockmintxfee {
			writeln!(w, "blockmintxfee={}", v)?;
		}
		if let Some(v) = self.minrelaytxfee {
			writeln!(w, "minrelaytxfee={}", v)?;
		}
		Ok(())
	}
}

#[derive(Default)]
pub struct State {
	pub last_update_tip: Option<(u32, bitcoin::BlockHash)>,
	/// Buffer holding all stderr output.
	pub stderr: String,
}

pub struct Daemon {
	name: String,
	executable: PathBuf,
	config: Config,

	/// The path of the written config file.
	/// [None] before it has been written.
	config_file: Option<PathBuf>,

	runtime_data: Option<Arc<RwLock<RuntimeData<State>>>>,
}

const UPDATE_TIP_REGEX: &str = r".*UpdateTip: new best=([0-9a-f]+) height=([0-9]+) version=.*$";
pub fn parse_update_tip(msg: &str) -> Option<(u32, bitcoin::BlockHash)> {
	UPDATE_TIP_REGEX.rx_n(2, msg).map(|m| {
		let blockhash = bitcoin::BlockHash::from_hex(
			m[1].expect("blockhash missing in UpdateTip")
		).expect("invalid blockhash in UpdateTip");
		let height = u32::from_str(
			m[2].expect("height missing in UpdateTip")
		).expect("invalid height in UpdateTip");
		(height, blockhash)
	})
}

impl Daemon {
	pub fn new<P: Into<PathBuf>>(executable: P, config: Config) -> Result<Daemon, Error> {
		Daemon::named("".into(), executable, config)
	}

	pub fn named<P: Into<PathBuf>>(name: String, executable: P, config: Config) -> Result<Daemon, Error> {
		if !config.datadir.is_absolute() {
			return Err(Error::Config("datadir should be an absolute path"));
		}

		Ok(Daemon {
			name: name,
			executable: executable.into(),
			config: config,

			config_file: None,
			runtime_data: None,
		})
	}

	pub fn datadir(&self) -> &Path {
		self.config.datadir.as_path()
	}

	pub fn last_update_tip(&self) -> Option<(u32, bitcoin::BlockHash)> {
		self.runtime_data.as_ref().and_then(|rt|
			rt.read().unwrap().state.last_update_tip
		)
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

	pub fn rpc_client(&self) -> Option<Result<rpc::Client, rpc::Error>> {
		let (url, auth) = self.rpc_info()?;
		Some(rpc::Client::new(url, auth))
	}

	pub fn take_stderr(&self) -> String {
		self.runtime_data.as_ref().map(|rt|
			mem::replace(&mut rt.write().unwrap().state.stderr, String::new())
		).unwrap_or_default()
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
			last_update_tip: None,
			stderr: String::new(),
		}
	}

	/// Notify that the daemon has started.
	fn _notif_started(&mut self, runtime_data: Arc<RwLock<RuntimeData<Self::State>>>) {
		self.runtime_data.replace(runtime_data);
	}

	/// Get the current runtime data.
	fn _get_runtime(&self) -> Option<Arc<RwLock<RuntimeData<Self::State>>>> {
		self.runtime_data.clone()
	}

	fn _process_stdout(state: &mut Self::State, line: &str) {
		if let Some(tip) = parse_update_tip(&line) {
			trace!("Setting new elementsd tip: {:?}", tip);
			state.last_update_tip = Some(tip);
		}
	}

	fn _process_stderr(state: &mut Self::State, line: &str) {
		trace!("stderr line of elementsd: {}", line);
		writeln!(&mut state.stderr, "{}", line).unwrap();
	}
}

impl DaemonRunner for Daemon {}

impl fmt::Debug for Daemon {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		if self.name.is_empty() {
			write!(f, "<unnamed> elementsd")
		} else {
			write!(f, "elementsd \"{}\"", self.name)
		}
	}
}

