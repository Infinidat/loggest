use log::{info, LevelFilter};
use loggest::init;
use std::thread;

fn main() {
    init(LevelFilter::Info, "example").unwrap();

    info!("Main thread");

    thread::spawn(move || {
        info!("A thread");
    })
    .join()
    .unwrap();
}
