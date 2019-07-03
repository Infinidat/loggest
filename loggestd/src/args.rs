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
    #[structopt(
        long,
        parse(from_os_str),
        default_value = "/run/loggestd.sock",
        env = "LOGGESTD_SOCKET"
    )]
    pub unix_socket: PathBuf,
}
