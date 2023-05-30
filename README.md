# jobqueue: job stealing everywhere


Run a server on a VM:

```
$ jobqueue-server localhost:3821
```

Run workers:

```
  $ jobqueue-worker server:3821
```

Run jobs:
```
$ jq command-line
```

## What do the `jq` tool does?

The `jq` tool:
- executes `command-line` on the first ready worker,
- redirects standard input, standard output and standard error, signals (jobs can be interrupted by `Ctrl-C`),
- waits that the job is completely executed and terminates with the same exit code than the job itself,
- if the worker disconnects before the job is terminated, the job is requeued completely.

In other words, `jq` mimicks the execution of `command-line`, but the execution is deported on a worker.
