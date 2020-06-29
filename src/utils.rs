
use std::net;

use rand::{thread_rng, Rng};

use regex::Regex;

pub trait RegexUtils: AsRef<str> {
	/// Get the first group from the first capture of the regex.
	fn rx_single<'a>(&self, s: &'a str) -> Option<&'a str> {
		Regex::new(self.as_ref()).expect("invalid regex").captures(s).map(|c| {
			c.get(1).unwrap().as_str()
		})
	}
	
	/// Get the first n groups from the first capture of the regex.
	/// Will return a vector with n+1 elements, the first being the entire capture.
	fn rx_n<'a>(&self, n: usize, s: &'a str) -> Option<Vec<Option<&'a str>>> {
		Regex::new(self.as_ref()).expect("invalid regex").captures(s).map(|c| {
			(0..n+1).map(|i| c.get(i).map(|m| m.as_str())).collect()
		})
	}
}

impl RegexUtils for &'static str {}

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
