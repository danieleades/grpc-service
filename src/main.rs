use std::{ffi::OsString, time::Duration};

use seebyte_service_handler::{SeebyteService, ServiceEvent};
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
};
mod server;
mod service;

use tokio::sync::{mpsc, oneshot};

use crate::service::GrpcService;

pub const SERVICE_NAME: &str = "grpc-service-rs";
const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;
const LOG_DIRECTORY: &str = r"C:\Users\daniel.eades\Desktop";

define_windows_service!(ffi_service_main, service_main);


fn service_main(arguments: Vec<OsString>) {
    if let Err(e) = run_service(&arguments) {
        tracing::error!("error: {}", e);
    }
}

#[tracing::instrument(err)]
fn run_service(arguments: &[OsString]) -> Result<(), Error> {
    let rt = tokio::runtime::Runtime::new()?;

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let event_rx = futures_util::stream::poll_fn(|cx| event_rx.poll_recv(cx));

    let event_handler = move |control: ServiceControl| {
        tracing::debug!("received a control command: {:?}", control);

        let (completion_tx, completion_rx) = oneshot::channel();

        match event_tx.send(ServiceEvent::new(control, completion_tx)) {
            Ok(()) => completion_rx
                .blocking_recv()
                .inspect_err(|e| {
                    tracing::error!("Couldn't receive a completion reply: {}", e);
                })
                .unwrap_or(ServiceControlHandlerResult::Other(127)),
            Err(e) => {
                tracing::error!("Couldn't send the event: {}", e);
                ServiceControlHandlerResult::Other(128)
            }
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    tracing::info!("setting service status to 'running'...");
    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;
    tracing::info!("service status is 'running'");

    let service = GrpcService::default();
    rt.block_on(service.run(event_rx));

    tracing::debug!("gRPC server has shutdown");

    // Tell the system that service has stopped.
    status_handle.set_service_status(ServiceStatus {
        service_type: SERVICE_TYPE,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let file_appender = tracing_appender::rolling::daily(LOG_DIRECTORY, "grpc-service.log");
    let subscriber = tracing_subscriber::fmt()
        .with_writer(file_appender)
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .finish();

    // Set the subscriber globally for the application
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set the global default subscriber");

    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("gRPC transport error: {0}")]
    Transport(#[from] tonic::transport::Error),

    #[error("windows service error: {0}")]
    WindowsService(#[from] windows_service::Error),

    #[error("tokio runtime error: {0}")]
    Runtime(#[from] std::io::Error),
}