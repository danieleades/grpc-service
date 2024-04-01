use std::ffi::OsString;

use service_handler::{run_service, Server, ServiceEvent};
use windows_service::{define_windows_service, service_dispatcher};
mod server;
mod service;

use crate::service::GrpcService;

const LOG_DIRECTORY: &str = r"C:\Users\daniel.eades\Desktop";

define_windows_service!(ffi_service_main, service_main);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    configure_logging();
    service_dispatcher::start(GrpcService::NAME, ffi_service_main)?;
    Ok(())
}

fn configure_logging() {
    let file_appender = tracing_appender::rolling::daily(LOG_DIRECTORY, "grpc-service.log");
    let subscriber = tracing_subscriber::fmt()
        .with_writer(file_appender)
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .finish();

    // Set the subscriber globally for the application
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set the global default subscriber");
}

fn service_main(_arguments: Vec<OsString>) {
    let server = GrpcService::default();
    if let Err(e) = run_service(server) {
        tracing::error!("error: {}", e);
    }
}
