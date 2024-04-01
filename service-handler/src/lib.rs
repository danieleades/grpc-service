use std::time::Duration;

use futures_util::{Future, Stream};
use tokio::sync::{mpsc, oneshot};
use windows_service::{
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
        ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
};

const SERVICE_TYPE: ServiceType = ServiceType::OWN_PROCESS;

pub trait Server: std::fmt::Debug {
    const NAME: &'static str;
    fn run(&self, event_rx: impl Stream<Item = ServiceEvent> + Unpin) -> impl Future<Output = ()>;
}

#[derive(Debug)]
pub struct ServiceEvent {
    service_control: ServiceControl,
    completion_tx: oneshot::Sender<ServiceControlHandlerResult>,
}

impl ServiceEvent {
    pub fn new(
        service_control: ServiceControl,
        completion_tx: oneshot::Sender<ServiceControlHandlerResult>,
    ) -> Self {
        Self {
            service_control,
            completion_tx,
        }
    }

    pub fn service_control(&self) -> ServiceControl {
        self.service_control
    }

    pub fn complete(self, result: ServiceControlHandlerResult) {
        if self.completion_tx.send(result).is_err() {
            tracing::error!("Failed to send a completion reply");
        }
    }
}

#[tracing::instrument(err)]
pub fn run_service<S: Server>(service: S) -> Result<(), Error> {
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

    let status_handle = service_control_handler::register(S::NAME, event_handler)?;

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

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("windows service error: {0}")]
    WindowsService(#[from] windows_service::Error),

    #[error("tokio runtime error: {0}")]
    Runtime(#[from] std::io::Error),
}
