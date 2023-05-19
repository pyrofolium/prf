fn main() {
    println!("Hello, world!");
}

pub mod task {
    use reqwest;
    use std::time::Duration;
    use async_std;
    use fred::error::RedisError;
    use fred::prelude::FromRedis;
    use fred::types::{RedisKey, RedisValue};
    use fred::types::RedisValueKind::String;
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
                TaskState::Started => TaskState::NOTSTARTED,
                TaskState::NotStarted => TaskState::NOTSTARTED,
                TaskState::Complete => TaskState::COMPLETE
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
                TaskType::Foo(_, _, _) => TaskType::FOO,
                TaskType::Bar(_, _, _) => TaskType::BAR,
                TaskType::Baz(_, _, _) => TaskType::BAZ
            }
        }
    }

    impl From<&RedisValue> for TaskState {
        fn from(value: &RedisValue) -> Self {
            match value {
                RedisValue::String(s) => {
                    match s.to_string().as_str() {
                        TaskState::STARTED => TaskState::Started,
                        TaskState::NOTSTARTED => TaskState::NotStarted,
                        TaskState::COMPLETE => TaskState::Complete,
                        _ => {
                            panic!("invalid string used to represent task state in redis")
                        }
                    }
                }
                _ => {
                    panic!("invalid RedisValue, cannot convert to TaskState");
                }
            }
        }
    }
    impl FromRedis for TaskType {
        fn from_value(value: RedisValue) -> Result<Self, RedisError> {
            value.convert()
        }
    }

    impl From<RedisValue> for TaskType {
        fn from(value: RedisValue) -> Self {
            match value {
                RedisValue::Map(map) => {
                    let task_id = map.get(&RedisKey::from("task_id")).unwrap().as_u64().unwrap();
                    let task_type = map.get(&RedisKey::from("task_type")).unwrap().as_str().unwrap().into_owned();
                    let task_time = map.get(&RedisKey::from("task_time")).unwrap().as_u64().unwrap();
                    let task_state = TaskState::from(map.get(&RedisKey::from("task_state")).unwrap());
                    match task_type.as_str() {
                        TaskType::FOO => Foo(task_id, task_time, task_state),
                        TaskType::BAR => Bar(task_id, task_time, task_state),
                        TaskType::BAZ => Baz(task_id, task_time, task_state),
                        _ => {
                            panic!("invalid task type used to represent Task type in redis")
                        }
                    }
                },
                _ => {
                    panic!("Error only redis map values can be converted to Tasks")
                }
            }
        }
    }

    #[derive(Debug)]
    pub enum TaskType {
        Foo(TaskId, TaskTimeStamp, TaskState),
        Bar(TaskId, TaskTimeStamp, TaskState),
        Baz(TaskId, TaskTimeStamp, TaskState),
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
                Self::Foo(id, _, _) => {
                    async_std::task::sleep(Duration::from_secs(3)).await;
                    async_std::println!("Foo {id}").await;
                    self.transition_task()
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
    use std::collections::HashMap;
    use crate::task::{TaskConstants, TaskState, TaskStateConstants, TaskType};
    use async_trait::async_trait;
    use fred::error::RedisError;
    // use rusqlite::{Connection, Error, OptionalExtension, Result, Row};
    use crate::task::TaskState::{Complete, NotStarted, Started};
    use crate::task::TaskType::{Bar, Baz, Foo};
    use fred::prelude::{ClientLike, ListInterface, RedisClient, TransactionInterface, RedisValue, HashesInterface, SetsInterface};
    use fred::types::{RedisKey, RedisMap};

    #[async_trait]
    pub trait State: Sized {
        async fn initialize_state() -> Self;
        async fn get_next_task(&self) -> Option<TaskType>;
        async fn add_task(&self, task: TaskType);
        async fn get_all_tasks(&self, task_state: &TaskState) -> Vec<TaskType>;
        async fn transition_task(&self, id: &u64);
    }

    pub struct RedisState {
        pub client: RedisClient,
    }

    pub trait RedisConstants {
        const TASK_ID_LIST_KEY_PREFIX: &'static str;
        const TASK_INFO_HASHSET_KEY_PREFIX: &'static str;
        fn create_hash_set_key(id: u64, state: &TaskState) -> String {
            Self::TASK_INFO_HASHSET_KEY_PREFIX.to_owned() + "_" + &id.to_string() + "_" + state.into()
        }

        fn create_task_id_list_key(state: &TaskState) -> &'static str {
            match state {
                Complete => "task_ids_COMPLETE",
                Started => "task_ids_STARTED",
                NotStarted => "task_ids_NOTSTARTED"
            }
        }
    }

    impl RedisConstants for RedisState {
        const TASK_ID_LIST_KEY_PREFIX: &'static str = "task_ids";
        const TASK_INFO_HASHSET_KEY_PREFIX: &'static str = "task_info";
    }

    #[async_trait]
    impl State for RedisState {
        async fn initialize_state() -> Self {
            let client = RedisClient::default();
            client.connect();
            client.wait_for_connect().await.unwrap();
            RedisState {
                client
            }
        }

        async fn get_next_task(&self) -> Option<TaskType> {
            todo!()
        }

        async fn add_task(&self, task: TaskType) {
            let (task_type, id, time, state): (&str, u64, u64, TaskState) = match task {
                Foo(id, time, state) => ((&task).into(), id, time, state),
                Bar(id, time, state) => ((&task).into(), id, time, state),
                Baz(id, time, state) => ((&task).into(), id, time, state)
            };
            let mut redis_map = RedisMap::new();
            redis_map.insert(RedisKey::from("task_id"), RedisValue::from(id as i64));
            redis_map.insert(RedisKey::from("task_type"), RedisValue::from(task_type));
            redis_map.insert(RedisKey::from("task_time"), RedisValue::from(time as i64));
            redis_map.insert(RedisKey::from("task_state"), RedisValue::from(<&TaskState as Into<&str>>::into(&state)));
            let transaction = self.client.multi();
            let _: u64 = transaction.sadd(RedisState::create_task_id_list_key(&state), id as i64).await.unwrap();
            let elements_added: u64 = transaction.hset(Self::create_hash_set_key(id, &state), redis_map).await.unwrap();
            if elements_added != 3 {
                panic!("only 3 properties are allowed per task.")
            }
            transaction.exec::<(u64, )>(true).await.unwrap();
        }

        async fn get_all_tasks(&self, task_state: &TaskState) -> Vec<TaskType> {
            let transaction = self.client.multi();
            let ids: RedisValue = transaction.lrange(RedisState::create_task_id_list_key(task_state), 0, -1).await.unwrap();
            let mut tasks: Vec<TaskType> = vec![];
            // for id in ids.iter() {
            //     tasks.push(transaction.hgetall(RedisState::create_hash_set_key(*id, task_state)).await.unwrap());
            // }
            tasks
        }

        async fn transition_task(&self, id: &u64) {
            todo!()
        }
    }


    // struct SqliteState {
    //     connection: Connection,
    //     next_state: Option<TaskState>,
    // }
    //
    // impl SqliteState {
    //     pub fn new(path_str: &str) -> Self {
    //         SqliteState {
    //             connection: Connection::open(Path::new(path_str)).unwrap(),
    //             next_state: None,
    //         }
    //     }
    //
    //     pub fn row_to_task(row: &Row) -> Result<TaskType, &'static str> {
    //         fn string_to_task_state(s: &str) -> TaskState {
    //             match s {
    //                 "NOTSTARTED" => NotStarted,
    //                 "STARTED" => Started,
    //                 "COMPLETE" => Complete,
    //                 _ => Err("Invalid Task state string in tasks table")
    //             }
    //         }
    //
    //         match row.get(2) {
    //             Ok("FOO") => Ok(Foo(row.get(0)?, row.get(1)?, string_to_task_state(row.get(3)?)?)),
    //             Ok("BAR") => Ok(Bar(row.get(0)?, row.get(1)?, string_to_task_state(row.get(3)?)?)),
    //             Ok("BAZ") => Ok(Baz(row.get(0)?, row.get(1)?, string_to_task_state(row.get(3)?)?)),
    //             Ok(_) => Err("Invalid string for task type in table"),
    //             Err(e) => Err("Getting column for table had an error")
    //         }
    //     }
    // }
    //
    //
    // impl State for SqliteState {
    //     fn initialize_state(&self) -> Result<(), &'static str> {
    //         let connection = &self.connection;
    //         if let Ok(()) = connection.execute_batch("
    //             BEGIN;
    //                 CREATE TABLE IF NOT EXISTS tasks (
    //                     id INT,
    //                     task_time INT NOT NULL,
    //                     task_type CHAR NOT NULL,
    //                     task_state CHAR NOT NULL,
    //                     PRIMARY KEY (id),
    //                 );
    //                 CREATE INDEX IF NOT EXISTS timestamp_sort ON tasks.task_time;
    //             COMMIT;
    //         ") {
    //             Ok(())
    //         } else {
    //             Err("Problem initializing state")
    //         }
    //     }
    //
    //
    //     fn get_next_task(&self) -> Result<Option<TaskType>, &'static str> {
    //         let mut statement = self.connection.prepare("SELECT id, task_time, task_type, task_state FROM tasks ORDER BY task_time ASC LIMIT 1;")?;
    //         match statement.query_row([], row_to_task).optional() {
    //             Ok(optional_task) => Ok(optional_task),
    //             Err(e) => Err("Problem querying table")
    //         }
    //     }
    //
    //     fn add_task(&self, task: TaskType) -> Result<(), &'static str> {
    //
    //     }
    //
    //     fn get_all_tasks(&self) -> Result<Vec<TaskType>, &'static str> {
    //         let mut statement = self.connection.prepare("SELECT id, task_time, task_type, task_state FROM tasks;")?;
    //         if let Ok(iterator) = statement.query_map([], |row| {
    //             match SqliteState::row_to_task(row) {
    //                 Ok(v) => Ok(v),
    //                 Err(_) => Err(rusqlite::Error::ExecuteReturnedResults)
    //             }
    //         }) {
    //             Ok(iterator.collect::<Vec<TaskType>>())
    //         } else {
    //             Err("Failed to convert query results to a vector")
    //         }
    //     }
}

mod executor {
    use crate::state::State;
    use async_trait::async_trait;

    #[async_trait]
    pub trait TaskRunner<T: State>: Sized {
        fn initialize_runner(state: T) -> Result<Self, &'static str>;
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
    use fred::prelude::ServerInterface;
    use crate::state;
    use crate::state::State;
    use crate::task::TaskState;
    use crate::task::TaskType::Foo;

    #[tokio::test]
    async fn test_state() {
        let state = state::RedisState::initialize_state().await;
        let _: String = state.client.flushall(true).await.unwrap();
        let task = Foo(1, 0, TaskState::NotStarted);
        state.add_task(task);
        let tasks = state.get_all_tasks(&TaskState::NotStarted).await;
        println!("tasks: {tasks:?}");

    }
}