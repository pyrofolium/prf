# task-runner-rust-async-demo

Small python challenge I found online, I decided to do it rust. 

Task Scheduler
The service consists of an API listener, which accepts HTTP API calls, and a worker which executes tasks of different types. There are 3 task types: "Foo", "Bar", and "Baz".

For "Foo" tasks, the worker should sleep for 3 seconds, and then print "Foo {task_id}".
For "Bar" tasks, the worker should make a GET request to https://www.whattimeisitrightnow.com/ and print the response's status code
For "Baz" tasks, the worker should generate a random number, N (0 to 343 inclusive), and print "Baz {N}"
Requirements
Expose an API that can:
Create a task of a specific type and execution time, returning the task's ID
Show a list of tasks, filterable by their state (whatever states you define) and/or their task type
Show a task based on its ID
Delete a task based on its ID
The tasks must be persisted into some external data store.
Only external services allowed: a database of your choice.
Process each task only once and only at/after their specified execution time.
Support running multiple instances of your code in parallel, such that a cloud deployment could be horizontally scaled.
