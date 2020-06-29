

extern crate daemon_runner;
extern crate fern;

use std::{time, thread};

use daemon_runner::bitcoincore_rpc::RpcApi;

use daemon_runner::bitcoin;
use daemon_runner::{DaemonRunner, bitcoind};

fn setup_logger() {
	fern::Dispatch::new()
		.format(|out, message, rec| {
			out.finish(format_args!("[{}][{}] {}", 
				rec.level(),
				rec.module_path().unwrap(),
				message,
			))
		})
		.level(log::LevelFilter::Trace)
		.level_for("hyper", log::LevelFilter::Off)
		.chain(std::io::stderr())
		.apply()
		.expect("error setting up logger");
}

fn main() {
	setup_logger();

	let mut d = bitcoind::Daemon::new("/home/steven/bin/bitcoind", bitcoind::Config {
		network: Some(bitcoin::Network::Regtest),
		datadir: "/home/steven/tmp/daemon_runner_test".into(),
		..Default::default()
	}).unwrap();

	println!("starting...");
	d.start().unwrap();
	println!("started!");


	thread::sleep(time::Duration::from_secs(10));

	let rpc = d.rpc_client().unwrap().unwrap();
	println!("tip: {}", rpc.get_best_block_hash().unwrap());


	println!("stopping...");
	d.stop().unwrap();
	println!("stopped!");

}
