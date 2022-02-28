use std::net;

use rand::{thread_rng, Rng};

/// Find a free IP port.
pub fn find_free_port() -> u16 {
	loop {
		let port = thread_rng().gen_range(49152, 65535);
		let addr: net::SocketAddr = format!("127.0.0.1:{}", port).parse().unwrap();
		if net::UdpSocket::bind(addr).is_ok() {
			return port;
		}
	}
}
