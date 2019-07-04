use env_logger::{self, Env};
use future::Future;
use log::{debug, error, info};
use std::fs;
use std::sync::Arc;
use structopt::StructOpt;
use tokio::net::unix::UnixListener;
use tokio::prelude::*;
use tokio::runtime::Runtime;

mod args;
mod codec;
mod log_file;
mod session;
mod usage_monitor;

fn main() {
    let opt = Arc::new(args::Opt::from_args());

    env_logger::from_env(Env::default().default_filter_or("info"))
        .default_format_timestamp(false)
        .init();

    if opt.unix_socket.exists() {
        debug!("Deleting {}", opt.unix_socket.display());
        fs::remove_file(&opt.unix_socket).unwrap();
    }

    info!("Listening in {}", opt.unix_socket.display());
    let socket = UnixListener::bind(&opt.unix_socket).unwrap().incoming();
    info!("Logging to {}", opt.directory.display());

    let server = socket
        .for_each({
            let opt = opt.clone();
            {
                move |socket| {
                    info!("Connected: {:?}", socket);
                    tokio::spawn(session::LoggestdSession::new(socket, opt.clone()).map_err(|e| {
                        error!("Session error: {}", e);
                    }));
                    Ok(())
                }
            }
        })
        .map_err(|e| {
            error!("Error accepting: {:?}", e);
        });

    let ctrl_c = tokio_signal::ctrl_c()
        .flatten_stream()
        .into_future()
        .map_err(|(e, _)| error!("Error setting up Ctrl+C handler: {}", e))
        .map(|_| info!("Ctrl+C received"));

    #[cfg(unix)]
    let ctrl_c = {
        use tokio_signal::unix::{Signal, SIGTERM};
        let sigterm = Signal::new(SIGTERM)
            .flatten_stream()
            .into_future()
            .map_err(|(e, _)| error!("Error setting up SIGTERM handler: {}", e))
            .map(|_| info!("SIGTERM received"));

        ctrl_c.select(sigterm)
    };

    let mut rt = Runtime::new().unwrap();
    rt.spawn(server);
    rt.spawn(usage_monitor::UsageMonitor::new(&opt.directory).map_err(|e| {
        error!("Usage monitor error: {}", e);
    }));

    rt.block_on(ctrl_c).ok();

    info!("Server exited");
}
