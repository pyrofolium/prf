mod task;
mod task_server;
mod task_state;
mod task_runner;

use crate::task_state::{SqliteState, State};
use tokio::runtime::Runtime;
use task_server::run_server;


fn main() {
    //create task runner
    //the sqlite libary is async and initializing requires async calls so use a tokio runtime to drive it to the end.
    let state = Runtime::new().unwrap().block_on(SqliteState::initialize_state());

    //actual std::thread handler
    let _ = task_runner::run(state.clone());

    //create http server
    Runtime::new().unwrap().block_on(async move {
        run_server(state.clone()).await;
    });
}




#[cfg(test)]
mod tests {
    use crate::task_state;
    use crate::task_state::State;
    use crate::task::TaskState::{Complete, NotStarted, Started};
    use crate::task::TaskType::{Bar, Baz, Foo};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn test_state() {
        let state = task_state::SqliteState::initialize_state().await;
        state.clear_all().await;
        let future_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 1000000;
        let task = Foo(None, future_time, NotStarted);
        let id1 = state.add_task(task.clone()).await;
        assert_eq!(id1, 1);
        let id2 = state.add_task(task.clone()).await;
        assert_eq!(id2, 2);
        let v = state.get_all_tasks(&NotStarted).await;
        assert_eq!(v.len(), 2);
        let v2 = state.get_tasks_that_need_to_be_executed().await;
        assert_eq!(v2.len(), 0);
        let near_future_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 2;
        let future_task = Baz(None, near_future_time, NotStarted);
        let id3 = state.add_task(future_task.clone()).await;
        assert_eq!(id3, 3);
        let v3 = state.get_tasks_that_need_to_be_executed().await;
        assert_eq!(v3.len(), 0);
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        let v4 = state.get_tasks_that_need_to_be_executed().await;
        assert_eq!(v4.len(), 1);
        let handles = state.consume_tasks().await;
        for handle in handles {
            let task = handle.await.unwrap();
            state.update_task(task).await;
        }
        let v5 = state.get_all_tasks(&Complete).await;
        assert_eq!(v5.len(), 1);
        let v6 = state.get_all_tasks(&Started).await;
        assert_eq!(v6.len(), 0);
        let v7 = state.get_all_tasks(&NotStarted).await;
        assert_eq!(v7.len(), 2);
        let task1 = state.get_task_from_id(1).await.unwrap();
        let task2 = state.get_task_from_id(2).await.unwrap();
        let task3 = state.get_task_from_id(3).await.unwrap();
        assert_eq!(task1, Foo(Some(1), future_time, NotStarted));
        assert_eq!(task2, Foo(Some(2), future_time, NotStarted));
        assert_eq!(task3, Baz(Some(3), near_future_time, Complete));
        for id in 1..4 {
            state.delete_task_from_id(id).await;
        }
        for id in 1..4 {
            let val = state.get_task_from_id(id).await;
            assert_eq!(val, None)
        }
        // let near_future_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 2;
        let test_foo = Foo(None, 0, NotStarted);
        let test_bar = Bar(None, 0, NotStarted);
        state.add_task(test_foo).await;
        let t_id = state.add_task(test_bar).await;
        assert_eq!(state.get_task_from_id(t_id).await.unwrap(), Bar(Some(t_id), 0, NotStarted));
        let tasks_to_consume = state.get_all_tasks(&NotStarted).await;
        assert_eq!(tasks_to_consume.len(), 2);
        let handlers2 = state.consume_tasks().await;
        for h in handlers2 {
            let task = h.await.unwrap();
            state.update_task(task).await;
        }
        assert_eq!(state.get_all_tasks(&NotStarted).await.len(), 0);
        assert_eq!(state.get_all_tasks(&Complete).await.len(), 2);
    }
}