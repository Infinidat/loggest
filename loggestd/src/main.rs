use future::Future;
use tokio::net::unix::UnixListener;
use tokio::prelude::*;
mod codec;
mod session;
use env_logger::{self, Env};
use log::{error, info};

fn main() {
    env_logger::from_env(Env::default().default_filter_or("info")).init();

    let socket = UnixListener::bind("/run/user/1000/loggestd.sock").unwrap().incoming();
    let server = socket
        .for_each(|socket| {
            info!("Connected: {:?}", socket);
            tokio::spawn(session::LoggestdSession::new(socket).map_err(|e| {
                error!("Session error: {}", e);
            }));
            Ok(())
        })
        .map_err(|e| {
            error!("Error accepting: {:?}", e);
        });

    tokio::run(server);
}
