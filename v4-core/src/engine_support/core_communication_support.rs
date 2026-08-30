use async_scoped::TokioScope;
use crossbeam_channel::{Receiver, Sender};
use thiserror::Error;

use crate::{
    ecs::{
        component::ComponentId,
        scene::{WorkloadOutput, WorkloadPacket},
    },
    engine_management::engine_action::EngineAction,
};

#[derive(Error, Debug)]
pub enum CommunicationError {
    #[error("Failed to create tokio runtime for workloads: {0}")]
    RuntimeInitError(#[from] std::io::Error),
    #[error("Failed to send workload output for component {id}")]
    WorkloadOutputSendError {
        id: ComponentId,
        err: crossbeam_channel::SendError<(u64, Box<dyn std::any::Any + Send + Sync>)>,
    },
}

#[derive(Debug)]
pub struct CoreCommunication {
    workload_sender: Sender<WorkloadPacket>,
    workload_output_receiver: Receiver<(ComponentId, WorkloadOutput)>,
    _workload_thread_handle: std::thread::JoinHandle<Result<(), CommunicationError>>,
    engine_action_sender: Sender<Box<dyn EngineAction>>,
    engine_action_receiver: Receiver<Box<dyn EngineAction>>,
}

impl CoreCommunication {
    pub fn new() -> Result<Self, CommunicationError> {
        let (workload_sender, workload_receiver): (
            Sender<WorkloadPacket>,
            Receiver<WorkloadPacket>,
        ) = crossbeam_channel::unbounded();

        let (workload_output_sender, workload_output_receiver): (
            Sender<(ComponentId, WorkloadOutput)>,
            Receiver<_>,
        ) = crossbeam_channel::unbounded();

        let _workload_thread_handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new()?;

            runtime.block_on(async move {
                TokioScope::scope_and_block(|async_scope| {
                    if let Ok(workload_packet) = workload_receiver.try_recv() {
                        let sender = workload_output_sender.clone();
                        async_scope.spawn(async move {
                            let workload_result = workload_packet.workload.await;
                            sender
                                .send((workload_packet.component_id, workload_result))
                                .map_err(|err| CommunicationError::WorkloadOutputSendError {
                                    id: workload_packet.component_id,
                                    err,
                                })?;

                            Ok::<(), CommunicationError>(())
                        });
                    }
                });
            });

            Ok(())
        });

        let (engine_action_sender, engine_action_receiver): (
            Sender<Box<dyn EngineAction>>,
            Receiver<Box<dyn EngineAction>>,
        ) = crossbeam_channel::unbounded();

        Ok(Self {
            workload_sender,
            workload_output_receiver,
            _workload_thread_handle,
            engine_action_sender,
            engine_action_receiver,
        })
    }

    pub fn workload_sender(&self) -> Sender<WorkloadPacket> {
        self.workload_sender.clone()
    }

    pub fn workload_output_receiver(&self) -> Receiver<(ComponentId, WorkloadOutput)> {
        self.workload_output_receiver.clone()
    }

    pub fn engine_action_sender(&self) -> Sender<Box<dyn EngineAction>> {
        self.engine_action_sender.clone()
    }

    pub fn engine_action_receiver(&self) -> Receiver<Box<dyn EngineAction>> {
        self.engine_action_receiver.clone()
    }
}
