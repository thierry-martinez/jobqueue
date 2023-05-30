use zmq;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct CommandLine {
    command: String,
    args: Vec<String>,
}

impl CommandLine {
    pub fn new(command: impl ToString, args: &Vec<impl ToString>) -> Self {
	CommandLine { command: command.to_string(), args: args.iter().map(|s| s.to_string()).collect() }
    }
}

impl std::fmt::Display for CommandLine {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
	write!(fmt, "{}", self.command)?;
	for arg in &self.args {
	    write!(fmt, " {}", arg)?;
	}
	Ok(())
    }
}

type Fd = i32;

pub struct FdTable {
    contents: std::collections::HashMap<Fd, Vec<u8>>,
}

impl FdTable {
    pub fn new() -> Self {
	Self { contents: std::collections::HashMap::new() }
    }

    pub fn add(&mut self, fd: Fd, initial_vec: Vec<u8>) {
	assert!(self.contents.insert(fd, initial_vec).is_none())
    }

    pub fn add_empty(&mut self, fd: Fd) {
	self.add(fd, Vec::new())
    }

    pub fn append(&mut self, src: FdTable) {
	for (fd, mut vec) in src.contents {
	    self.contents.get_mut(&fd).unwrap().append(&mut vec)
	}
    }

    pub fn flush(&mut self) -> FdTable {
	let mut result = FdTable::new();
	for (fd, mut vec) in &mut self.contents {
	    result.add(*fd, std::mem::take(&mut vec))
	}
	result
    }

    pub fn is_empty(&self) -> bool {
	self.contents.iter().all(|(_fd, vec)| vec.is_empty())
    }

    pub fn take(&mut self, fd: Fd) -> Vec<u8> {
	std::mem::take(&mut self.contents.get_mut(&fd).unwrap())
    }
}

#[derive(Clone, Copy)]
pub enum ExitStatus {
    Success,
    Failure { error_code : i8 },
}

pub enum RunStatus<Running> {
    Running(Running),
    Stopped { status: ExitStatus },
}

pub trait Server {
    type Job: Job;
    type Run: Run;
    fn queue(&mut self, command_line: CommandLine) -> Self::Job;
    fn query(&mut self, callback: Box<dyn FnOnce(Query<Self::Run>)>);
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub enum ServerCommand {
    Queue(CommandLine),
    Query
}

pub struct ZmqListener<Server: Server> {
    server: Server;
    socket: zmq::Socket;
    job_count: i64;
    jobs: std::collections::HashMap<i64, Server::Job>;
}

impl<Server: Server> ZmqListener<Server> {
    pub fn new(server: Server, socket: zmq::Socket) -> Self {
	Self {
	    server,
	    socket,
	    job_count: 0,
	    jobs: std::collections::HashMap::new(),
	}
    }

    pub fn serve(&mut self) {
	loop {
	    let msg = self.socket.recv_multipart(0);
	    let identity = msg[0];
	    let command: ServerCommand = server_json.from_str(msg[1]);
	    let job = self.server.queue(command);
	    let job_id = self.job_count;
	    self.job_count = job_id + 1;
	    self.jobs.insert(job_id, job);
	    self.socket.send(vec![identity, &job_id.to_string()]);
	}
    }
}

pub struct ZmqServer {
    socket: zmq::Socket;
}

impl ZmqServer {
    pub fn new(socket: zmq::Socket) -> Self {
	Self { socket }
    }
}

impl Server for ZmqServer {
    type Job = ZmqJob;
    type Run = ZmqRun;

    fn queue(&mut self, command_line: CommandLine) -> Self::Job {
	let command_str = serde_json.to_string(ServerCommand::Queue(command_line));
	self.socket.send(&command_str, 0);
	let id_str = self.socket.recv_string().unwrap().unwrap();
	ZmqJob { id: id_str.parse().unwrap() }
    }

    fn query(&mut self, callback: Box<dyn FnOnce(Query<Self::Run>)>) {
    }
}

pub struct ZmqJob {
    id: i64;
}

impl Job for ZmqJob {
    fn watch(&self, input: FdTable, callback: Box<dyn FnOnce(JobStatus<Output>)>) {
    }
    fn signal(&self, signal: Signal) {
    }
    fn cancel(&self) {
    }
}

pub struct ZmqRun {
    id: i64;
}

impl Run for ZmqRun {
    fn report(&self, feedback: RunStatus<Output>) -> Option<Interaction> {
	None
    }
}

pub enum Signal {
    Kill,
}

pub struct Output {
    output: FdTable,
}

pub trait Job {
    fn watch(&self, input: FdTable, callback: Box<dyn FnOnce(JobStatus<Output>)>);
    fn signal(&self, signal: Signal);
    fn cancel(&self);
}

pub struct Query<Run> {
    command_line: CommandLine,
    run: Run,
}

pub enum Interaction {
    Input(FdTable),
    Signal(Vec<Signal>),
}

pub trait Run {
    fn report(&self, feedback: RunStatus<Output>) -> Option<Interaction>;
}

pub enum JobStatus<Running> {
    Waiting,
    Lost,
    Run(RunStatus<Running>)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestServer {
	jobs: std::collections::VecDeque<TestJob>,
	queries: std::collections::VecDeque<PendingQuery>,
    }

    type PendingQuery = Box<dyn FnOnce(Query<TestRun>)>;

    struct TestJobDesc {
	command_line: CommandLine,
	status: std::sync::Mutex<JobStatus<()>>,
	input_fds: std::sync::Mutex<FdTable>,
	output_fds: std::sync::Mutex<FdTable>,
	cancelled: std::sync::Mutex<bool>,
	signals: std::sync::Mutex<Vec<Signal>>,
    }

    impl TestJobDesc {
	fn new(command_line: CommandLine) -> Self {
	    let mut input_fds = FdTable::new();
	    let mut output_fds = FdTable::new();
	    input_fds.add_empty(0);
	    output_fds.add_empty(1);
	    output_fds.add_empty(2);
	    let input_fds = std::sync::Mutex::new(input_fds);
	    let output_fds = std::sync::Mutex::new(output_fds);
	    Self {
		command_line,
		status: std::sync::Mutex::new(JobStatus::Waiting),
		input_fds,
		output_fds,
		cancelled: std::sync::Mutex::new(false),
		signals: std::sync::Mutex::new(Vec::new()),
	    }
	}
    }

    type TestJob = std::sync::Arc<TestJobDesc>;

    struct TestRun {
	job: TestJob,
    }

    impl TestServer {
	fn new() -> Self {
	    Self {
		jobs: std::collections::VecDeque::new(),
		queries: std::collections::VecDeque::new(),
	    }
	}

	fn run(&self, query: PendingQuery, job: TestJob) {
	    let command_line = job.command_line.clone();
	    *job.status.lock().unwrap() = JobStatus::Run(RunStatus::Running(()));
	    let run = TestRun { job };
	    query(Query { command_line, run })
	}
    }

    impl Server for TestServer {
	type Job = TestJob;
	type Run = TestRun;

	fn queue(&mut self, command_line: CommandLine) -> Self::Job {
	    let desc = TestJobDesc::new(command_line);
	    let result = TestJob::new(desc);
	    match self.queries.pop_front() {
		None => {
		    self.jobs.push_back(result.clone())
		},
		Some(query) => {
		    self.run(query, result.clone())
		}
	    }
	    result
	}

	fn query(&mut self, callback: Box<dyn FnOnce(Query<Self::Run>)>) {
	    match self.jobs.pop_front() {
		None => {
		    self.queries.push_back(callback)
		},
		Some(job) => {
		    self.run(callback, job)
		}
	    }
	}
    }

    impl Job for TestJob {
	fn watch(&self, input: FdTable, callback: Box<dyn FnOnce(JobStatus<Output>)>) {
	    self.input_fds.lock().unwrap().append(input);
	    let mut status = self.status.lock().unwrap();
	    let mut output_fds = self.output_fds.lock().unwrap();
	    match *status {
		JobStatus::Run(RunStatus::Running(())) => {
		    callback(JobStatus::Run (RunStatus::Running (Output { output: output_fds.flush() })))
		}
		JobStatus::Run(RunStatus::Stopped { status }) => {
		    if output_fds.is_empty() {
			callback(JobStatus::Run (RunStatus::Stopped { status }))
		    }
		    else {
			callback(JobStatus::Run (RunStatus::Running (Output { output: output_fds.flush() })))
		    }
		}
		JobStatus::Waiting => callback(JobStatus::Waiting),
		JobStatus::Lost => {
		    *status = JobStatus::Waiting;
		    callback(JobStatus::Lost)
		}
	    }
	}

	fn signal(&self, signal: Signal) {
	    self.signals.lock().unwrap().push(signal);
	}

	fn cancel(&self) {
	    *self.cancelled.lock().unwrap() = true;
	}
    }

    impl Run for TestRun {
	fn report(&self, feedback: RunStatus<Output>) -> Option<Interaction> {
	    match feedback {
		RunStatus::Stopped { status } => {
		    *self.job.status.lock().unwrap() = JobStatus::Run(RunStatus::Stopped { status });
		    None
		}
		RunStatus::Running(output) => {
		    self.job.output_fds.lock().unwrap().append(output.output);
		    let mut signals = self.job.signals.lock().unwrap();
		    if signals.is_empty() {
			Some(Interaction::Input(self.job.input_fds.lock().unwrap().flush()))
		    }
		    else {
			Some(Interaction::Signal(std::mem::take(&mut signals)))
		    }
		}
	    }
	}
    }

    #[test]
    fn server() {
	let mut server = TestServer::new();
	let job = server.queue(CommandLine::new("ls", &vec!["-a"]));
	let received = std::sync::Arc::new(std::sync::Mutex::new(false));
	let received_clone = received.clone();
	server.query(Box::new(|query| {
	    assert!(query.command_line.command == "ls");
	    let mut output = FdTable::new();
	    output.add(1, vec![42]);
	    query.run.report(RunStatus::Running(Output { output }));
	}));
	job.watch(FdTable::new(), Box::new(move |status| {
	    match status {
		JobStatus::Run(RunStatus::Running(Output { mut output })) => {
		    assert!(output.take(1) == vec![42]);
		    *received_clone.lock().unwrap() = true;
		}
		_ => panic!()
	    }
	}));
	assert!(*received.lock().unwrap());
    }

    #[zmq_test]
    fn server() {
	let mut server = TestServer::new();
	let server_socket = 
	let mut listener = ZmqListener(server, server_socket);
	let job = server.queue(CommandLine::new("ls", &vec!["-a"]));
	let received = std::sync::Arc::new(std::sync::Mutex::new(false));
	let received_clone = received.clone();
	server.query(Box::new(|query| {
	    assert!(query.command_line.command == "ls");
	    let mut output = FdTable::new();
	    output.add(1, vec![42]);
	    query.run.report(RunStatus::Running(Output { output }));
	}));
	job.watch(FdTable::new(), Box::new(move |status| {
	    match status {
		JobStatus::Run(RunStatus::Running(Output { mut output })) => {
		    assert!(output.take(1) == vec![42]);
		    *received_clone.lock().unwrap() = true;
		}
		_ => panic!()
	    }
	}));
	assert!(*received.lock().unwrap());
    }
}
