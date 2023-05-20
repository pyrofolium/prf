use crate::task::{TaskConstants, TaskState, TaskType};
use async_trait::async_trait;
use crate::task::TaskType::{Bar, Baz, Foo};
use deadpool_sqlite::{Config, Pool, Runtime};
use std::time::{SystemTime, UNIX_EPOCH};
use rusqlite::OptionalExtension;
use tokio::task::JoinHandle;

#[async_trait]
pub trait State: Sized + Clone {
    async fn initialize_state() -> Self;
    async fn get_tasks_that_need_to_be_executed(&self) -> Vec<TaskType>;
    async fn add_task(&self, task: TaskType) -> u64;
    async fn filter_task_by_task_state(&self, task_state: &TaskState) -> Vec<TaskType>;
    async fn consume_tasks(&self) -> Vec<JoinHandle<TaskType>>;
    async fn delete_task_from_id(&self, id: u64) -> usize;
    async fn get_task_from_id(&self, id: u64) -> Option<TaskType>;
    async fn clear_all(&self);
    async fn update_task(&self, task: TaskType);
}

#[derive(Clone)]
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
            let rows_updated: usize;
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

    async fn filter_task_by_task_state(&self, task_state: &TaskState) -> Vec<TaskType> {
        let task_state_string: &str = task_state.into();
        let connection = self.pool.get().await.unwrap();
        connection.interact(move |connection| {
            let mut statement = connection.prepare("SELECT id, task_time, task_type, task_state FROM tasks WHERE tasks.task_state = ?1;").unwrap();
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
            }).unwrap().map(|task| {
                task.unwrap()
            }).collect()
        }).await.unwrap()
    }

    async fn consume_tasks(&self) -> Vec<JoinHandle<TaskType>> {
        let connection = self.pool.get().await.unwrap();
        let tasks_to_be_completed: Vec<TaskType> = connection.interact(|connection| {
            let transaction = connection.transaction().unwrap();
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();


            //This section fetches all tasks that need to be executed from the database and updates them to STARTED
            //It happens in one transaction effectively locking it from other calls to this method.
            //This section blocks and is the most expensive part of the task runner loop.
            let tasks_to_be_completed: Vec<TaskType> =
                transaction
                    .prepare("SELECT id, task_time, task_type, task_state FROM tasks WHERE tasks.task_state = 'NOTSTARTED' AND task_time <= ?1;")
                    .unwrap()
                    .query_map((now, ), |row| {
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
                    })
                    .unwrap()
                    .map(|result| { result.unwrap().transition_task() })
                    .collect();

            tasks_to_be_completed.iter().for_each(|not_started_task| {
                let parameters_not_started: (u64, &str) = not_started_task.get_parameters_from_task();
                transaction.execute(
                    "UPDATE tasks SET task_state = ?2 WHERE id = ?1",
                    parameters_not_started).unwrap();
            });
            transaction.commit().unwrap();
            tasks_to_be_completed
        }).await.unwrap();

        //This section executes the tasks async via spawn. When the task finishes it updates the database.
        //handlers are returned for potential blocking. This section is non blocking.
        tasks_to_be_completed.iter().map(|&task| {
            let state_copy = self.clone();
            let handler = tokio::spawn(async move {
                let finished_task = task.handle_task().await;
                state_copy.update_task(finished_task).await;
                finished_task
            });
            handler
        }).collect()
    }

    async fn delete_task_from_id(&self, id: u64) -> usize {
        let connection = self.pool.get().await.unwrap();
        connection.interact(move |connection| {
            let rows_deleted = connection.execute("DELETE FROM tasks WHERE id = ?1;", (id, )).unwrap();
            if rows_deleted > 1 {
                panic!("logic error only 1 or 0 rows should be deleted")
            }
            rows_deleted
        }).await.unwrap()
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

    async fn clear_all(&self) {
        let connection = self.pool.get().await.unwrap();
        connection.interact(|connection| {
            connection.execute("DELETE FROM tasks;", ()).unwrap()
        }).await.unwrap();
    }

    async fn update_task(&self, task: TaskType) {
        let connection = self.pool.get().await.unwrap();
        connection.interact(move |connection| {
            let parameters_finished: (u64, &str) = task.get_parameters_from_task();
            connection.execute("UPDATE tasks SET task_state = ?2 WHERE id = ?1", parameters_finished).unwrap()
        }).await.unwrap();
    }
}