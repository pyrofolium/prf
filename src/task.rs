use std::str::FromStr;
use std::time::Duration;
use async_std;
use TaskType::{Bar, Baz, Foo};
use serde::{Deserialize, Serialize};
use isahc;


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

//sort of redundant to the From impl below but it was required by warp.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

impl FromStr for TaskState {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            TaskState::STARTED => Ok(Self::Started),
            TaskState::NOTSTARTED => Ok(Self::NotStarted),
            TaskState::COMPLETE => Ok(Self::Complete),
            _ => Err(())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskType {
    Foo(Option<TaskId>, TaskTimeStamp, TaskState),
    Bar(Option<TaskId>, TaskTimeStamp, TaskState),
    Baz(Option<TaskId>, TaskTimeStamp, TaskState),
}

impl TaskType {
    pub fn transition_task(&self) -> TaskType {
        match self {
            Foo(id, time, state) =>
                Foo(*id, *time, state.transition_state()),
            Bar(id, time, state) =>
                Bar(*id, *time, state.transition_state()),
            Baz(id, time, state) =>
                Baz(*id, *time, state.transition_state())
        }
    }

    pub fn get_parameters_from_task(&self) -> (TaskId, &'static str) {
        match self {
            Foo(Some(id), _, state) => (*id, state.into()),
            Bar(Some(id), _, state) => (*id, state.into()),
            Baz(Some(id), _, state) => (*id, state.into()),
            _ => {
                panic!("Cannot update tasks with no id.")
            }
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
                if let Ok(response) = isahc::get(url) {
                    let status = response.status();
                    let status_code = status.as_u16();
                    println!("{status_code}");
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