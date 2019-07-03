use env_logger::{self, Env};
use future::Future;
use futures::future::lazy;
use log::{error, info};
use std::env;
use tokio::net::unix::UnixListener;
use tokio::prelude::*;

mod codec;
mod log_file;
mod session;
mod usage_monitor;

fn main() {
    env_logger::from_env(Env::default().default_filter_or("info"))
        .default_format_timestamp(false)
        .init();

    let addr = env::var("LOGGESTD_SOCKET").unwrap_or_else(|_| "/run/loggestd.sock".into());
    info!("Listening in {}", addr);
    let socket = UnixListener::bind(addr).unwrap().incoming();
    let server = lazy(move || {
        tokio::spawn(usage_monitor::UsageMonitor::default().map_err(|e| {
            error!("Usage monitor error: {}", e);
        }));

        socket
            .for_each(|socket| {
                info!("Connected: {:?}", socket);
                tokio::spawn(session::LoggestdSession::new(socket).map_err(|e| {
                    error!("Session error: {}", e);
                }));
                Ok(())
            })
            .map_err(|e| {
                error!("Error accepting: {:?}", e);
            })
    });

    tokio::run(server);
}
