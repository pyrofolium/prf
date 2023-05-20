use crate::task_state::{State};
use std::thread;


pub fn run<T: Sync + State + Send + 'static>(state: T) -> thread::JoinHandle<()> {
    let handler = thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            loop {
                let state_clone = state.clone();
                tokio::spawn(async move {
                    state_clone.consume_tasks().await
                });
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
            }
        });
    });
    handler
}