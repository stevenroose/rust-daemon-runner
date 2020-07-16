

use std::{process, fmt, io, thread, time, mem};
use std::sync::{Arc, RwLock};
use std::io::{BufRead, Read};

use error::Error;
use ::KillOnDropChild;

pub struct RuntimeData<S> {
	pub state: S,

	process: Option<KillOnDropChild>,
	stdout_thread: Option<thread::JoinHandle<()>>,
	stderr_thread: Option<thread::JoinHandle<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
	Init,
	Running,
	Stopped(process::ExitStatus),
}

/// Methods in this trait are intended to be used only
/// by the [DaemonRunner] implementation.
#[doc(hidden)]
pub trait RunnerHelper {
	type State;

	/// Prepare the daemon for running.
	fn _prepare(&mut self) -> Result<(), Error>;

	/// The command to run.
	fn _command(&self) -> process::Command;

	/// Create the initial state.
    fn _init_state(&self) -> Self::State;

	/// Notify that the daemon has started.
	fn _notif_started(&mut self, runtime_data: Arc<RwLock<RuntimeData<Self::State>>>);

	/// Get the current runtime data.
	fn _get_runtime(&self) -> Option<Arc<RwLock<RuntimeData<Self::State>>>>;

	/// Process some lines of stdout output.
	/// All lines not processed will be discarded.
	fn _process_stdout(state: &mut Self::State, line: &str);

	/// Process some lines of stderr output.
	/// All lines not processed will be discarded.
	fn _process_stderr(state: &mut Self::State, line: &str);
}

fn start_stdread_thread<F, R>(
	name: String, stream: F, rt: Arc<RwLock<RuntimeData<R::State>>>,
) -> thread::JoinHandle<()>
	where F: Read + Sync + Send + 'static, R: RunnerHelper, R::State: Sync + Send + 'static
{
	thread::Builder::new().name(name).spawn(move || {
		thread::sleep(time::Duration::from_secs(1));
		let mut buf_read = io::BufReader::new(stream);
		for line in buf_read.lines() {
			R::_process_stdout(&mut rt.write().unwrap().state, &line.unwrap());
		}
		trace!("Thread {} stopped", thread::current().name().unwrap());
	}).expect(&format!("failed to start std read thread"))
}

pub trait DaemonRunner: RunnerHelper + fmt::Debug + Sized
	where <Self as RunnerHelper>::State: 'static + Send + Sync,
{
	/// The actual startup function.
	/// This intended for internal use only, use [start] and [restart] instead.
	fn _start_up(&self, rt: Arc<RwLock<RuntimeData<Self::State>>>) -> Result<(), Error> {
		info!("Starting daemon {:?}...", self);

		let mut cmd = self._command();
		cmd.stdout(process::Stdio::piped());
		cmd.stderr(process::Stdio::piped());
		debug!("Launching daemon {:?} with command: {:?}", self, cmd);
		let mut process = KillOnDropChild(cmd.spawn()?);
		let pid = process.get().id();

		let mut stdout = process.0.stdout.take().unwrap();
		let mut stderr = process.0.stderr.take().unwrap();

		rt.write().unwrap().process = Some(process);

		let rt_cloned = rt.clone();
		rt.write().unwrap().stdout_thread.replace(start_stdread_thread::<_, Self>(
			format!("stdout thread for {:?}", self), stdout, rt_cloned,
		));
		let rt_cloned = rt.clone();
		rt.write().unwrap().stderr_thread.replace(start_stdread_thread::<_, Self>(
			format!("stderr thread for {:?}", self), stderr, rt_cloned,
		));

		info!("Daemon {:?} started. PID: {}", self, pid);
		Ok(())
	}

	/// Start the daemon for the first time.
	///
	/// Currenly it's not supported to use this method to start with a fresh state.
	/// To restart a daemon after having stopped it, use [restart].
	fn start(&mut self) -> Result<(), Error> {
		let status = self.status()?;
		if status != Status::Init {
			return Err(Error::InvalidState(status));
		}

		self._prepare()?;

		let rt = Arc::new(RwLock::new(RuntimeData {
			process: None,
			stdout_thread: None,
			stderr_thread: None,
			state: self._init_state(),
		}));

		self._start_up(rt.clone())?;
		self._notif_started(rt);
		Ok(())
	}

	/// Restart a daemon using the same state.
	fn restart(&self) -> Result<(), Error> {
		match self.status()? {
			Status::Init => return Err(Error::InvalidState(Status::Init)),
			Status::Running => self.stop()?,
			Status::Stopped(_) => {},
		}

		self._start_up(self._get_runtime().unwrap())
	}

	/// Stop the daemon.
	/// State is preserved so that it can be restarted with [restart].
	fn stop(&self) -> Result<(), Error> {
		match self.status()? {
			Status::Init => return Err(Error::InvalidState(Status::Init)),
			Status::Running => {},
			Status::Stopped(_) => return Ok(()),
		}

		let rt_ref = self._get_runtime().unwrap();
		let mut rt = rt_ref.write().unwrap();

		info!("Stopping daemon {:?}...", self);
		rt.process.as_mut().unwrap().get_mut().kill()?;

		info!("Daemon {:?} stopped", self);
		Ok(())
	}

	/// The the running status of the daemon.
	fn status(&self) -> Result<Status, Error> {
		let rt = match self._get_runtime() {
			Some(rt) => rt,
			None => return Ok(Status::Init),
		};

		let mut lock = rt.write().unwrap();
		match lock.process.as_mut().unwrap().0.try_wait()? {
			None => Ok(Status::Running),
			Some(c) => Ok(Status::Stopped(c)),
		}
	}

	/// Get the OS process ID of the daemon.
	fn pid(&self) -> Option<u32> {
		self._get_runtime().map(|rt| rt.read().unwrap().process.as_ref().unwrap().get().id())
	}

	//TODO(stevenroose) try make a generic method
	// fn state(&self) -> Option<XXX> where XXX has Deref<Self::State> somehow
}
