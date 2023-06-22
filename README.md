# task-runner-challenge

Small python challenge I found online, I decided to do a lightweight monolithic version with sqlite. 

## Task Scheduler
The service consists of an API listener, which accepts HTTP API calls, and a worker which executes tasks of different types. There are 3 task types: "Foo", "Bar", and "Baz".

- For "Foo" tasks, the worker should sleep for 3 seconds, and then print "Foo {task_id}".
- For "Bar" tasks, the worker should make a GET request to https://www.whattimeisitrightnow.com/ and print the response's status code
- For "Baz" tasks, the worker should generate a random number, N (0 to 343 inclusive), and print "Baz {N}"

## Requirements
Expose an API that can:
- Create a task of a specific type and execution time, returning the task's ID
- Show a list of tasks, filterable by their state (whatever states you define) and/or their task type
- Show a task based on its ID
- Delete a task based on its ID
- The tasks must be persisted into some external data store.
- Only external services allowed: a database of your choice.
- Process each task only once and only at/after their specified execution time.
- Support running multiple instances of your code in parallel, such that a cloud deployment could be horizontally scaled.


# Solution
## 5 files:
- Task: Has types for each task and functions related to the task itself, Foo, Bar and Baz
- State: A trait called State responsible for handling all shared state. This was implemented with sqlite.
- Runner: A task runner that composes State and Task types into a loop that constantly runs based off of whats in the database.
- Server: Warp http server with routes made according to the spec to manipulate the database.
- Main: Not really a module, just the fn main. It runs two threads of Runner and Server. I mention it here because it can be changed to several main targets.

## Sample http calls:

### GET
#### 127.0.0.1:3030/id/1
Gets task by id. Returns json. path: /id/ (task id)

### GET
#### 127.0.0.1:3030/taskstate/COMPLETE
Gets list of tasks in json filtered by task_state (STARTED, NOTSTARTED, COMPLETE)
path: /taskstate/ (task_state)

### GET
#### 127.0.0.1:3030/taskstate/NOTSTARTED

### GET
#### 127.0.0.1:3030/taskstate/STARTED

### POST
#### 127.0.0.1:3030/FOO/0
path: (task type) / (timestamp)
Creates Job with timestamp. Returns task id.

### POST
#### 127.0.0.1:3030/FOO/0

### POST
#### 127.0.0.1:3030/BAZ/0

### POST
#### 127.0.0.1:3030/FOO/64072713903

### DELETE
#### 127.0.0.1:3030/
deletes all tasks, for testing.

### DELETE
#### 127.0.0.1:3030/id/1
Deletes task by id.
