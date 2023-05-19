fn main() {
    println!("Hello, world!");
}

pub mod task {
    use reqwest;
    use std::time::Duration;
    use async_std;
    use crate::task::TaskType::{Bar, Baz, Foo};


    pub trait TaskStateConstants {
        const NOTSTARTED: &'static str;
        const STARTED: &'static str;
        const COMPLETE: &'static str;
    }

    impl TaskStateConstants for TaskState {
        const NOTSTARTED: &'static str = "NOTSTARTED";
        const STARTED: &'static str = "STARTED";
        const COMPLETE: &'static str = "COMPLETE";
    }

    impl From<&TaskState> for &str {
        fn from(task_state: &TaskState) -> &'static str {
            match task_state {
                TaskState::Started => TaskState::STARTED,
                TaskState::NotStarted => TaskState::NOTSTARTED,
                TaskState::Complete => TaskState::COMPLETE
            }
        }
    }

    impl From<&str> for TaskState {
        fn from(value: &str) -> Self {
            match value {
                TaskState::NOTSTARTED => Self::NotStarted,
                TaskState::STARTED => Self::Started,
                TaskState::COMPLETE => Self::Complete,
                _ => {
                    panic!("invalid string, cannot convert to TaskState")
                }
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TaskState {
        NotStarted,
        Started,
        Complete,
    }

    impl TaskState {
        //state machine at all points only have one possible path
        fn transition_state(&self) -> TaskState {
            match self {
                Self::NotStarted => Self::Started,
                Self::Started => Self::Complete,
                Self::Complete => Self::Complete
            }
        }
    }

    pub type TaskId = u64;
    pub type TaskTimeStamp = u64;

    pub trait TaskConstants {
        const FOO: &'static str;
        const BAR: &'static str;
        const BAZ: &'static str;
    }

    impl TaskConstants for TaskType {
        const FOO: &'static str = "FOO";
        const BAR: &'static str = "BAR";
        const BAZ: &'static str = "BAZ";
    }


    impl From<&TaskType> for &'static str {
        fn from(task_type: &TaskType) -> &'static str {
            match task_type {
                Foo(_, _, _) => TaskType::FOO,
                Bar(_, _, _) => TaskType::BAR,
                Baz(_, _, _) => TaskType::BAZ
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TaskType {
        Foo(Option<TaskId>, TaskTimeStamp, TaskState),
        Bar(Option<TaskId>, TaskTimeStamp, TaskState),
        Baz(Option<TaskId>, TaskTimeStamp, TaskState),
    }

    impl TaskType {
        pub fn transition_task(&self) -> TaskType {
            match self {
                Self::Foo(id, time, state) =>
                    Self::Foo(*id, *time, state.transition_state()),
                Self::Bar(id, time, state) =>
                    Self::Bar(*id, *time, state.transition_state()),
                Self::Baz(id, time, state) =>
                    Self::Baz(*id, *time, state.transition_state())
            }
        }

        pub async fn handle_task(&self) -> TaskType {
            match self {
                Self::Foo(Some(id), _, TaskState::Started) => {
                    async_std::task::sleep(Duration::from_secs(3)).await;
                    std::println!("Foo {id}");
                    self.transition_task()
                }
                Self::Foo(None, _, _) => {
                    panic!("Cannot execute handler on Foo task with no id.")
                }
                Self::Baz(_, _, TaskState::Started) => {
                    let number = rand::random::<u64>() % 344;
                    std::println!("Baz {number}");
                    self.transition_task()
                }
                Self::Bar(_, _, TaskState::Started) => {
                    let url = "https://www.whattimeisitrightnow.com";
                    if let Ok(response) = reqwest::(url).await {
                        let status_code = response.status();
                        let text = response.text().await.unwrap();
                        println!("{text}");
                        let status_code_str = status_code.as_str();
                        std::println!("{status_code_str}");
                        self.transition_task()
                    } else {
                        panic!("Undefined behavior: {url} not found")
                    }
                }
                _ => {
                    panic!("handle tasks only works on tasks that have been transitioned to <Started>")
                }
            }
        }
    }
}

mod state {
    use crate::task::{TaskConstants, TaskState, TaskType};
    use async_trait::async_trait;
    use crate::task::TaskType::{Bar, Baz, Foo};
    use deadpool_sqlite::{Config, Pool, Runtime};
    use std::time::{SystemTime, UNIX_EPOCH};
    use rusqlite::OptionalExtension;
    use tokio::task::JoinHandle;
    use crate::task;

    #[async_trait]
    pub trait State: Sized {
        async fn initialize_state() -> Self;
        async fn get_tasks_that_need_to_be_executed(&self) -> Vec<TaskType>;
        async fn add_task(&self, task: TaskType) -> u64;
        async fn get_all_tasks(&self, task_state: &TaskState) -> Vec<TaskType>;
        async fn consume_tasks(&self, tasks_not_started: Vec<TaskType>) -> Vec<JoinHandle<TaskType>>;
        async fn delete_task_from_id(&self, id: u64);
        async fn get_task_from_id(&self, id: u64) -> Option<TaskType>;
        async fn clear_all(&self);
    }


    pub struct SqliteState {
        pool: Pool,
    }

    #[async_trait]
    impl State for SqliteState {
        async fn initialize_state() -> Self {
            let cfg = Config {
                path: "db.sqlite3".into(),
                pool: None,
            };
            let pool = cfg.create_pool(Runtime::Tokio1).unwrap();
            let connection = pool.get().await.unwrap();
            connection.interact(|connection| {
                let transaction = connection.transaction().unwrap();
                transaction.execute("CREATE TABLE IF NOT EXISTS tasks (
                        id INTEGER PRIMARY KEY,
                        task_state CHAR NOT NULL,
                        task_time INT NOT NULL,
                        task_type CHAR NOT NULL
                    );", ()).unwrap();
                transaction.execute("CREATE INDEX IF NOT EXISTS taskstatesort ON tasks (task_state);", ()).unwrap();
                transaction.execute("CREATE INDEX IF NOT EXISTS timesort ON tasks (task_time);", ()).unwrap();
                transaction.commit()
            }).await.unwrap().unwrap();
            SqliteState {
                pool
            }
        }

        async fn consume_tasks(&self, tasks_not_started: Vec<TaskType>) -> Vec<JoinHandle<TaskType>> {
            let mut join_handles = Vec::with_capacity(tasks_not_started.len());
            let mut_ref_join_handler = &mut join_handles;
            for task in tasks_not_started.into_iter() {
                let pool = self.pool.clone();
                let handle = tokio::spawn(async move {
                    fn get_parameters_from_task(task: TaskType) -> (task::TaskId, &'static str) {
                        match task {
                            Foo(Some(id), _, state) => (id, (&state).into()),
                            Bar(Some(id), _, state) => (id, (&state).into()),
                            Baz(Some(id), _, state) => (id, (&state).into()),
                            _ => {
                                panic!("Cannot update tasks with no id.")
                            }
                        }
                    }
                    let connection = pool.get().await.unwrap();
                    connection.interact(move |connection| {
                        let parameters_not_started: (u64, &str) = get_parameters_from_task(task);
                        connection.execute(
                            "UPDATE tasks SET task_state = ?2 WHERE id = ?1",
                            parameters_not_started).unwrap();
                    }).await.unwrap();
                    let pending_task = task.transition_task();
                    let finished_task = pending_task.handle_task().await;
                    connection.interact(move |connection| {
                        let parameters_finished: (u64, &str) = get_parameters_from_task(finished_task);
                        connection.execute(
                            "UPDATE tasks SET task_state = ?2 WHERE id = ?1",
                            parameters_finished).unwrap();
                    }).await.unwrap();
                    finished_task
                });
                mut_ref_join_handler.push(handle);
            }
            join_handles
        }

        async fn get_tasks_that_need_to_be_executed(&self) -> Vec<TaskType> {
            let connection = self.pool.get().await.unwrap();
            connection.interact(move |connection| {
                let mut statement = connection.prepare("SELECT id, task_time, task_type, task_state FROM tasks WHERE tasks.task_state = 'NOTSTARTED' AND task_time <= ?1;").unwrap();
                let mut result = vec![]; //allocation here but easier to debug. It's more efficient to return an iterator

                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

                statement.query_map((now, ), |row| {
                    let (id, time, task_type, state): (i64, i64, String, String) =
                        (row.get(0).unwrap(), row.get(1).unwrap(), row.get(2).unwrap(), row.get(3).unwrap());
                    match task_type.as_str() {
                        TaskType::FOO => Ok(Foo(Some(id as u64), time as u64, state.as_str().into())),
                        TaskType::BAR => Ok(Bar(Some(id as u64), time as u64, state.as_str().into())),
                        TaskType::BAZ => Ok(Baz(Some(id as u64), time as u64, state.as_str().into())),
                        _ => {
                            panic!("invalid string, cannot convert to task.")
                        }
                    }
                }).unwrap().for_each(|task| {
                    result.push(task.unwrap());
                });
                result
            }).await.unwrap()
        }

        async fn add_task(&self, task: TaskType) -> u64 {
            let (task_type, optional_id, time, state): (&str, Option<u64>, u64, &str) = match task {
                TaskType::Foo(id, time, state) => (TaskType::FOO, id, time, (&state).into()),
                TaskType::Bar(id, time, state) => (TaskType::BAR, id, time, (&state).into()),
                TaskType::Baz(id, time, state) => (TaskType::BAZ, id, time, (&state).into())
            };
            let connection = self.pool.get().await.unwrap();
            connection.interact(move |connection| {
                let mut rows_updated: usize;
                let id = match optional_id {
                    Some(id) => {
                        rows_updated = connection.execute("INSERT INTO tasks (id, task_time, task_type, task_state) VALUES (?1, ?2, ?3, ?4)", (id, time, task_type, state)).unwrap();
                        id
                    }
                    None => {
                        let transaction = connection.transaction().unwrap();
                        rows_updated = transaction.execute("INSERT INTO tasks (task_time, task_type, task_state) VALUES (?1, ?2, ?3)", (time, task_type, state)).unwrap();
                        let id: u64 = transaction.last_insert_rowid() as u64;
                        transaction.commit().unwrap();
                        id
                    }
                };
                if rows_updated != 1 {
                    panic!("insert says less or more than 1 row was updated. This is a error with the code, only one row should be inserted")
                }
                id
            }).await.unwrap()
        }

        async fn get_all_tasks(&self, task_state: &TaskState) -> Vec<TaskType> {
            let task_state_string: &str = task_state.into();
            let connection = self.pool.get().await.unwrap();
            connection.interact(move |connection| {
                let mut statement = connection.prepare("SELECT id, task_time, task_type, task_state FROM tasks WHERE tasks.task_state = ?1;").unwrap();
                let mut result = vec![];
                statement.query_map((task_state_string, ), |row| {
                    let (id, time, task_type, state): (i64, i64, String, String) =
                        (row.get(0).unwrap(), row.get(1).unwrap(), row.get(2).unwrap(), row.get(3).unwrap());
                    match task_type.as_str() {
                        TaskType::FOO => Ok(Foo(Some(id as u64), time as u64, state.as_str().into())),
                        TaskType::BAR => Ok(Bar(Some(id as u64), time as u64, state.as_str().into())),
                        TaskType::BAZ => Ok(Baz(Some(id as u64), time as u64, state.as_str().into())),
                        _ => {
                            panic!("invalid string, cannot convert to task.")
                        }
                    }
                }).unwrap().for_each(|task| {
                    result.push(task.unwrap());
                });
                result
            }).await.unwrap()
        }

        async fn clear_all(&self) {
            let connection = self.pool.get().await.unwrap();
            connection.interact(|connection| {
                connection.execute("DELETE FROM tasks;", ()).unwrap()
            }).await.unwrap();
        }

        async fn delete_task_from_id(&self, id: u64) {
            let connection = self.pool.get().await.unwrap();
            connection.interact(move |connection| {
                connection.execute("DELETE FROM tasks WHERE id = ?1;", (id, )).unwrap()
            }).await.unwrap();
        }

        async fn get_task_from_id(&self, id: u64) -> Option<TaskType> {
            let connection = self.pool.get().await.unwrap();
            connection.interact(move |connection| {
                connection.query_row("SELECT id, task_time, task_type, task_state FROM tasks WHERE tasks.id = ?1 LIMIT 1", (id, ), |row| {
                    let (id, time, task_type, state): (u64, u64, String, String) = (row.get(0).unwrap(), row.get(1).unwrap(), row.get(2).unwrap(), row.get(3).unwrap());
                    Ok(match task_type.as_str() {
                        TaskType::FOO => Foo(Some(id), time, state.as_str().into()),
                        TaskType::BAR => Bar(Some(id), time, state.as_str().into()),
                        TaskType::BAZ => Baz(Some(id), time, state.as_str().into()),
                        _ => {
                            panic!("invalid string used for task_type.")
                        }
                    })
                }).optional().unwrap()
            }).await.unwrap()
        }
    }
}

mod executor {
    use crate::state::State;
    use async_trait::async_trait;
    use crate::task::TaskType;

    #[async_trait]
    pub trait TaskRunner<T: State>: Sized {
        fn initialize_runner(state: &T) -> Self;
        async fn execute_tasks();
        async fn add_task(task: TaskType);
        async fn run(&self) -> Result<(), &'static str>;
    }
}

mod app {
    use crate::state::State;
    use crate::executor::TaskRunner;

    fn run_app<S: State, R: TaskRunner<S>>() {
        // let state = S::initialize_state().unwrap();
        // let runner = R::initialize_runner(state).unwrap();
        // runner.run();
    }
}

#[cfg(test)]
mod tests {
    use crate::state;
    use crate::state::State;
    use crate::task::TaskState::{Complete, NotStarted, Started};
    use crate::task::TaskType::{Bar, Baz, Foo};
    use std::time::{SystemTime, UNIX_EPOCH};
    use async_std::prelude::FutureExt;

    #[tokio::test]
    async fn test_state() {
        let state = state::SqliteState::initialize_state().await;
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
        let handles = state.consume_tasks(v4).await;
        for handle in handles {
            handle.await.unwrap();
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
        let near_future_time = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 2;
        let test_foo = Foo(None, 0, NotStarted);
        let test_bar = Bar(None, 0, NotStarted);
        state.add_task(test_foo).await;
        state.add_task(test_bar).await;
        let tasks_to_consume = state.get_all_tasks(&NotStarted).await;
        assert_eq!(tasks_to_consume.len(), 2);
        let handlers2 = state.consume_tasks(tasks_to_consume).await;
        for h in handlers2 {
            h.await.unwrap();
        }
        assert_eq!(state.get_all_tasks(&NotStarted).await.len(), 0);
        assert_eq!(state.get_all_tasks(&Complete).await.len(), 2);

    }
}