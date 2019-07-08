#[cfg(windows)]
use std::net::SocketAddr;
use std::path::PathBuf;
use structopt::StructOpt;

/// A basic example
#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
pub struct Opt {
    /// Output directory
    #[structopt(short, long, parse(from_os_str))]
    pub directory: PathBuf,

    /// Unix socket to listen to
    #[cfg(unix)]
    #[structopt(
        long,
        parse(from_os_str),
        default_value = "/run/loggestd.sock",
        env = "LOGGESTD_SOCKET"
    )]
    pub unix_socket: PathBuf,

    /// Address to listen to
    #[cfg(windows)]
    #[structopt(long, default_value = "127.0.0.1:1337", env = "LOGGESTD_LISTEN")]
    pub listen: SocketAddr,
}
