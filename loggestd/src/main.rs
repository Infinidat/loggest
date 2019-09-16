#[cfg(windows)]
use crossbeam_channel;
use env_logger::{self, Env};
#[cfg(windows)]
use futures::{future, Future};
#[cfg(unix)]
use log::debug;
use log::{error, info};
#[cfg(windows)]
use std::ffi::OsString;
#[cfg(unix)]
use std::fs;
use std::sync::Arc;
#[cfg(windows)]
use std::time::Duration;
use structopt::StructOpt;
#[cfg(unix)]
use tokio::net::unix::UnixListener;
#[cfg(windows)]
use tokio::net::TcpListener;
use tokio::prelude::*;
use tokio::runtime::Runtime;
#[cfg(windows)]
use windows_service;
#[cfg(windows)]
use windows_service::service;
#[cfg(windows)]
use windows_service::service_control_handler;

#[cfg(windows)]
windows_service::define_windows_service!(service_entry_point, service_main);

mod args;
mod codec;
mod log_file;
mod session;
mod usage_monitor;

#[cfg(windows)]
const SERVICE_NAME: &str = "Loggest";
#[cfg(windows)]
const SERVICE_TYPE: service::ServiceType = service::ServiceType::OWN_PROCESS;

#[cfg(windows)]
fn wait_for_recv(recv: crossbeam_channel::Receiver<()>) -> impl Future<Item = (), Error = ()> {
    recv.recv().ok();
    future::ok(())
}

enum CrossbeamReceiverOption {
    #[cfg(unix)]
    None,
    #[cfg(windows)]
    Receiver(crossbeam_channel::Receiver<()>),
}

fn run_loggest(stop_recv_option: CrossbeamReceiverOption) {
    let opt = Arc::new(args::Opt::from_args());

    env_logger::from_env(Env::default().default_filter_or("info"))
        .default_format_timestamp(false)
        .init();

    #[cfg(unix)]
    let socket = {
        if opt.unix_socket.exists() {
            debug!("Deleting {}", opt.unix_socket.display());
            fs::remove_file(&opt.unix_socket).unwrap();
        }

        info!("Listening in {}", opt.unix_socket.display());

        UnixListener::bind(&opt.unix_socket).unwrap().incoming()
    };

    #[cfg(windows)]
    let socket = {
        info!("Listening in {}", opt.listen);
        TcpListener::bind(&opt.listen).unwrap().incoming()
    };

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

    #[cfg(unix)]
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

    #[cfg(unix)]
    rt.block_on(ctrl_c).ok();

    match stop_recv_option {
        #[cfg(unix)]
        CrossbeamReceiverOption::None => (),
        #[cfg(windows)]
        CrossbeamReceiverOption::Receiver(recv) => rt.block_on(wait_for_recv(recv)).unwrap(),
    }

    info!("Server exited");
}

#[cfg(windows)]
fn service_main(_arguments: Vec<OsString>) {
    let (stop_send, stop_recv) = crossbeam_channel::bounded(1);
    let event_handler = move |control_event| -> service_control_handler::ServiceControlHandlerResult {
        match control_event {
            service::ServiceControl::Interrogate => service_control_handler::ServiceControlHandlerResult::NoError,
            service::ServiceControl::Stop => {
                stop_send.send(()).unwrap();
                service_control_handler::ServiceControlHandlerResult::NoError
            }
            _ => service_control_handler::ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler).unwrap();
    status_handle
        .set_service_status(service::ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: service::ServiceState::Running,
            controls_accepted: service::ServiceControlAccept::STOP,
            exit_code: service::ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
        })
        .unwrap();

    run_loggest(CrossbeamReceiverOption::Receiver(stop_recv));

    status_handle
        .set_service_status(service::ServiceStatus {
            service_type: SERVICE_TYPE,
            current_state: service::ServiceState::Stopped,
            controls_accepted: service::ServiceControlAccept::empty(),
            exit_code: service::ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
        })
        .unwrap();
}

#[cfg(unix)]
fn main() {
    run_loggest(CrossbeamReceiverOption::None);
}

#[cfg(windows)]
fn main() -> Result<(), windows_service::Error> {
    windows_service::service_dispatcher::start(SERVICE_NAME, service_entry_point)?;
    Ok(())
}
