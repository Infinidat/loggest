use future::Future;
use tokio::net::unix::UnixListener;
use tokio::prelude::*;
mod codec;
mod session;

fn main() {
    let socket = UnixListener::bind("/run/user/1000/loggestd.sock").unwrap().incoming();
    let server = socket
        .for_each(|socket| {
            println!("Connected: {:?}", socket);
            tokio::spawn(session::LoggestdSession::new(socket).map_err(|_| ()));
            Ok(())
        })
        .map_err(|e| {
            println!("Error accepting: {:?}", e);
        });

    tokio::run(server);
}
