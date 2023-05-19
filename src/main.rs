fn main() {
    println!("Hello, world!");
}

pub mod task {
    use reqwest;
    use std::time::Duration;
    use async_std;
    use rand::{Rng};
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

    #[derive(Clone, Copy, Debug)]
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

    type TaskId = u64;
    type TaskTimeStamp = u64;

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

    #[derive(Debug, Clone)]
    pub enum TaskType {
        Foo(Option<TaskId>, TaskTimeStamp, TaskState),
        Bar(Option<TaskId>, TaskTimeStamp, TaskState),
        Baz(Option<TaskId>, TaskTimeStamp, TaskState),
    }

    impl TaskType {
        fn transition_task(&self) -> TaskType {
            match self {
                Self::Foo(id, time, state) =>
                    Self::Foo(*id, *time, state.transition_state()),
                Self::Bar(id, time, state) =>
                    Self::Bar(*id, *time, state.transition_state()),
                Self::Baz(id, time, state) =>
                    Self::Baz(*id, *time, state.transition_state())
            }
        }

        async fn handle_task(&self, rando: &mut rand::rngs::ThreadRng) -> TaskType {
            match self {
                Self::Foo(Some(id), _, _) => {
                    async_std::task::sleep(Duration::from_secs(3)).await;
                    async_std::println!("Foo {id}").await;
                    self.transition_task()
                }
                Self::Foo(None, _, _) => {
                    panic!("Cannot execute handler on Foo task with no id.")
                }
                Self::Baz(_, _, _) => {
                    let number = rando.gen_range(0..344);
                    async_std::println!("Baz {number}").await;
                    self.transition_task()
                }
                Self::Bar(_, _, _) => {
                    let url = "https://www.whattimeisitrightnow.com/";
                    if let Ok(response) = reqwest::get(url).await {
                        let status_code = response.status();
                        let status_code_str = status_code.as_str();
                        async_std::println!("{status_code_str}").await;
                        self.transition_task()
                    } else {
                        panic!("Undefined behavior: {url} not found")
                    }
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

    #[async_trait]
    pub trait State: Sized {
        async fn initialize_state() -> Self;
        async fn get_tasks_that_need_to_be_executed(&self) -> Vec<TaskType>;
        async fn add_task(&self, task: TaskType);
        async fn get_all_tasks(&self, task_state: &TaskState) -> Vec<TaskType>;
        async fn transition_task(&self, id: &u64);
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

        async fn get_tasks_that_need_to_be_executed(&self) -> Vec<TaskType> {
            let connection = self.pool.get().await.unwrap();
            connection.interact(move |connection| {
                let mut statement = connection.prepare("SELECT id, task_time, task_type, task_state FROM tasks WHERE tasks.task_state = 'NOTSTARTED' AND task_time <= ?1;").unwrap();
                let mut result = vec![];

                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();

                statement.query_map((now,), |row|{
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
                }).unwrap().for_each(|task|{
                    result.push(task.unwrap());
                });
                result
            }).await.unwrap()
        }

        async fn add_task(&self, task: TaskType) {
            let (task_type, optional_id, time, state): (&str, Option<u64>, u64, &str) = match task {
                TaskType::Foo(id, time, state) => (TaskType::FOO, id, time, (&state).into()),
                TaskType::Bar(id, time, state) => (TaskType::BAR, id, time, (&state).into()),
                TaskType::Baz(id, time, state) => (TaskType::BAZ, id, time, (&state).into())
            };
            let connection = self.pool.get().await.unwrap();
            let rows_updated = connection.interact(move |connection| {
                if let Some(id) = optional_id {
                    connection.execute("INSERT INTO tasks (id, task_time, task_type, task_state) VALUES (?1, ?2, ?3, ?4)", (id, time, task_type, state)).unwrap()
                } else {
                    connection.execute("INSERT INTO tasks (task_time, task_type, task_state) VALUES (?1, ?2, ?3)", (time, task_type, state)).unwrap()
                }
            }).await.unwrap();
            if rows_updated != 1 {
                panic!("insert says less or more than 1 row was updated.")
            }
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

        async fn transition_task(&self, id: &u64) {
            todo!()
        }

        async fn clear_all(&self) {
            let connection = self.pool.get().await.unwrap();
            connection.interact(|connection| {
                connection.execute("DELETE FROM tasks;", ()).unwrap()
            }).await.unwrap();
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
    use crate::task::TaskType::{Baz, Foo};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[tokio::test]
    async fn test_state() {
        let state = state::SqliteState::initialize_state().await;
        state.clear_all().await;
        let task = Foo(None, SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 1000000, NotStarted);
        state.add_task(task.clone()).await;
        state.add_task(task.clone()).await;
        let v = state.get_all_tasks(&NotStarted).await;
        assert_eq!(v.len(), 2);
        let v2 = state.get_tasks_that_need_to_be_executed().await;
        assert_eq!(v2.len(), 0);
        let future_task = Baz(None, SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 2, NotStarted);
        state.add_task(future_task).await;
        let v3 = state.get_tasks_that_need_to_be_executed().await;
        assert_eq!(v3.len(), 0);
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
        let v4 = state.get_tasks_that_need_to_be_executed().await;
        assert_eq!(v4.len(), 1);
        let v5 = state.get_all_tasks(&Complete).await;
        assert_eq!(v5.len(), 0);
        let v6 = state.get_all_tasks(&Started).await;
        assert_eq!(v6.len(), 0);
        let v7 = state.get_all_tasks(&NotStarted).await;
        assert_eq!(v7.len(), 3);
    }
}