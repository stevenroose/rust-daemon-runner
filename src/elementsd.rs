use std::fmt::Write;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::{fmt, fs, io, mem, process};

use bitcoin::hashes::hex::FromHex;
use bitcoin::{PublicKey, Script};
use liquid_rpc as rpc;
use regex::Regex;

use error::Error;
use runner::{DaemonRunner, RunnerHelper, RuntimeData};
use utils;

pub const CONFIG_FILENAME: &str = "elements.conf";

pub const DEFAULT_VERSION: u64 = 21_00_01;
/// length of the torv3 address
pub const TORV3_ADDR_LEN: usize = 62;

//throw std::runtime_error("ElementsVersion bits parameters malformed, expecting deployment:start:end:period:threshold");
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct EvbParams {
	pub start: Option<u64>,
	pub end: Option<u64>,
	pub period: Option<u64>,
	pub threshold: Option<u64>,
}

impl fmt::Display for EvbParams {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		if let Some(v) = self.start {
			fmt::Display::fmt(&v, f)?;
		}
		f.write_str(":")?;
		if let Some(v) = self.end {
			fmt::Display::fmt(&v, f)?;
		}
		f.write_str(":")?;
		if let Some(v) = self.period {
			fmt::Display::fmt(&v, f)?;
		}
		f.write_str(":")?;
		if let Some(v) = self.threshold {
			fmt::Display::fmt(&v, f)?;
		}
		Ok(())
	}
}

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
	pub listenonion: bool,
	pub discover: bool,
	pub port: Option<u16>,
	pub externalip: Option<String>,
	pub proxy: Option<String>,
	pub bind: Vec<String>,
	pub onlynet: Vec<String>,
	pub txindex: bool,
	pub connect: Vec<String>,
	pub fdefaultconsistencychecks: bool,

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
	pub anyonecanspendaremine: bool,
	pub peginconfirmationdepth: Option<usize>,
	pub signblockscript: Option<Script>,
	pub con_max_block_sig_size: Option<usize>,
	pub con_mandatorycoinbase: Option<String>,
	pub fedpegscript: Option<Script>,
	#[serde(default)]
	pub pak_pubkeys: Vec<(PublicKey, PublicKey)>,
	pub evbparams_dynafed: Option<EvbParams>,
	pub evbparams_taproot: Option<EvbParams>,
	pub con_taproot_signal_start: Option<u64>,
	pub con_dyna_deploy_start: Option<u64>,
	pub con_dyna_deploy_signal: Option<bool>,
	pub con_nminerconfirmationwindow: Option<u64>,
	pub con_nrulechangeactivationthreshold: Option<u64>,
	pub dynamic_epoch_length: Option<u64>,
	pub blockmaxweight: Option<u64>,
	pub mainchain_rpchost: Option<String>,
	pub mainchain_rpcport: Option<u16>,
	pub mainchain_rpcuser: Option<String>,
	pub mainchain_rpcpass: Option<String>,
}
impl Config {
	pub fn write_into(&self, mut w: impl io::Write) -> Result<(), io::Error> {
		//TODO(stevenroose) error?
		assert!(!self.chain.is_empty());

		let version = if self.version > 0 {
			self.version
		} else {
			DEFAULT_VERSION
		};

		let datadir = self.datadir.as_path().to_str().unwrap_or("");
		if datadir.len() > 0 {
			writeln!(w, "datadir={}", datadir)?;
		}

		writeln!(w, "chain={}", self.chain)?;
		writeln!(w, "[{}]", self.chain)?;

		writeln!(w, "fdefaultconsistencychecks={}", self.fdefaultconsistencychecks as u8)?;

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
		for bind in &self.bind {
			writeln!(w, "bind={}", bind)?;
		}
		for onlynet in &self.onlynet {
			writeln!(w, "onlynet={}", onlynet)?;
		}
		if let Some(ref v) = self.externalip {
			if v.len() == TORV3_ADDR_LEN && &v[v.len() - 6..] == ".onion" && version < 21_00_00 {
				// liquid/elements/bitcoin up to version 21 don't support torv3 externalip
				// leave the reference, but commented out
				write!(w, ";")?;
			}
			writeln!(w, "externalip={}", v)?;
		}
		writeln!(w, "txindex={}", self.txindex as u8)?;

		// Consensus variables have no effect for pre-defined chains.
		if self.chain != "liquidv1" {
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
				writeln!(w, "pak={}{}", pair.0, pair.1)?;
			}

			if let Some(ref v) = self.con_mandatorycoinbase {
				writeln!(w, "con_mandatorycoinbase={}", v)?;
			}
			writeln!(w, "con_npowtargetspacing=60")?;
			if let Some(v) = self.con_nminerconfirmationwindow {
				writeln!(w, "con_nminerconfirmationwindow={}", v)?;
			}
			if let Some(v) = self.con_nrulechangeactivationthreshold {
				writeln!(w, "con_nrulechangeactivationthreshold={}", v)?;
			}
			if let Some(v) = self.dynamic_epoch_length {
				writeln!(w, "dynamic_epoch_length={}", v)?;
			}
			if version < 21_00_00 {
				if self.chain == "elementsregtest" {
					// make older versions compatible with
					// https://github.com/ElementsProject/elements/pull/1040
					writeln!(w, "pchmessagestart=5319F20E")?;
				} else if self.chain == "liquidv1test" {
					// make older versions compatible with
					// https://github.com/ElementsProject/elements/pull/1052
					writeln!(w, "pchmessagestart=143EFCB1")?;
				}
			}

			if let Some(v) = self.con_dyna_deploy_start {
				writeln!(w, "con_dyna_deploy_start={}", v)?;
			}
			if let Some(v) = self.con_taproot_signal_start {
				writeln!(w, "con_taproot_signal_start={}", v)?;
			}

			if let Some(ref p) = self.evbparams_dynafed {
				writeln!(w, "evbparams=dynafed:{}", p)?;
			}
			if let Some(ref p) = self.evbparams_taproot {
				writeln!(w, "evbparams=taproot:{}", p)?;
			}
		}

		if let Some(v) = self.blockmaxweight {
			writeln!(w, "blockmaxweight={}", v)?;
		}
		if let Some(v) = self.con_dyna_deploy_signal {
			writeln!(w, "con_dyna_deploy_signal={}", v as u8)?;
		}

		if !self.pak_pubkeys.is_empty() {
			writeln!(w, "enforce_pak=1")?;
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

		writeln!(w, "anyonecanspendaremine={}", self.anyonecanspendaremine as u8)?;
		if let Some(v) = self.peginconfirmationdepth {
			writeln!(w, "peginconfirmationdepth={}", v)?;
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
			writeln!(w, "blockmintxfee={:.8}", v)?;
		}
		if let Some(v) = self.minrelaytxfee {
			writeln!(w, "minrelaytxfee={:.8}", v)?;
		}
		Ok(())
	}
}

#[derive(Default)]
pub struct State {
	pub last_update_tip: Option<(u64, bitcoin::BlockHash)>,
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

pub fn parse_update_tip(msg: &str) -> Option<(u64, bitcoin::BlockHash)> {
	lazy_static! {
		/// The regular expression for UpdateTip messages.
		static ref UPDATE_TIP_REGEX: Regex = Regex::new(
			r".*UpdateTip: new best=([0-9a-f]+) height=([0-9]+) version=.*$"
		).unwrap();
	}

	UPDATE_TIP_REGEX.captures(msg).map(|c| {
		let blockhash = bitcoin::BlockHash::from_hex(
			c.get(1).expect("blockhash missing in UpdateTip").as_str(),
		)
		.expect("invalid blockhash in UpdateTip");
		let height = u64::from_str(c.get(2).expect("height missing in UpdateTip").as_str())
			.expect("invalid height in UpdateTip");
		(height, blockhash)
	})
}

impl Daemon {
	pub fn new(executable: impl Into<PathBuf>, config: Config) -> Result<Daemon, Error> {
		Daemon::named("".into(), executable, config)
	}

	pub fn named(
		name: String,
		executable: impl Into<PathBuf>,
		config: Config,
	) -> Result<Daemon, Error> {
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

	pub fn last_update_tip(&self) -> Option<(u64, bitcoin::BlockHash)> {
		self.runtime_data.as_ref().and_then(|rt| rt.lock().unwrap().state.last_update_tip)
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
			last_update_tip: None,
			stderr: String::new(),
			stdout_file: None,
			error_msgs: Vec::new(),
		}
	}

	/// Notify that the daemon has started.
	fn _notif_started(&mut self, runtime_data: Arc<Mutex<RuntimeData<Self::State>>>) {
		self.runtime_data.replace(runtime_data);
	}

	/// Get the current runtime data.
	fn _get_runtime(&self) -> Option<Arc<Mutex<RuntimeData<Self::State>>>> {
		self.runtime_data.clone()
	}

	fn _process_stdout(name: &str, state: &mut Self::State, line: &str) {
		use std::io::Write;

		if let Some(ref mut file) = state.stdout_file {
			writeln!(file, "{}", line).unwrap();
		}

		if let Some(tip) = parse_update_tip(&line) {
			trace!("Setting new elementsd tip: {:?}", tip);
			state.last_update_tip = Some(tip);
			return;
		}

		lazy_static! {
			/// Regular expression to match for error messages.
			static ref ERROR_REGEX: Regex = Regex::new(r"(?i)ERROR").unwrap();
		}
		if ERROR_REGEX.is_match(line) {
			debug!("{}: found error: {}", name, line);
			state.error_msgs.push(line.to_string());
			return;
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
