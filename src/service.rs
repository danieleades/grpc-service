use std::{net::SocketAddr, str::FromStr};

use crate::{server::serve, ServiceEvent};
use futures_util::{Future, Stream, StreamExt};
use seebyte_service_handler::SeebyteService;
use tokio_util::sync::CancellationToken;
use windows_service::{service::ServiceControl, service_control_handler::ServiceControlHandlerResult};

pub async fn run(address: SocketAddr, shutdown_signal: impl Future<Output = ()> + Send) {
    // If this task returns, it's because we sent a shutdown signal. Otherwise it'll just keep restarting and looping
    serve(address, shutdown_signal)
        .await
        .expect("gRPC server error!");
}

#[derive(Debug, Default)]
pub struct GrpcService;

impl SeebyteService for GrpcService {
    async fn run(&self, mut event_rx: impl Stream<Item = ServiceEvent> + Unpin) {
        let shutdown_token = CancellationToken::new();
        let shutdown_fut = shutdown_token.clone().cancelled_owned();
    
        let address = SocketAddr::from_str("[::1]:10000").unwrap();
        let mut service_join_handle = Some(tokio::spawn(run(address, shutdown_fut)));
    
        while let Some(service_event) = event_rx.next().await {
            let service_control_result: ServiceControlHandlerResult =
                match service_event.service_control() {
                    ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
                    ServiceControl::Stop => {
                        shutdown_token.cancel();
    
                        if let Some(service_join_handle) = service_join_handle.take() {
                            if let Err(e) = service_join_handle.await {
                                tracing::error!("Couldn't join on service: {}", e);
                            }
                        } // else service is already processing a shutdown command
    
                        ServiceControlHandlerResult::NoError
                    }
                    _ => ServiceControlHandlerResult::NotImplemented,
                };
    
            service_event.complete(service_control_result);
        }
    }
}