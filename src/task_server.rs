use warp::Filter;
use crate::task_state::{SqliteState, State};
use crate::task::{TaskConstants, TaskState, TaskType};
use crate::task::TaskState::NotStarted;
use crate::task::TaskType::{Bar, Baz, Foo};

//warp has a complex typing schema. Not worth it to bend a generic to get it working
//with the State interface
// pub async fn run_server<T: State>(state: T)
pub async fn run_server(state: SqliteState) {
    let warp_state = warp::any().map(move || {
        state.clone()
    });

    async fn get_task_handler(id: u64, state: SqliteState) -> Result<impl warp::Reply, warp::Rejection> {
        match state.get_task_from_id(id).await {
            Some(task) => Ok(warp::reply::json(&task)),
            None => Err(warp::reject::not_found())
        }
    }

    async fn get_tasks_by_state_handler(task_state: TaskState, state: SqliteState) -> Result<impl warp::Reply, warp::Rejection> {
        let result = state.get_all_tasks(&task_state).await;
        Ok(warp::reply::json(&result))
    }

    async fn del_task_handler(id: u64, state: SqliteState) -> Result<impl warp::Reply, warp::Rejection> {
        let rows_deleted = state.delete_task_from_id(id).await;
        Ok(warp::reply::json(&rows_deleted))
    }

    async fn post_new_task_handler(task_type_str: String, time: u64, state: SqliteState) -> Result<impl warp::Reply, warp::Rejection> {
        let task = match task_type_str.as_str() {
            TaskType::BAR => Bar(None, time, NotStarted),
            TaskType::BAZ => Baz(None, time, NotStarted),
            TaskType::FOO => Foo(None, time, NotStarted),
            _ => {
                return Err(warp::reject::not_found());
            }
        };
        let id = state.add_task(task).await;
        Ok(warp::reply::json(&id))
    }

    async fn del_all_handler(state: SqliteState) -> Result<impl warp::Reply, warp::Rejection> {
        state.clear_all().await;
        Ok(warp::http::StatusCode::OK)
    }

    let post_new_task_route = warp::post()
        .and(warp::path!(String / u64))
        .and(warp::path::end())
        .and(warp_state.clone())
        .and_then(post_new_task_handler);

    let del_task_route = warp::delete()
        .and(warp::path!("id" / u64))
        .and(warp::path::end())
        .and(warp_state.clone())
        .and_then(del_task_handler);

    let del_all_route = warp::delete()
        .and(warp::path::end())
        .and(warp_state.clone())
        .and_then(del_all_handler);

    let get_tasks_by_state_route = warp::get()
        .and(warp::path!("taskstate" / TaskState))
        .and(warp::path::end())
        .and(warp_state.clone())
        .and_then(get_tasks_by_state_handler);

    let get_task_route = warp::get()
        .and(warp::path!("id" / u64))
        .and(warp::path::end())
        .and(warp_state.clone())
        .and_then(get_task_handler);

    let router =
        get_task_route
            .or(get_tasks_by_state_route)
            .or(del_task_route)
            .or(post_new_task_route)
            .or(del_all_route);


    warp::serve(router).run(([127, 0, 0, 1], 3030)).await;
}