use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, mpsc};
use std::task::{Context, Poll, Wake, Waker};

use crate::host::HostRuntime;
use crate::runtime::{
    OperationId, RuntimeError, RuntimeErrorKind, RuntimeResult, TaskId, Value, Vm, VmRunOutcome,
};

pub type OperationFuture = Pin<Box<dyn Future<Output = RuntimeResult<Value>> + Send + 'static>>;

#[derive(Clone, Debug)]
pub enum TaskState {
    Ready,
    Running,
    Waiting(OperationId),
    Completed(Value),
    Failed(RuntimeError),
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct ScriptTask {
    pub id: TaskId,
    pub vm: Vm,
    pub parent: Option<TaskId>,
    pub children: HashSet<TaskId>,
    pub state: TaskState,
}

struct PendingOperation {
    future: OperationFuture,
}

struct OperationWaker {
    operation: OperationId,
    sender: mpsc::Sender<OperationId>,
}

impl Wake for OperationWaker {
    fn wake(self: Arc<Self>) {
        let _ = self.sender.send(self.operation);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        let _ = self.sender.send(self.operation);
    }
}

pub struct Scheduler {
    pub tasks: HashMap<TaskId, ScriptTask>,
    pub ready_queue: VecDeque<TaskId>,
    pub instruction_quantum: usize,
    pub max_tasks: usize,
    waiting: HashMap<OperationId, TaskId>,
    operations: HashMap<OperationId, PendingOperation>,
    completed_operations: HashMap<OperationId, RuntimeResult<Value>>,
    queued_operations: HashSet<OperationId>,
    operation_queue: VecDeque<OperationId>,
    wake_sender: mpsc::Sender<OperationId>,
    wake_receiver: mpsc::Receiver<OperationId>,
    next_task_id: u64,
    next_operation_id: u64,
    host: Option<HostRuntime>,
}

impl fmt::Debug for Scheduler {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Scheduler")
            .field("tasks", &self.tasks)
            .field("ready_queue", &self.ready_queue)
            .field("instruction_quantum", &self.instruction_quantum)
            .field("waiting", &self.waiting)
            .field("pending_operations", &self.operations.len())
            .finish()
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(10_000)
    }
}

impl Scheduler {
    pub fn new(instruction_quantum: usize) -> Self {
        assert!(
            instruction_quantum > 0,
            "instruction quantum must be non-zero"
        );
        let (wake_sender, wake_receiver) = mpsc::channel();
        Self {
            tasks: HashMap::new(),
            ready_queue: VecDeque::new(),
            instruction_quantum,
            max_tasks: 1_024,
            waiting: HashMap::new(),
            operations: HashMap::new(),
            completed_operations: HashMap::new(),
            queued_operations: HashSet::new(),
            operation_queue: VecDeque::new(),
            wake_sender,
            wake_receiver,
            next_task_id: 0,
            next_operation_id: 0,
            host: None,
        }
    }

    pub fn spawn(&mut self, vm: Vm) -> RuntimeResult<TaskId> {
        self.spawn_child(vm, None)
    }

    pub fn spawn_child(&mut self, mut vm: Vm, parent: Option<TaskId>) -> RuntimeResult<TaskId> {
        if self.tasks.len() >= self.max_tasks {
            return Err(RuntimeError::coded(
                "E1240",
                RuntimeErrorKind::ResourceLimit,
                "maximum script task count exceeded",
            ));
        }
        if let Some(parent) = parent
            && !self.tasks.contains_key(&parent)
        {
            return Err(RuntimeError::coded(
                "E900",
                RuntimeErrorKind::Internal,
                "parent task does not exist",
            ));
        }
        if let Some(host) = &self.host {
            host.install_globals(&mut vm);
        }
        let id = TaskId(self.next_task_id);
        self.next_task_id += 1;
        self.tasks.insert(
            id,
            ScriptTask {
                id,
                vm,
                parent,
                children: HashSet::new(),
                state: TaskState::Ready,
            },
        );
        if let Some(parent) = parent {
            self.tasks
                .get_mut(&parent)
                .expect("parent checked")
                .children
                .insert(id);
        }
        self.ready_queue.push_back(id);
        Ok(id)
    }

    pub fn set_host(&mut self, host: HostRuntime) {
        for task in self.tasks.values_mut() {
            host.install_globals(&mut task.vm);
        }
        self.host = Some(host);
    }

    pub fn host(&self) -> Option<&HostRuntime> {
        self.host.as_ref()
    }
    pub fn host_mut(&mut self) -> Option<&mut HostRuntime> {
        self.host.as_mut()
    }

    pub fn register<F>(&mut self, future: F) -> OperationId
    where
        F: Future<Output = RuntimeResult<Value>> + Send + 'static,
    {
        self.register_boxed(Box::pin(future))
    }

    pub fn register_boxed(&mut self, future: OperationFuture) -> OperationId {
        let id = OperationId(self.next_operation_id);
        self.next_operation_id += 1;
        self.operations.insert(id, PendingOperation { future });
        self.enqueue_operation(id);
        id
    }

    pub fn task(&self, id: TaskId) -> Option<&ScriptTask> {
        self.tasks.get(&id)
    }
    pub fn task_mut(&mut self, id: TaskId) -> Option<&mut ScriptTask> {
        self.tasks.get_mut(&id)
    }

    /// Runs one operation poll and one VM quantum. Returns whether progress was made.
    pub fn tick(&mut self) -> RuntimeResult<bool> {
        self.collect_wakes();
        let mut progressed = self.poll_one_operation()?;
        if let Some(task) = self.ready_queue.pop_front() {
            self.run_task(task)?;
            progressed = true;
        }
        Ok(progressed)
    }

    /// Blocks only waiting for a future wake when every VM is suspended.
    pub fn run_until_complete(&mut self, task: TaskId) -> RuntimeResult<Value> {
        loop {
            if let Some(result) = self.terminal_result(task) {
                return result;
            }
            if self.tick()? {
                continue;
            }
            let operation = self.wake_receiver.recv().map_err(|_| {
                RuntimeError::coded(
                    "E900",
                    RuntimeErrorKind::Internal,
                    "scheduler wake channel disconnected",
                )
            })?;
            self.enqueue_operation(operation);
        }
    }

    pub fn run_until_stalled(&mut self) -> RuntimeResult<()> {
        while self.tick()? {}
        Ok(())
    }

    pub fn cancel(&mut self, task: TaskId) -> bool {
        let Some(children) = self
            .tasks
            .get(&task)
            .map(|task| task.children.iter().copied().collect::<Vec<_>>())
        else {
            return false;
        };
        for child in children {
            self.cancel(child);
        }
        let operation = match self.tasks.get(&task).map(|task| &task.state) {
            Some(TaskState::Waiting(operation)) => Some(*operation),
            _ => None,
        };
        if let Some(operation) = operation {
            self.waiting.remove(&operation);
            self.operations.remove(&operation);
            self.completed_operations.remove(&operation);
            self.queued_operations.remove(&operation);
        }
        self.ready_queue.retain(|queued| *queued != task);
        if let Some(task) = self.tasks.get_mut(&task) {
            task.state = TaskState::Cancelled;
            task.vm.status = crate::runtime::VmStatus::Failed(RuntimeError::coded(
                "E_CANCELLED",
                RuntimeErrorKind::Cancelled,
                "script task cancelled",
            ));
        }
        true
    }

    fn run_task(&mut self, id: TaskId) -> RuntimeResult<()> {
        let outcome = {
            let Some(task) = self.tasks.get_mut(&id) else {
                return Ok(());
            };
            if !matches!(task.state, TaskState::Ready) {
                return Ok(());
            }
            task.state = TaskState::Running;
            task.vm.run_quantum(self.instruction_quantum)
        };
        match outcome {
            Ok(VmRunOutcome::Yielded) => {
                self.tasks.get_mut(&id).expect("task exists").state = TaskState::Ready;
                self.ready_queue.push_back(id);
            }
            Ok(VmRunOutcome::Completed(value)) => {
                self.tasks.get_mut(&id).expect("task exists").state = TaskState::Completed(value)
            }
            Ok(VmRunOutcome::HostCall(request)) => {
                let dispatched = self
                    .host
                    .as_ref()
                    .ok_or_else(|| {
                        RuntimeError::coded(
                            "E_HOST",
                            RuntimeErrorKind::HostError,
                            "no host runtime is configured",
                        )
                    })
                    .and_then(|host| host.dispatch(request));
                let result = dispatched.map(|future| self.register_boxed(future));
                self.resume_host_call(id, result)?;
            }
            Ok(VmRunOutcome::OptionCall(request)) => {
                let dispatched = self
                    .host
                    .as_ref()
                    .ok_or_else(|| {
                        RuntimeError::coded(
                            "E_HOST",
                            RuntimeErrorKind::HostError,
                            "no host runtime is configured",
                        )
                    })
                    .and_then(|host| host.dispatch_option(request));
                let result = dispatched.map(|future| self.register_boxed(future));
                self.resume_host_call(id, result)?;
            }
            Ok(VmRunOutcome::CommandCall(request)) => {
                if request.command.name == "command" {
                    let result = self
                        .host
                        .as_mut()
                        .ok_or_else(|| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "no host runtime is configured",
                            )
                        })
                        .and_then(|host| host.define_user_command(&request.command))
                        .map(|()| Value::Null);
                    self.complete_command(id, result)?;
                } else if request.command.name == "delcommand" {
                    let result = self
                        .host
                        .as_mut()
                        .ok_or_else(|| {
                            RuntimeError::coded(
                                "E_HOST",
                                RuntimeErrorKind::HostError,
                                "no host runtime is configured",
                            )
                        })
                        .and_then(|host| host.delete_user_command(&request.command))
                        .map(|()| Value::Null);
                    self.complete_command(id, result)?;
                } else {
                    let registration = self
                        .host
                        .as_mut()
                        .and_then(|host| host.handle_registration_command(&request));
                    if let Some(result) = registration {
                        self.complete_command(id, result.map(|()| Value::Null))?;
                    } else {
                        let dispatched = self
                            .host
                            .as_ref()
                            .ok_or_else(|| {
                                RuntimeError::coded(
                                    "E_HOST",
                                    RuntimeErrorKind::HostError,
                                    "no host runtime is configured",
                                )
                            })
                            .and_then(|host| host.prepare_command(request))
                            .and_then(|request| {
                                self.host
                                    .as_ref()
                                    .expect("host checked")
                                    .dispatch_command(request)
                            });
                        match dispatched {
                            Ok(future) => {
                                let operation = self.register_boxed(future);
                                let task = self.tasks.get_mut(&id).expect("task exists");
                                task.vm.suspend_for_operation(operation);
                                task.state = TaskState::Waiting(operation);
                                self.waiting.insert(operation, id);
                            }
                            Err(error) => self.complete_command(id, Err(error))?,
                        }
                    }
                }
            }
            Ok(VmRunOutcome::Waiting(operation)) => {
                if let Some(result) = self.completed_operations.remove(&operation) {
                    self.resume_task(id, result)?;
                } else if self.operations.contains_key(&operation) {
                    self.tasks.get_mut(&id).expect("task exists").state =
                        TaskState::Waiting(operation);
                    self.waiting.insert(operation, id);
                } else {
                    let error = RuntimeError::coded(
                        "E900",
                        RuntimeErrorKind::Internal,
                        format!("unknown async operation {}", operation.0),
                    );
                    let task = self.tasks.get_mut(&id).expect("task exists");
                    task.state = TaskState::Failed(error.clone());
                    task.vm.status = crate::runtime::VmStatus::Failed(error);
                }
            }
            Err(error) => {
                self.tasks.get_mut(&id).expect("task exists").state = TaskState::Failed(error)
            }
        }
        Ok(())
    }

    fn poll_one_operation(&mut self) -> RuntimeResult<bool> {
        let Some(id) = self.operation_queue.pop_front() else {
            return Ok(false);
        };
        self.queued_operations.remove(&id);
        let Some(operation) = self.operations.get_mut(&id) else {
            return Ok(false);
        };
        let waker = Waker::from(Arc::new(OperationWaker {
            operation: id,
            sender: self.wake_sender.clone(),
        }));
        let mut context = Context::from_waker(&waker);
        match operation.future.as_mut().poll(&mut context) {
            Poll::Pending => {}
            Poll::Ready(result) => {
                self.operations.remove(&id);
                if let Some(task) = self.waiting.remove(&id) {
                    self.resume_task(task, result)?;
                } else {
                    self.completed_operations.insert(id, result);
                }
            }
        }
        Ok(true)
    }

    fn complete_command(&mut self, id: TaskId, result: RuntimeResult<Value>) -> RuntimeResult<()> {
        let task = self.tasks.get_mut(&id).ok_or_else(|| {
            RuntimeError::coded(
                "E900",
                RuntimeErrorKind::Internal,
                "command task disappeared",
            )
        })?;
        match task.vm.complete_command(result) {
            Ok(()) => {
                task.state = TaskState::Ready;
                self.ready_queue.push_back(id);
            }
            Err(error) => task.state = TaskState::Failed(error),
        }
        Ok(())
    }

    fn resume_host_call(
        &mut self,
        id: TaskId,
        result: RuntimeResult<OperationId>,
    ) -> RuntimeResult<()> {
        let task = self.tasks.get_mut(&id).ok_or_else(|| {
            RuntimeError::coded(
                "E900",
                RuntimeErrorKind::Internal,
                "host-calling task disappeared",
            )
        })?;
        match task.vm.resume_host_call(result) {
            Ok(()) => {
                task.state = TaskState::Ready;
                self.ready_queue.push_back(id);
            }
            Err(error) => task.state = TaskState::Failed(error),
        }
        Ok(())
    }

    fn resume_task(&mut self, id: TaskId, result: RuntimeResult<Value>) -> RuntimeResult<()> {
        let task = self.tasks.get_mut(&id).ok_or_else(|| {
            RuntimeError::coded(
                "E900",
                RuntimeErrorKind::Internal,
                "waiting task disappeared",
            )
        })?;
        match task.vm.resume_await(result) {
            Ok(()) => {
                task.state = TaskState::Ready;
                self.ready_queue.push_back(id);
            }
            Err(error) => task.state = TaskState::Failed(error),
        }
        Ok(())
    }

    fn terminal_result(&self, id: TaskId) -> Option<RuntimeResult<Value>> {
        match &self.tasks.get(&id)?.state {
            TaskState::Completed(value) => Some(Ok(value.clone())),
            TaskState::Failed(error) => Some(Err(error.clone())),
            TaskState::Cancelled => Some(Err(RuntimeError::coded(
                "E_CANCELLED",
                RuntimeErrorKind::Cancelled,
                "script task cancelled",
            ))),
            _ => None,
        }
    }

    fn collect_wakes(&mut self) {
        while let Ok(operation) = self.wake_receiver.try_recv() {
            self.enqueue_operation(operation);
        }
    }
    fn enqueue_operation(&mut self, operation: OperationId) {
        if self.operations.contains_key(&operation) && self.queued_operations.insert(operation) {
            self.operation_queue.push_back(operation);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::Compiler;
    use crate::host::{
        Arity, Capability, CommandDefinition, CommandRequest, Host, HostContext, HostRequest,
        HostRuntime, OptionRequest, OptionRequestOperation,
    };
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::resolver::{Resolver, ResolverConfig};
    use crate::source::SourceId;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration;

    fn vm(source: &str, globals: HashMap<String, Value>) -> Vm {
        let lexed = Lexer::new(SourceId(0), source).lex();
        assert!(lexed.diagnostics.is_empty());
        let parsed = Parser::new(&lexed.tokens).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        let resolved = Resolver::new(ResolverConfig::default()).resolve(parsed.program.unwrap());
        assert!(
            resolved.diagnostics.is_empty(),
            "{:?}",
            resolved.diagnostics
        );
        let compiled = Compiler::new(&resolved.program.unwrap()).compile();
        assert!(
            compiled.diagnostics.is_empty(),
            "{:?}",
            compiled.diagnostics
        );
        Vm::with_globals(compiled.module.unwrap(), globals).unwrap()
    }

    #[derive(Default)]
    struct MockHost {
        requests: Arc<Mutex<Vec<HostRequest>>>,
    }

    impl Host for MockHost {
        fn call(&self, request: HostRequest) -> crate::host::HostFuture {
            self.requests.lock().unwrap().push(request.clone());
            Box::pin(async move {
                let name = request.arguments.first().cloned().unwrap_or(Value::Null);
                Ok(Value::String(Arc::from(format!("read:{name:?}"))))
            })
        }
    }

    struct CommandHost {
        commands: Arc<Mutex<Vec<CommandRequest>>>,
    }
    impl Host for CommandHost {
        fn call(&self, _request: HostRequest) -> crate::host::HostFuture {
            Box::pin(async { Ok(Value::Null) })
        }
        fn execute_command(&self, request: CommandRequest) -> crate::host::HostFuture {
            self.commands.lock().unwrap().push(request);
            Box::pin(async { Ok(Value::Null) })
        }
    }

    struct OptionHost {
        value: Arc<Mutex<Value>>,
    }

    impl Host for OptionHost {
        fn call(&self, _request: HostRequest) -> crate::host::HostFuture {
            Box::pin(async { Ok(Value::Null) })
        }

        fn option(&self, request: OptionRequest) -> crate::host::HostFuture {
            let value = self.value.clone();
            Box::pin(async move {
                match request.operation {
                    OptionRequestOperation::Get => Ok(value.lock().unwrap().clone()),
                    OptionRequestOperation::Set(new_value) => {
                        *value.lock().unwrap() = new_value;
                        Ok(Value::Null)
                    }
                }
            })
        }
    }

    struct DelayedFuture {
        state: Arc<Mutex<DelayedState>>,
    }
    struct DelayedState {
        value: Option<Value>,
        waker: Option<Waker>,
    }
    impl Future for DelayedFuture {
        type Output = RuntimeResult<Value>;
        fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
            let mut state = self.state.lock().unwrap();
            if let Some(value) = state.value.take() {
                Poll::Ready(Ok(value))
            } else {
                state.waker = Some(context.waker().clone());
                Poll::Pending
            }
        }
    }

    #[test]
    fn suspends_and_resumes_from_another_thread() {
        let mut scheduler = Scheduler::new(10);
        let state = Arc::new(Mutex::new(DelayedState {
            value: None,
            waker: None,
        }));
        let operation = scheduler.register(DelayedFuture {
            state: state.clone(),
        });
        let globals = HashMap::from([("g:future".into(), Value::Future(operation))]);
        let task = scheduler
            .spawn(vm("let g:result = await g:future\n", globals))
            .unwrap();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            let mut state = state.lock().unwrap();
            state.value = Some(Value::Integer(42));
            if let Some(waker) = state.waker.take() {
                waker.wake();
            }
        });
        assert_eq!(scheduler.run_until_complete(task).unwrap(), Value::Null);
        assert_eq!(
            scheduler.task(task).unwrap().vm.globals.get("g:result"),
            Some(&Value::Integer(42))
        );
    }

    #[test]
    fn immediate_operations_resume_without_blocking() {
        let mut scheduler = Scheduler::new(1);
        let operation = scheduler.register(async { Ok(Value::String(Arc::from("done"))) });
        let globals = HashMap::from([("g:future".into(), Value::Future(operation))]);
        let task = scheduler
            .spawn(vm("let g:result = await g:future\n", globals))
            .unwrap();
        assert_eq!(scheduler.run_until_complete(task).unwrap(), Value::Null);
        assert_eq!(
            scheduler.task(task).unwrap().vm.globals.get("g:result"),
            Some(&Value::String(Arc::from("done")))
        );
    }

    #[test]
    fn ex_commands_are_resolved_and_implicitly_awaited() {
        let commands = Arc::new(Mutex::new(Vec::new()));
        let mut runtime = HostRuntime::new(Arc::new(CommandHost {
            commands: commands.clone(),
        }));
        runtime.capabilities.grant(Capability::FileSystemWrite);
        runtime.register_command(CommandDefinition {
            name: "write".into(),
            minimum_abbreviation: 1,
            accepts_bang: true,
            accepts_range: false,
            accepts_count: false,
            accepts_register: false,
            required_capabilities: vec![Capability::FileSystemWrite],
        });
        let mut scheduler = Scheduler::new(10);
        scheduler.set_host(runtime);
        let task = scheduler
            .spawn(vm(":w\nlet g:after = 1\n", HashMap::new()))
            .unwrap();
        assert_eq!(scheduler.run_until_complete(task).unwrap(), Value::Null);
        assert_eq!(
            scheduler.task(task).unwrap().vm.globals.get("g:after"),
            Some(&Value::Integer(1))
        );
        let commands = commands.lock().unwrap();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].command.name, "write");
    }

    #[test]
    fn option_reads_writes_and_compound_assignments_are_implicitly_awaited() {
        let value = Arc::new(Mutex::new(Value::Integer(40)));
        let mut runtime = HostRuntime::new(Arc::new(OptionHost {
            value: value.clone(),
        }));
        runtime.capabilities.grant(Capability::Settings);
        let mut scheduler = Scheduler::new(10);
        scheduler.set_host(runtime);
        let task = scheduler
            .spawn(vm(
                "let g:before = &number\nlet &number += 2\nlet g:after = &number\n",
                HashMap::new(),
            ))
            .unwrap();

        scheduler.run_until_complete(task).unwrap();
        let globals = &scheduler.task(task).unwrap().vm.globals;
        assert_eq!(globals.get("g:before"), Some(&Value::Integer(40)));
        assert_eq!(globals.get("g:after"), Some(&Value::Integer(42)));
        assert_eq!(*value.lock().unwrap(), Value::Integer(42));
    }

    #[test]
    fn dispatches_capability_checked_host_calls() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let host = Arc::new(MockHost {
            requests: requests.clone(),
        });
        let mut runtime = HostRuntime::new(host);
        runtime.capabilities.grant(Capability::FileSystemRead);
        runtime.register_function(
            "read_file",
            Arity::Exact(1),
            vec![Capability::FileSystemRead],
        );
        let mut scheduler = Scheduler::new(10);
        scheduler.set_host(runtime);
        let mut script_vm = vm(
            "let g:result = await g:read_file('notes.txt')\n",
            HashMap::new(),
        );
        script_vm.host_context = HostContext {
            script_name: Some("plugin.vim".into()),
            ..HostContext::default()
        };
        let task = scheduler.spawn(script_vm).unwrap();
        assert_eq!(scheduler.run_until_complete(task).unwrap(), Value::Null);
        assert!(
            matches!(scheduler.task(task).unwrap().vm.globals.get("g:result"), Some(Value::String(value)) if value.contains("notes.txt"))
        );
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].context.script_name.as_deref(),
            Some("plugin.vim")
        );
    }

    #[test]
    fn capability_denial_is_catchable_and_skips_the_host() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut runtime = HostRuntime::new(Arc::new(MockHost {
            requests: requests.clone(),
        }));
        runtime.register_function(
            "read_file",
            Arity::Exact(1),
            vec![Capability::FileSystemRead],
        );
        let mut scheduler = Scheduler::new(10);
        scheduler.set_host(runtime);
        let task = scheduler.spawn(vm("let g:result = 0\ntry\nlet g:ignored = g:read_file('secret')\ncatch\nlet g:result = 1\nendtry\n", HashMap::new())).unwrap();
        assert_eq!(scheduler.run_until_complete(task).unwrap(), Value::Null);
        assert_eq!(
            scheduler.task(task).unwrap().vm.globals.get("g:result"),
            Some(&Value::Integer(1))
        );
        assert!(requests.lock().unwrap().is_empty());
    }

    #[test]
    fn async_errors_resume_through_try_catch() {
        let mut scheduler = Scheduler::new(10);
        let operation = scheduler.register(async {
            Err(RuntimeError::coded(
                "E_IO",
                RuntimeErrorKind::HostError,
                "I/O failed",
            ))
        });
        let globals = HashMap::from([("g:future".into(), Value::Future(operation))]);
        let task = scheduler.spawn(vm("let g:result = 0\ntry\nlet g:ignored = await g:future\ncatch\nlet g:result = 1\nendtry\n", globals)).unwrap();
        assert_eq!(scheduler.run_until_complete(task).unwrap(), Value::Null);
        assert_eq!(
            scheduler.task(task).unwrap().vm.globals.get("g:result"),
            Some(&Value::Integer(1))
        );
    }

    #[test]
    fn instruction_quanta_rotate_ready_tasks() {
        let mut scheduler = Scheduler::new(1);
        let first = scheduler
            .spawn(vm("let g:a = 1 + 2\n", HashMap::new()))
            .unwrap();
        let second = scheduler
            .spawn(vm("let g:b = 3 + 4\n", HashMap::new()))
            .unwrap();
        scheduler.tick().unwrap();
        scheduler.tick().unwrap();
        assert!(matches!(
            scheduler.task(first).unwrap().state,
            TaskState::Ready
        ));
        assert!(matches!(
            scheduler.task(second).unwrap().state,
            TaskState::Ready
        ));
        assert_eq!(scheduler.run_until_complete(first).unwrap(), Value::Null);
        assert_eq!(scheduler.run_until_complete(second).unwrap(), Value::Null);
    }

    #[test]
    fn cancellation_cascades_to_children() {
        let mut scheduler = Scheduler::new(1);
        let operation = scheduler.register(std::future::pending());
        let parent = scheduler
            .spawn(vm(
                "let g:result = await g:future\n",
                HashMap::from([("g:future".into(), Value::Future(operation))]),
            ))
            .unwrap();
        let child = scheduler
            .spawn_child(vm("let g:x = 1\n", HashMap::new()), Some(parent))
            .unwrap();
        assert!(scheduler.cancel(parent));
        assert!(matches!(
            scheduler.task(parent).unwrap().state,
            TaskState::Cancelled
        ));
        assert!(matches!(
            scheduler.task(child).unwrap().state,
            TaskState::Cancelled
        ));
    }
}
