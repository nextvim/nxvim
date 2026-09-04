use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::ast::{BinaryOperator, UnaryOperator};
use crate::bytecode::{
    BytecodeModule, Constant, ConstantId, FunctionPrototype, Instruction, OptionScopeOperand,
};
use crate::host::{
    CommandRequest, HostContext, HostRequest, HostTarget, OptionRequest, OptionRequestOperation,
    OptionRequestScope,
};
use crate::resolver::FunctionId;
use crate::runtime::{BuiltinRegistry, Closure, FunctionRef, OperationId, Value};
use crate::source::Span;

#[derive(Clone, Debug)]
pub struct IteratorState {
    pub values: Vec<Value>,
    pub cursor: usize,
}

#[derive(Clone, Debug)]
pub struct CallFrame {
    pub module: Arc<BytecodeModule>,
    pub function: FunctionId,
    pub instruction_pointer: u32,
    pub stack_base: usize,
    pub locals: Vec<Value>,
    pub closure: Option<Arc<Closure>>,
    pub iterators: Vec<IteratorState>,
}

#[derive(Clone, Debug)]
pub struct ExceptionFrame {
    pub frame_depth: usize,
    pub stack_depth: usize,
    pub handler_ip: u32,
}

#[derive(Clone, Debug)]
pub enum VmStatus {
    Ready,
    Running,
    Suspended { waiting_on: OperationId },
    Completed(Value),
    Failed(RuntimeError),
}

#[derive(Clone, Debug, PartialEq)]
pub enum StepOutcome {
    Continue,
    HostCall(HostRequest),
    OptionCall(OptionRequest),
    CommandCall(CommandRequest),
    Waiting(OperationId),
    Completed(Value),
}

#[derive(Clone, Debug, PartialEq)]
pub enum VmRunOutcome {
    Yielded,
    HostCall(HostRequest),
    OptionCall(OptionRequest),
    CommandCall(CommandRequest),
    Waiting(OperationId),
    Completed(Value),
}

#[derive(Clone, Debug)]
pub struct Vm {
    pub module: Arc<BytecodeModule>,
    pub stack: Vec<Value>,
    pub frames: Vec<CallFrame>,
    pub exceptions: Vec<ExceptionFrame>,
    pub globals: HashMap<String, Value>,
    pub status: VmStatus,
    pub instruction_budget: Option<u64>,
    pub limits: ResourceLimits,
    pub builtins: BuiltinRegistry,
    pub host_context: HostContext,
}

#[derive(Clone, Debug)]
pub struct RuntimeError {
    pub code: Option<String>,
    pub kind: RuntimeErrorKind,
    pub message: String,
    pub span: Option<Span>,
    pub stack_trace: Box<[StackTraceEntry]>,
    pub notes: Box<[String]>,
}

#[derive(Clone, Debug)]
pub enum RuntimeErrorKind {
    TypeError,
    NameError,
    ArityError,
    IndexError,
    KeyError,
    DivisionByZero,
    InvalidCommand,
    PermissionDenied,
    HostError,
    Cancelled,
    ResourceLimit,
    UserThrown(Box<Value>),
    Internal,
}

#[derive(Clone, Debug)]
pub struct StackTraceEntry {
    pub function: Option<String>,
    pub span: Option<Span>,
    pub instruction: u32,
}
pub type RuntimeResult<T> = Result<T, RuntimeError>;

impl RuntimeError {
    pub fn coded(
        code: impl Into<String>,
        kind: RuntimeErrorKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: Some(code.into()),
            kind,
            message: message.into(),
            span: None,
            stack_trace: Box::new([]),
            notes: Box::new([]),
        }
    }
}

impl Vm {
    pub fn new(module: BytecodeModule) -> RuntimeResult<Self> {
        Self::with_globals(module, HashMap::new())
    }

    pub fn with_globals(
        module: BytecodeModule,
        mut globals: HashMap<String, Value>,
    ) -> RuntimeResult<Self> {
        globals
            .entry("v:errors".to_string())
            .or_insert_with(|| Value::List(Vec::new()));
        globals
            .entry("v:exception".to_string())
            .or_insert_with(|| Value::String("".into()));
        globals
            .entry("v:errmsg".to_string())
            .or_insert_with(|| Value::String("".into()));
        globals
            .entry("v:throwpoint".to_string())
            .or_insert_with(|| Value::String("".into()));
        globals
            .entry("v:version".to_string())
            .or_insert_with(|| Value::Integer(900));

        let module = Arc::new(module);
        let entrypoint = module.entrypoint;
        let prototype = module.function(entrypoint).ok_or_else(|| {
            bare_error(RuntimeErrorKind::Internal, "entrypoint function is missing")
        })?;
        let limits = ResourceLimits::default();
        let frame = CallFrame {
            module: module.clone(),
            function: entrypoint,
            instruction_pointer: 0,
            stack_base: 0,
            locals: vec![Value::Null; prototype.local_count as usize],
            closure: None,
            iterators: Vec::new(),
        };
        Ok(Self {
            module,
            stack: Vec::new(),
            frames: vec![frame],
            exceptions: Vec::new(),
            globals,
            status: VmStatus::Ready,
            instruction_budget: limits.max_instructions,
            limits,
            builtins: BuiltinRegistry::with_defaults(),
            host_context: HostContext::default(),
        })
    }

    pub fn run(&mut self) -> RuntimeResult<Value> {
        loop {
            match self.run_quantum(usize::MAX)? {
                VmRunOutcome::Completed(value) => return Ok(value),
                VmRunOutcome::Yielded => continue,
                VmRunOutcome::HostCall(request) => {
                    return Err(self.error(
                        RuntimeErrorKind::HostError,
                        format!(
                            "host call {} requires a scheduler and host runtime",
                            request.function
                        ),
                    ));
                }
                VmRunOutcome::OptionCall(request) => {
                    return Err(self.error(
                        RuntimeErrorKind::HostError,
                        format!(
                            "option access {} requires a scheduler and host runtime",
                            request.name
                        ),
                    ));
                }
                VmRunOutcome::CommandCall(request) => {
                    return Err(self.error(
                        RuntimeErrorKind::HostError,
                        format!(
                            "command {} requires a scheduler and host runtime",
                            request.command.name
                        ),
                    ));
                }
                VmRunOutcome::Waiting(operation) => {
                    return Err(self.error(
                        RuntimeErrorKind::HostError,
                        format!(
                            "VM suspended on operation {} without a scheduler",
                            operation.0
                        ),
                    ));
                }
            }
        }
    }

    pub fn run_quantum(&mut self, quantum: usize) -> RuntimeResult<VmRunOutcome> {
        if let VmStatus::Completed(value) = &self.status {
            return Ok(VmRunOutcome::Completed(value.clone()));
        }
        self.status = VmStatus::Running;
        for _ in 0..quantum {
            match self.step() {
                Ok(StepOutcome::Continue) => {}
                Ok(StepOutcome::HostCall(request)) => {
                    self.status = VmStatus::Ready;
                    return Ok(VmRunOutcome::HostCall(request));
                }
                Ok(StepOutcome::OptionCall(request)) => {
                    self.status = VmStatus::Ready;
                    return Ok(VmRunOutcome::OptionCall(request));
                }
                Ok(StepOutcome::CommandCall(request)) => {
                    self.status = VmStatus::Ready;
                    return Ok(VmRunOutcome::CommandCall(request));
                }
                Ok(StepOutcome::Waiting(operation)) => {
                    self.status = VmStatus::Suspended {
                        waiting_on: operation,
                    };
                    return Ok(VmRunOutcome::Waiting(operation));
                }
                Ok(StepOutcome::Completed(value)) => {
                    self.status = VmStatus::Completed(value.clone());
                    return Ok(VmRunOutcome::Completed(value));
                }
                Err(error) => {
                    if self.dispatch_catchable_error(&error)? {
                        continue;
                    }
                    self.status = VmStatus::Failed(error.clone());
                    return Err(error);
                }
            }
        }
        self.status = VmStatus::Ready;
        Ok(VmRunOutcome::Yielded)
    }

    pub fn suspend_for_operation(&mut self, operation: OperationId) {
        self.status = VmStatus::Suspended {
            waiting_on: operation,
        };
    }

    pub fn complete_command(&mut self, result: RuntimeResult<Value>) -> RuntimeResult<()> {
        match result {
            Ok(value) => self.push(value)?,
            Err(error) if self.dispatch_catchable_error(&error)? => {}
            Err(error) => {
                self.status = VmStatus::Failed(error.clone());
                return Err(error);
            }
        }
        self.status = VmStatus::Ready;
        Ok(())
    }

    pub fn complete_host_call(&mut self, result: RuntimeResult<Value>) -> RuntimeResult<()> {
        match result {
            Ok(value) => self.push(value)?,
            Err(error) if self.dispatch_catchable_error(&error)? => {}
            Err(error) => {
                self.status = VmStatus::Failed(error.clone());
                return Err(error);
            }
        }
        self.status = VmStatus::Ready;
        Ok(())
    }

    pub fn resume_host_call(&mut self, result: RuntimeResult<OperationId>) -> RuntimeResult<()> {
        match result {
            Ok(operation) => self.push(Value::Future(operation))?,
            Err(error) if self.dispatch_catchable_error(&error)? => {}
            Err(error) => {
                self.status = VmStatus::Failed(error.clone());
                return Err(error);
            }
        }
        self.status = VmStatus::Ready;
        Ok(())
    }

    pub fn resume_await(&mut self, result: RuntimeResult<Value>) -> RuntimeResult<()> {
        if !matches!(self.status, VmStatus::Suspended { .. }) {
            return Err(self.error(
                RuntimeErrorKind::Internal,
                "cannot resume a VM that is not suspended",
            ));
        }
        match result {
            Ok(value) => self.push(value)?,
            Err(error) if self.dispatch_catchable_error(&error)? => {}
            Err(error) => {
                self.status = VmStatus::Failed(error.clone());
                return Err(error);
            }
        }
        self.status = VmStatus::Ready;
        Ok(())
    }

    pub fn step(&mut self) -> RuntimeResult<StepOutcome> {
        if let Some(budget) = &mut self.instruction_budget {
            if *budget == 0 {
                return Err(self.error(
                    RuntimeErrorKind::ResourceLimit,
                    "instruction limit exceeded",
                ));
            }
            *budget -= 1;
        }
        let (module, function_id, ip) = {
            let frame = self
                .frames
                .last()
                .ok_or_else(|| bare_error(RuntimeErrorKind::Internal, "VM has no call frame"))?;
            (
                frame.module.clone(),
                frame.function,
                frame.instruction_pointer,
            )
        };
        let instruction = self
            .prototype(&module, function_id)?
            .code
            .get(ip as usize)
            .cloned()
            .ok_or_else(|| {
                self.error(
                    RuntimeErrorKind::Internal,
                    "instruction pointer is out of bounds",
                )
            })?;
        self.frames
            .last_mut()
            .expect("frame exists")
            .instruction_pointer += 1;
        match instruction {
            Instruction::LoadConstant(id) => {
                let value = self.constant_value(&module, function_id, id)?;
                self.push(value)?;
            }
            Instruction::LoadNull => self.push(Value::Null)?,
            Instruction::LoadLocal(slot) => {
                let value = self
                    .frames
                    .last()
                    .and_then(|frame| frame.locals.get(slot as usize))
                    .cloned()
                    .ok_or_else(|| {
                        self.error(RuntimeErrorKind::Internal, "local slot is out of bounds")
                    })?;
                self.push(value)?;
            }
            Instruction::StoreLocal(slot) => {
                let value = self.pop()?;
                let frame = self.frames.last_mut().expect("frame exists");
                let local = frame.locals.get_mut(slot as usize).ok_or_else(|| {
                    bare_error(RuntimeErrorKind::Internal, "local slot is out of bounds")
                })?;
                *local = value;
            }
            Instruction::DeleteLocal(slot) => {
                let frame = self.frames.last_mut().expect("frame exists");
                let local = frame.locals.get_mut(slot as usize).ok_or_else(|| {
                    bare_error(RuntimeErrorKind::Internal, "local slot is out of bounds")
                })?;
                if matches!(local, Value::Null) {
                    return Err(self.error(
                        RuntimeErrorKind::NameError,
                        "cannot unlet an undefined local variable",
                    ));
                }
                *local = Value::Null;
            }
            Instruction::LoadCapture(slot) => {
                let value = self
                    .frames
                    .last()
                    .and_then(|frame| frame.closure.as_ref())
                    .and_then(|closure| closure.captures.get(slot as usize))
                    .cloned()
                    .ok_or_else(|| {
                        self.error(RuntimeErrorKind::Internal, "capture slot is out of bounds")
                    })?;
                self.push(value)?;
            }
            Instruction::StoreCapture(slot) => {
                let _ = slot;
                return Err(self.error(
                    RuntimeErrorKind::Internal,
                    "mutable captures require shared upvalue cells",
                ));
            }
            Instruction::DeleteCapture(slot) => {
                let _ = slot;
                return Err(self.error(
                    RuntimeErrorKind::Internal,
                    "mutable captures require shared upvalue cells",
                ));
            }
            Instruction::LoadGlobal(id) | Instruction::LoadScoped { name: id, .. } => {
                let name = self.constant_string(&module, function_id, id)?;
                let value = if let Some(value) = self.globals.get(&name).cloned() {
                    value
                } else if let Some(value) = name
                    .strip_prefix("g:")
                    .and_then(|clean| self.globals.get(&format!(":{clean}")).cloned())
                {
                    value
                } else if let Some(value) = name
                    .strip_prefix("g:")
                    .and_then(|clean| self.globals.get(clean).cloned())
                {
                    value
                } else if let Some(value) = name
                    .strip_prefix(':')
                    .and_then(|clean| self.globals.get(&format!("g:{clean}")).cloned())
                {
                    value
                } else if name.ends_with(':') {
                    self.namespace_dictionary(&name)
                } else {
                    let builtin_name = name.strip_prefix(':').unwrap_or(&name);
                    if self.builtins.contains(builtin_name) {
                        Value::Builtin(Arc::from(builtin_name))
                    } else {
                        return Err(self.error(
                            RuntimeErrorKind::NameError,
                            format!("undefined runtime variable {name}"),
                        ));
                    }
                };
                self.push(value)?;
            }
            Instruction::StoreGlobal(id) | Instruction::StoreScoped { name: id, .. } => {
                let name = self.constant_string(&module, function_id, id)?;
                let value = self.pop()?;
                self.globals.insert(name, value);
            }
            Instruction::DeleteGlobal(id) | Instruction::DeleteScoped { name: id, .. } => {
                let name = self.constant_string(&module, function_id, id)?;
                if self.globals.remove(&name).is_none() {
                    return Err(self.error(
                        RuntimeErrorKind::NameError,
                        format!("cannot unlet undefined variable {name}"),
                    ));
                }
            }
            Instruction::LoadOption { scope, name } => {
                let name = self.constant_string(&module, function_id, name)?;
                return Ok(StepOutcome::OptionCall(OptionRequest {
                    operation: OptionRequestOperation::Get,
                    name,
                    scope: option_request_scope(scope),
                    context: self.host_context.clone(),
                }));
            }
            Instruction::StoreOption { scope, name } => {
                let name = self.constant_string(&module, function_id, name)?;
                let value = self.pop()?;
                return Ok(StepOutcome::OptionCall(OptionRequest {
                    operation: OptionRequestOperation::Set(value),
                    name,
                    scope: option_request_scope(scope),
                    context: self.host_context.clone(),
                }));
            }
            Instruction::Pop => {
                self.pop()?;
            }
            Instruction::Duplicate => {
                let value = self.stack.last().cloned().ok_or_else(|| {
                    self.error(RuntimeErrorKind::Internal, "operand stack underflow")
                })?;
                self.push(value)?;
            }
            Instruction::Unary(operator) => {
                let value = self.pop()?;
                let result = self.unary(operator, value)?;
                self.push(result)?;
            }
            Instruction::Binary(operator) => {
                let right = self.pop()?;
                let left = self.pop()?;
                let result = self.binary(operator, left, right)?;
                self.push(result)?;
            }
            Instruction::BuildList(count) => {
                let values = self.pop_many(count as usize)?;
                self.push(Value::List(values))?;
            }
            Instruction::BuildDictionary(count) => {
                let values = self.pop_many(count as usize * 2)?;
                let mut dictionary = BTreeMap::new();
                for pair in values.chunks_exact(2) {
                    dictionary.insert(dictionary_key(&pair[0])?, pair[1].clone());
                }
                self.push(Value::Dictionary(dictionary))?;
            }
            Instruction::GetIndex => {
                let index = self.pop()?;
                let target = self.pop()?;
                let value = self.get_index(target, index)?;
                self.push(value)?;
            }
            Instruction::SetIndex => {
                return Err(self.error(
                    RuntimeErrorKind::Internal,
                    "SetIndex is not emitted by the current compiler",
                ));
            }
            Instruction::GetMember(id) => {
                let name = self.constant_string(&module, function_id, id)?;
                let target = self.pop()?;
                let value = self.get_index(target, Value::String(Arc::from(name)))?;
                self.push(value)?;
            }
            Instruction::Call(argc) => {
                if let Some(request) = self.call(argc)? {
                    return Ok(StepOutcome::HostCall(request));
                }
            }
            Instruction::CallNamed { name, argc } => {
                let name = self.constant_string(&module, function_id, name)?;
                let arguments = self.pop_many(argc as usize)?;
                if (self.builtins.contains(&name) && name != "feedkeys" && name != "execute")
                    || name == "exists"
                    || name == "eval"
                    || name.starts_with("assert_")
                {
                    let result = if name == "exists" {
                        self.builtin_exists(&arguments)
                    } else if name == "eval" || name.starts_with("assert_") {
                        self.builtin_assert(&name, &arguments)
                    } else {
                        self.builtins.call(&name, &arguments)
                    }
                    .map_err(|mut error| {
                        error.span = self.current_span();
                        error.stack_trace = self.stack_trace().into_boxed_slice();
                        error
                    })?;
                    self.push(result)?;
                } else {
                    return Ok(StepOutcome::HostCall(HostRequest {
                        target: HostTarget::Global,
                        function: name.to_string(),
                        arguments,
                        context: self.host_context.clone(),
                    }));
                }
            }
            Instruction::Return => {
                let value = self.pop().unwrap_or(Value::Null);
                let frame = self.frames.pop().expect("frame exists");
                self.stack.truncate(frame.stack_base);
                self.exceptions
                    .retain(|exception| exception.frame_depth < self.frames.len());
                if let Some(parent) = self.frames.last_mut() {
                    self.stack.push(value);
                    let _ = parent;
                } else {
                    self.status = VmStatus::Completed(value.clone());
                    return Ok(StepOutcome::Completed(value));
                }
            }
            Instruction::MakeClosure { function, captures } => {
                let values = self.pop_many(captures as usize)?;
                self.push(Value::Closure(Arc::new(Closure {
                    function: FunctionRef {
                        module: module.clone(),
                        function,
                    },
                    captures: values,
                })))?;
            }
            Instruction::Jump(target) | Instruction::Loop(target) => self.jump(target)?,
            Instruction::JumpIfFalse(target) => {
                if !self.pop()?.is_truthy() {
                    self.jump(target)?;
                }
            }
            Instruction::JumpIfTrue(target) => {
                if self.pop()?.is_truthy() {
                    self.jump(target)?;
                }
            }
            Instruction::IterStart => {
                let iterable = self.pop()?;
                let values = match iterable {
                    Value::List(values) => values,
                    Value::Dictionary(values) => values
                        .keys()
                        .cloned()
                        .map(|key| Value::String(Arc::from(key)))
                        .collect(),
                    Value::String(value) => value
                        .chars()
                        .map(|ch| Value::String(Arc::from(ch.to_string())))
                        .collect(),
                    other => {
                        return Err(self.error(
                            RuntimeErrorKind::TypeError,
                            format!("{} is not iterable", other.type_name()),
                        ));
                    }
                };
                self.frames
                    .last_mut()
                    .expect("frame exists")
                    .iterators
                    .push(IteratorState { values, cursor: 0 });
            }
            Instruction::IterNext { end } => {
                let frame = self.frames.last_mut().expect("frame exists");
                let iterator = frame.iterators.last_mut().ok_or_else(|| {
                    bare_error(RuntimeErrorKind::Internal, "iterator stack underflow")
                })?;
                if let Some(value) = iterator.values.get(iterator.cursor).cloned() {
                    iterator.cursor += 1;
                    self.push(value)?;
                } else {
                    self.jump(end)?;
                }
            }
            Instruction::IterEnd => {
                self.frames
                    .last_mut()
                    .expect("frame exists")
                    .iterators
                    .pop()
                    .ok_or_else(|| {
                        self.error(RuntimeErrorKind::Internal, "iterator stack underflow")
                    })?;
            }
            Instruction::TryBegin {
                handler,
                stack_depth,
            } => self.exceptions.push(ExceptionFrame {
                frame_depth: self.frames.len() - 1,
                stack_depth: self.stack.len() + stack_depth as usize,
                handler_ip: handler,
            }),
            Instruction::TryEnd => {
                self.exceptions.pop();
            }
            Instruction::Throw => {
                let thrown = self.pop()?;
                let exc_str = thrown.to_string();
                self.globals.insert(
                    "v:exception".into(),
                    Value::String(Arc::from(exc_str.as_str())),
                );
                self.globals.insert(
                    "v:errmsg".into(),
                    Value::String(Arc::from(exc_str.as_str())),
                );
                if let Some(exception) = self.exceptions.pop() {
                    while self.frames.len() - 1 > exception.frame_depth {
                        self.frames.pop();
                    }
                    self.stack.truncate(exception.stack_depth);
                    self.push(thrown)?;
                    self.frames
                        .last_mut()
                        .expect("handler frame")
                        .instruction_pointer = exception.handler_ip;
                } else {
                    return Err(self.error(
                        RuntimeErrorKind::UserThrown(Box::new(thrown.clone())),
                        format!("uncaught exception: {thrown:?}"),
                    ));
                }
            }
            Instruction::Await => {
                let value = self.pop()?;
                let Value::Future(operation) = value else {
                    return Err(self.error(
                        RuntimeErrorKind::TypeError,
                        format!("await requires a future, got {}", value.type_name()),
                    ));
                };
                return Ok(StepOutcome::Waiting(operation));
            }
            Instruction::ExecuteCommand(id) => {
                let command = self.constant_command(&module, function_id, id)?;
                return Ok(StepOutcome::CommandCall(CommandRequest {
                    command,
                    context: self.host_context.clone(),
                }));
            }
            Instruction::EmitEvent(_) => self.push(Value::Null)?,
        }
        Ok(StepOutcome::Continue)
    }

    fn dispatch_catchable_error(&mut self, error: &RuntimeError) -> RuntimeResult<bool> {
        if matches!(
            error.kind,
            RuntimeErrorKind::Internal
                | RuntimeErrorKind::ResourceLimit
                | RuntimeErrorKind::Cancelled
        ) {
            return Ok(false);
        }
        let Some(exception) = self.exceptions.pop() else {
            return Ok(false);
        };
        while self.frames.len().saturating_sub(1) > exception.frame_depth {
            self.frames.pop();
        }
        let Some(frame) = self.frames.last_mut() else {
            return Ok(false);
        };
        self.stack.truncate(exception.stack_depth);
        let prefix = error
            .code
            .as_deref()
            .map_or(String::new(), |code| format!("{code}: "));
        let exc_str = format!("{prefix}{}", error.message);
        self.globals.insert(
            "v:exception".into(),
            Value::String(Arc::from(exc_str.as_str())),
        );
        self.globals.insert(
            "v:errmsg".into(),
            Value::String(Arc::from(exc_str.as_str())),
        );
        self.stack.push(Value::String(Arc::from(exc_str)));
        frame.instruction_pointer = exception.handler_ip;
        Ok(true)
    }

    fn call(&mut self, argc: u16) -> RuntimeResult<Option<HostRequest>> {
        if self.frames.len() >= self.limits.max_call_depth {
            return Err(self.error(
                RuntimeErrorKind::ResourceLimit,
                "maximum call depth exceeded",
            ));
        }
        let arguments = self.pop_many(argc as usize)?;
        let callee = self.pop()?;
        if let Value::Builtin(name) = &callee {
            if name.as_ref() == "feedkeys" || name.as_ref() == "execute" {
                return Ok(Some(HostRequest {
                    target: HostTarget::Global,
                    function: name.to_string(),
                    arguments,
                    context: self.host_context.clone(),
                }));
            }
            let result = if name.as_ref() == "exists" {
                self.builtin_exists(&arguments)
            } else if name.as_ref() == "eval" || name.starts_with("assert_") {
                self.builtin_assert(name, &arguments)
            } else {
                self.builtins.call(name, &arguments)
            }
            .map_err(|mut error| {
                error.span = self.current_span();
                error.stack_trace = self.stack_trace().into_boxed_slice();
                error
            })?;
            self.push(result)?;
            return Ok(None);
        }
        if let Value::HostFunction(name) = callee {
            return Ok(Some(HostRequest {
                target: HostTarget::Global,
                function: name.to_string(),
                arguments,
                context: self.host_context.clone(),
            }));
        }
        let Value::Closure(closure) = callee else {
            return Err(self.error(
                RuntimeErrorKind::TypeError,
                format!("{} is not callable", callee.type_name()),
            ));
        };
        let function = closure.function.clone();
        let prototype = self.prototype(&function.module, function.function)?;
        let required = prototype
            .arity
            .saturating_sub(prototype.optional_parameters);
        if argc < required || (!prototype.variadic && argc > prototype.arity) {
            return Err(self.error(
                RuntimeErrorKind::ArityError,
                format!(
                    "expected {required}..={} arguments, got {argc}",
                    prototype.arity
                ),
            ));
        }
        let mut locals = vec![Value::Null; prototype.local_count as usize];
        for (index, argument) in arguments.into_iter().enumerate() {
            if let Some(local) = locals.get_mut(index) {
                *local = argument;
            }
        }
        self.frames.push(CallFrame {
            module: function.module,
            function: function.function,
            instruction_pointer: 0,
            stack_base: self.stack.len(),
            locals,
            closure: Some(closure),
            iterators: Vec::new(),
        });
        Ok(None)
    }

    fn builtin_exists(&self, arguments: &[Value]) -> RuntimeResult<Value> {
        let [Value::String(expression)] = arguments else {
            return Err(RuntimeError::coded(
                "E730",
                RuntimeErrorKind::TypeError,
                "exists() requires one String argument",
            ));
        };
        let expression = expression.as_ref();
        let exists = if let Some(function) = expression.strip_prefix('*') {
            self.builtins.contains(function)
                || matches!(
                    self.globals.get(&format!(":{function}")),
                    Some(Value::Closure(_) | Value::Builtin(_) | Value::HostFunction(_))
                )
                || matches!(
                    self.globals.get(&format!("g:{function}")),
                    Some(Value::Closure(_) | Value::HostFunction(_))
                )
        } else if let Some(environment) = expression.strip_prefix('$') {
            std::env::var_os(environment).is_some()
        } else if let Some(name) = expression.strip_prefix("s:") {
            let source = self
                .frames
                .last()
                .map_or(self.module.source, |frame| frame.module.source);
            self.globals.contains_key(&format!("s{}:{name}", source.0))
        } else if expression.starts_with('&')
            || expression.starts_with(':')
            || expression.starts_with('+')
        {
            false
        } else {
            let runtime_name = if expression.contains(':') {
                expression.to_owned()
            } else {
                format!(":{expression}")
            };
            self.globals.contains_key(&runtime_name)
        };
        Ok(Value::Integer(i64::from(exists)))
    }

    pub fn add_v_error(&mut self, err: String) {
        if let Some(Value::List(list)) = self.globals.get_mut("v:errors") {
            list.push(Value::String(Arc::from(err)));
        } else {
            self.globals.insert(
                "v:errors".to_string(),
                Value::List(vec![Value::String(Arc::from(err))]),
            );
        }
    }

    fn builtin_assert(&mut self, name: &str, args: &[Value]) -> RuntimeResult<Value> {
        let is_truthy = |val: &Value| -> bool {
            match val {
                Value::Bool(b) => *b,
                Value::Integer(i) => *i != 0,
                Value::Float(f) => *f != 0.0,
                Value::String(s) => !s.is_empty(),
                Value::List(l) => !l.is_empty(),
                Value::Dictionary(d) => !d.is_empty(),
                Value::Null => false,
                _ => true,
            }
        };

        let msg_prefix = |args: &[Value], idx: usize| -> String {
            if args.len() > idx {
                format!("{}: ", args[idx])
            } else {
                String::new()
            }
        };

        match name {
            "assert_equal" => {
                if args.len() < 2 {
                    return Err(self.error(
                        RuntimeErrorKind::ArityError,
                        "assert_equal expects at least 2 arguments",
                    ));
                }
                let expected = &args[0];
                let actual = &args[1];
                if expected == actual {
                    Ok(Value::Integer(0))
                } else {
                    let prefix = msg_prefix(args, 2);
                    self.add_v_error(format!("{prefix}Expected {expected} but got {actual}"));
                    Ok(Value::Integer(1))
                }
            }
            "assert_notequal" => {
                if args.len() < 2 {
                    return Err(self.error(
                        RuntimeErrorKind::ArityError,
                        "assert_notequal expects at least 2 arguments",
                    ));
                }
                let expected = &args[0];
                let actual = &args[1];
                if expected != actual {
                    Ok(Value::Integer(0))
                } else {
                    let prefix = msg_prefix(args, 2);
                    self.add_v_error(format!("{prefix}Expected not equal to {expected}"));
                    Ok(Value::Integer(1))
                }
            }
            "assert_true" => {
                if args.is_empty() {
                    return Err(self.error(
                        RuntimeErrorKind::ArityError,
                        "assert_true expects at least 1 argument",
                    ));
                }
                let actual = &args[0];
                if is_truthy(actual) {
                    Ok(Value::Integer(0))
                } else {
                    let prefix = msg_prefix(args, 1);
                    self.add_v_error(format!("{prefix}Expected True but got {actual}"));
                    Ok(Value::Integer(1))
                }
            }
            "assert_false" => {
                if args.is_empty() {
                    return Err(self.error(
                        RuntimeErrorKind::ArityError,
                        "assert_false expects at least 1 argument",
                    ));
                }
                let actual = &args[0];
                if !is_truthy(actual) {
                    Ok(Value::Integer(0))
                } else {
                    let prefix = msg_prefix(args, 1);
                    self.add_v_error(format!("{prefix}Expected False but got {actual}"));
                    Ok(Value::Integer(1))
                }
            }
            "assert_inrange" => {
                if args.len() < 3 {
                    return Err(self.error(
                        RuntimeErrorKind::ArityError,
                        "assert_inrange expects at least 3 arguments",
                    ));
                }
                let lower = match &args[0] {
                    Value::Integer(i) => *i,
                    Value::Float(f) => *f as i64,
                    _ => 0,
                };
                let upper = match &args[1] {
                    Value::Integer(i) => *i,
                    Value::Float(f) => *f as i64,
                    _ => 0,
                };
                let actual = match &args[2] {
                    Value::Integer(i) => *i,
                    Value::Float(f) => *f as i64,
                    _ => 0,
                };
                if actual >= lower && actual <= upper {
                    Ok(Value::Integer(0))
                } else {
                    let prefix = msg_prefix(args, 3);
                    self.add_v_error(format!(
                        "{prefix}Expected range [{lower}, {upper}] but got {actual}"
                    ));
                    Ok(Value::Integer(1))
                }
            }
            "assert_match" => {
                if args.len() < 2 {
                    return Err(self.error(
                        RuntimeErrorKind::ArityError,
                        "assert_match expects at least 2 arguments",
                    ));
                }
                let pat_str = args[0].to_string();
                let act_str = args[1].to_string();
                let matched = if let Ok(regex) = vim_regex::Regex::compile(
                    &pat_str,
                    vim_regex::CompileOptions::default(),
                ) {
                    regex.find(&act_str).ok().flatten().is_some()
                } else {
                    act_str.contains(&pat_str)
                };
                if matched {
                    Ok(Value::Integer(0))
                } else {
                    let prefix = msg_prefix(args, 2);
                    self.add_v_error(format!(
                        "{prefix}Pattern '{pat_str}' does not match '{act_str}'"
                    ));
                    Ok(Value::Integer(1))
                }
            }
            "assert_report" => {
                if args.is_empty() {
                    return Err(self.error(
                        RuntimeErrorKind::ArityError,
                        "assert_report expects 1 argument",
                    ));
                }
                let msg = args[0].to_string();
                self.add_v_error(msg);
                Ok(Value::Integer(1))
            }
            "assert_fails" => {
                if args.is_empty() {
                    return Err(self.error(
                        RuntimeErrorKind::ArityError,
                        "assert_fails expects at least 1 argument",
                    ));
                }
                let cmd_str = match &args[0] {
                    Value::String(s) => s.to_string(),
                    other => other.to_string(),
                };
                let expected_err = if args.len() > 1 {
                    match &args[1] {
                        Value::String(s) => Some(s.to_string()),
                        Value::List(l) => {
                            if l.is_empty() {
                                None
                            } else {
                                Some(l[0].to_string())
                            }
                        }
                        other => Some(other.to_string()),
                    }
                } else {
                    None
                };

                let run_res = self.execute_sub_script(&cmd_str);
                match run_res {
                    Err(err) => {
                        let err_msg = err.message.clone();
                        if let Some(exp) = expected_err {
                            if err_msg.contains(&exp) {
                                Ok(Value::Integer(0))
                            } else {
                                let prefix = msg_prefix(args, 2);
                                self.add_v_error(format!(
                                    "{prefix}Expected error containing '{exp}' but got '{err_msg}'"
                                ));
                                Ok(Value::Integer(1))
                            }
                        } else {
                            Ok(Value::Integer(0))
                        }
                    }
                    Ok(_) => {
                        let prefix = msg_prefix(args, 2);
                        self.add_v_error(format!("{prefix}Expected command to fail: {cmd_str}"));
                        Ok(Value::Integer(1))
                    }
                }
            }
            "eval" => self.builtin_eval(args),
            "execute" => self.builtin_execute(args),
            _ => Err(self.error(
                RuntimeErrorKind::NameError,
                format!("unknown assert function: {name}"),
            )),
        }
    }

    fn builtin_execute(&mut self, args: &[Value]) -> RuntimeResult<Value> {
        if args.is_empty() {
            return Err(self.error(
                RuntimeErrorKind::ArityError,
                "execute expects at least 1 argument",
            ));
        }
        let cmd_str = match &args[0] {
            Value::String(s) => s.to_string(),
            Value::List(l) => {
                let mut lines = Vec::new();
                for item in l.iter() {
                    lines.push(item.to_string());
                }
                lines.join("\n")
            }
            other => other.to_string(),
        };
        let _ = self.execute_sub_script(&cmd_str);
        Ok(Value::String(Arc::from("")))
    }

    fn builtin_eval(&mut self, args: &[Value]) -> RuntimeResult<Value> {
        if args.is_empty() {
            return Err(self.error(
                RuntimeErrorKind::ArityError,
                "eval expects 1 argument",
            ));
        }
        let expr_str = args[0].to_string();
        let eval_code = format!("let g:__eval_result = ({expr_str})");
        self.execute_sub_script(&eval_code)?;
        Ok(self
            .globals
            .get("g:__eval_result")
            .cloned()
            .unwrap_or(Value::Null))
    }

    fn execute_sub_script(&mut self, code: &str) -> RuntimeResult<Value> {
        use crate::compiler::Compiler;
        use crate::lexer::Lexer;
        use crate::parser::Parser;
        use crate::resolver::{Resolver, ResolverConfig};
        use crate::source::SourceId;

        let lexed = Lexer::new(SourceId(0), code).lex();
        if !lexed.diagnostics.is_empty() {
            return Err(self.error(
                RuntimeErrorKind::InvalidCommand,
                format!("lexer error parsing '{code}'"),
            ));
        }
        let parsed = Parser::new_with_source(&lexed.tokens, code).parse();
        if !parsed.diagnostics.is_empty() || parsed.program.is_none() {
            return Err(self.error(
                RuntimeErrorKind::InvalidCommand,
                format!("syntax error parsing '{code}'"),
            ));
        }
        let mut config = ResolverConfig::default();
        config.unqualified_is_global = true;
        let resolved = Resolver::new(config).resolve(parsed.program.unwrap());
        if !resolved.diagnostics.is_empty() || resolved.program.is_none() {
            return Err(self.error(
                RuntimeErrorKind::InvalidCommand,
                format!("resolver error resolving '{code}'"),
            ));
        }
        let compiled = Compiler::new(&resolved.program.unwrap()).compile();
        if !compiled.diagnostics.is_empty() || compiled.module.is_none() {
            return Err(self.error(
                RuntimeErrorKind::InvalidCommand,
                format!("compiler error compiling '{code}'"),
            ));
        }
        let mut sub_vm = Vm::with_globals(compiled.module.unwrap(), self.globals.clone())?;
        sub_vm.builtins = self.builtins.clone();
        sub_vm.host_context = self.host_context.clone();
        let res = sub_vm.run();
        self.globals = sub_vm.globals;
        res
    }

    fn namespace_dictionary(&self, prefix: &str) -> Value {
        let values = self
            .globals
            .iter()
            .filter_map(|(name, value)| {
                name.strip_prefix(prefix)
                    .filter(|key| !key.is_empty())
                    .map(|key| (key.to_owned(), value.clone()))
            })
            .collect::<BTreeMap<_, _>>();
        Value::Dictionary(values)
    }

    fn unary(&self, operator: UnaryOperator, value: Value) -> RuntimeResult<Value> {
        match operator {
            UnaryOperator::Not => Ok(Value::Bool(!value.is_truthy())),
            UnaryOperator::Positive => match value {
                Value::Integer(_) | Value::Float(_) => Ok(value),
                other => Err(self.error(
                    RuntimeErrorKind::TypeError,
                    format!("unary + requires a number, got {}", other.type_name()),
                )),
            },
            UnaryOperator::Negate => match value {
                Value::Integer(value) => value
                    .checked_neg()
                    .map(Value::Integer)
                    .ok_or_else(|| self.error(RuntimeErrorKind::TypeError, "integer overflow")),
                Value::Float(value) => Ok(Value::Float(-value)),
                other => Err(self.error(
                    RuntimeErrorKind::TypeError,
                    format!("unary - requires a number, got {}", other.type_name()),
                )),
            },
        }
    }

    fn binary(&self, operator: BinaryOperator, left: Value, right: Value) -> RuntimeResult<Value> {
        use BinaryOperator as B;
        match operator {
            B::Add => numeric(left, right, |a, b| a.checked_add(b), |a, b| a + b, self),
            B::Subtract => numeric(left, right, |a, b| a.checked_sub(b), |a, b| a - b, self),
            B::Multiply => numeric(left, right, |a, b| a.checked_mul(b), |a, b| a * b, self),
            B::Divide => {
                if matches!(right, Value::Integer(0))
                    || matches!(right, Value::Float(value) if value == 0.0)
                {
                    return Err(self.error(RuntimeErrorKind::DivisionByZero, "division by zero"));
                }
                numeric(left, right, |a, b| a.checked_div(b), |a, b| a / b, self)
            }
            B::Remainder => {
                let (Value::Integer(left), Value::Integer(right)) = (left, right) else {
                    return Err(
                        self.error(RuntimeErrorKind::TypeError, "remainder requires integers")
                    );
                };
                if right == 0 {
                    return Err(self.error(RuntimeErrorKind::DivisionByZero, "division by zero"));
                }
                Ok(Value::Integer(left % right))
            }
            B::Concatenate => Ok(Value::String(Arc::from(format!(
                "{}{}",
                display_value(&left),
                display_value(&right)
            )))),
            B::Equal => Ok(Value::Bool(left == right)),
            B::NotEqual => Ok(Value::Bool(left != right)),
            B::Is => Ok(Value::Bool(identity(&left, &right))),
            B::IsNot => Ok(Value::Bool(!identity(&left, &right))),
            B::Less | B::LessEqual | B::Greater | B::GreaterEqual => {
                compare(operator, left, right, self)
            }
            B::LogicalAnd => Ok(Value::Bool(left.is_truthy() && right.is_truthy())),
            B::LogicalOr => Ok(Value::Bool(left.is_truthy() || right.is_truthy())),
            B::Coalesce => Ok(if matches!(left, Value::Null) {
                right
            } else {
                left
            }),
            B::Match | B::NoMatch => Err(self.error(
                RuntimeErrorKind::TypeError,
                "pattern matching requires the regex subsystem",
            )),
        }
    }

    fn get_index(&self, target: Value, index: Value) -> RuntimeResult<Value> {
        match (target, index) {
            (Value::List(values), Value::Integer(index)) => normalize_index(index, values.len())
                .and_then(|index| values.get(index).cloned())
                .ok_or_else(|| self.error(RuntimeErrorKind::IndexError, "list index out of range")),
            (Value::String(value), Value::Integer(index)) => {
                let chars: Vec<_> = value.chars().collect();
                normalize_index(index, chars.len())
                    .and_then(|index| chars.get(index))
                    .map(|value| Value::String(Arc::from(value.to_string())))
                    .ok_or_else(|| {
                        self.error(RuntimeErrorKind::IndexError, "string index out of range")
                    })
            }
            (Value::Dictionary(values), key) => {
                let key = dictionary_key(&key)?;
                values.get(&key).cloned().ok_or_else(|| {
                    self.error(
                        RuntimeErrorKind::KeyError,
                        format!("dictionary has no key {key:?}"),
                    )
                })
            }
            (target, _) => Err(self.error(
                RuntimeErrorKind::TypeError,
                format!("{} cannot be indexed", target.type_name()),
            )),
        }
    }
    fn jump(&mut self, target: u32) -> RuntimeResult<()> {
        let frame = self.frames.last().expect("frame");
        let code_len = self.prototype(&frame.module, frame.function)?.code.len();
        if target as usize > code_len {
            return Err(self.error(RuntimeErrorKind::Internal, "jump target is out of bounds"));
        }
        self.frames.last_mut().expect("frame").instruction_pointer = target;
        Ok(())
    }
    fn constant_value(
        &self,
        module: &Arc<BytecodeModule>,
        function: FunctionId,
        id: ConstantId,
    ) -> RuntimeResult<Value> {
        match self
            .prototype(module, function)?
            .constants
            .get(id.0 as usize)
            .ok_or_else(|| {
                self.error(
                    RuntimeErrorKind::Internal,
                    "constant index is out of bounds",
                )
            })? {
            Constant::Null => Ok(Value::Null),
            Constant::Bool(value) => Ok(Value::Bool(*value)),
            Constant::Integer(value) => Ok(Value::Integer(*value)),
            Constant::Float(value) => Ok(Value::Float(*value)),
            Constant::String(value) => Ok(Value::String(Arc::from(value.as_str()))),
            Constant::Blob(value) => Ok(Value::Blob(Arc::from(value.as_slice()))),
            Constant::Function(function) => Ok(Value::Closure(Arc::new(Closure {
                function: FunctionRef {
                    module: module.clone(),
                    function: *function,
                },
                captures: Vec::new(),
            }))),
            Constant::Command(_) => Err(self.error(
                RuntimeErrorKind::Internal,
                "command constant used as a value",
            )),
        }
    }
    fn constant_command(
        &self,
        module: &BytecodeModule,
        function: FunctionId,
        id: ConstantId,
    ) -> RuntimeResult<crate::ast::ExCommand> {
        match self
            .prototype(module, function)?
            .constants
            .get(id.0 as usize)
        {
            Some(Constant::Command(command)) => Ok((**command).clone()),
            _ => Err(self.error(RuntimeErrorKind::Internal, "constant is not a command")),
        }
    }

    fn constant_string(
        &self,
        module: &BytecodeModule,
        function: FunctionId,
        id: ConstantId,
    ) -> RuntimeResult<String> {
        match self
            .prototype(module, function)?
            .constants
            .get(id.0 as usize)
        {
            Some(Constant::String(value)) => Ok(value.clone()),
            _ => Err(self.error(RuntimeErrorKind::Internal, "constant is not a string")),
        }
    }
    fn prototype<'a>(
        &self,
        module: &'a BytecodeModule,
        function: FunctionId,
    ) -> RuntimeResult<&'a FunctionPrototype> {
        module.function(function).ok_or_else(|| {
            self.error(
                RuntimeErrorKind::Internal,
                format!("function {} is missing", function.0),
            )
        })
    }
    fn pop(&mut self) -> RuntimeResult<Value> {
        self.stack
            .pop()
            .ok_or_else(|| self.error(RuntimeErrorKind::Internal, "operand stack underflow"))
    }
    fn pop_many(&mut self, count: usize) -> RuntimeResult<Vec<Value>> {
        if count > self.stack.len() {
            return Err(self.error(RuntimeErrorKind::Internal, "operand stack underflow"));
        }
        Ok(self.stack.split_off(self.stack.len() - count))
    }
    fn push(&mut self, value: Value) -> RuntimeResult<()> {
        if self.stack.len() >= self.limits.max_stack_size {
            return Err(self.error(
                RuntimeErrorKind::ResourceLimit,
                "maximum operand stack size exceeded",
            ));
        }
        self.stack.push(value);
        Ok(())
    }
    fn error(&self, kind: RuntimeErrorKind, message: impl Into<String>) -> RuntimeError {
        let span = self.current_span();
        RuntimeError {
            code: runtime_code(&kind).map(str::to_owned),
            kind,
            message: message.into(),
            span,
            stack_trace: self.stack_trace().into_boxed_slice(),
            notes: Box::new([]),
        }
    }
    fn current_span(&self) -> Option<Span> {
        let frame = self.frames.last()?;
        let prototype = frame.module.function(frame.function)?;
        let ip = frame.instruction_pointer.saturating_sub(1);
        prototype
            .spans
            .iter()
            .rev()
            .find(|(offset, _)| *offset <= ip)
            .map(|(_, span)| *span)
    }
    fn stack_trace(&self) -> Vec<StackTraceEntry> {
        self.frames
            .iter()
            .rev()
            .map(|frame| {
                let prototype = frame.module.function(frame.function);
                StackTraceEntry {
                    function: prototype.and_then(|value| value.name.clone()),
                    span: prototype.and_then(|value| {
                        value
                            .spans
                            .iter()
                            .rev()
                            .find(|(offset, _)| {
                                *offset <= frame.instruction_pointer.saturating_sub(1)
                            })
                            .map(|(_, span)| *span)
                    }),
                    instruction: frame.instruction_pointer.saturating_sub(1),
                }
            })
            .collect()
    }
}

fn numeric(
    vm_left: Value,
    vm_right: Value,
    integer: impl FnOnce(i64, i64) -> Option<i64>,
    float: impl FnOnce(f64, f64) -> f64,
    vm: &Vm,
) -> RuntimeResult<Value> {
    match (vm_left, vm_right) {
        (Value::Integer(left), Value::Integer(right)) => integer(left, right)
            .map(Value::Integer)
            .ok_or_else(|| vm.error(RuntimeErrorKind::TypeError, "integer overflow")),
        (Value::Integer(left), Value::Float(right)) => Ok(Value::Float(float(left as f64, right))),
        (Value::Float(left), Value::Integer(right)) => Ok(Value::Float(float(left, right as f64))),
        (Value::Float(left), Value::Float(right)) => Ok(Value::Float(float(left, right))),
        (left, right) => Err(vm.error(
            RuntimeErrorKind::TypeError,
            format!(
                "numeric operator cannot use {} and {}",
                left.type_name(),
                right.type_name()
            ),
        )),
    }
}
fn compare(operator: BinaryOperator, left: Value, right: Value, vm: &Vm) -> RuntimeResult<Value> {
    let ordering = match (&left, &right) {
        (Value::Integer(left), Value::Integer(right)) => left.partial_cmp(right),
        (Value::Float(left), Value::Float(right)) => left.partial_cmp(right),
        (Value::Integer(left), Value::Float(right)) => (*left as f64).partial_cmp(right),
        (Value::Float(left), Value::Integer(right)) => left.partial_cmp(&(*right as f64)),
        (Value::String(left), Value::String(right)) => left.partial_cmp(right),
        _ => {
            return Err(vm.error(
                RuntimeErrorKind::TypeError,
                format!(
                    "cannot compare {} and {}",
                    left.type_name(),
                    right.type_name()
                ),
            ));
        }
    };
    let Some(ordering) = ordering else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(match operator {
        BinaryOperator::Less => ordering.is_lt(),
        BinaryOperator::LessEqual => ordering.is_le(),
        BinaryOperator::Greater => ordering.is_gt(),
        BinaryOperator::GreaterEqual => ordering.is_ge(),
        _ => false,
    }))
}
fn dictionary_key(value: &Value) -> RuntimeResult<String> {
    match value {
        Value::String(value) => Ok(value.to_string()),
        Value::Integer(value) => Ok(value.to_string()),
        other => Err(bare_error(
            RuntimeErrorKind::TypeError,
            format!(
                "dictionary key must be string or number, got {}",
                other.type_name()
            ),
        )),
    }
}
fn normalize_index(index: i64, len: usize) -> Option<usize> {
    let normalized = if index < 0 { len as i64 + index } else { index };
    (normalized >= 0 && normalized < len as i64).then_some(normalized as usize)
}
fn identity(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Closure(left), Value::Closure(right)) => Arc::ptr_eq(left, right),
        (Value::Builtin(left), Value::Builtin(right)) => left == right,
        (Value::HostFunction(left), Value::HostFunction(right)) => left == right,
        (Value::HostObject(left), Value::HostObject(right)) => left == right,
        _ => left == right,
    }
}
fn display_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => i32::from(*value).to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) => value.to_string(),
        Value::String(value) => value.to_string(),
        other => format!("{other:?}"),
    }
}
fn runtime_code(kind: &RuntimeErrorKind) -> Option<&'static str> {
    match kind {
        RuntimeErrorKind::TypeError => Some("E745"),
        RuntimeErrorKind::NameError => Some("E121"),
        RuntimeErrorKind::ArityError => Some("E119"),
        RuntimeErrorKind::IndexError => Some("E684"),
        RuntimeErrorKind::KeyError => Some("E716"),
        RuntimeErrorKind::DivisionByZero => Some("E805"),
        RuntimeErrorKind::ResourceLimit => Some("E1240"),
        RuntimeErrorKind::InvalidCommand => Some("E492"),
        _ => None,
    }
}

fn option_request_scope(scope: OptionScopeOperand) -> OptionRequestScope {
    match scope {
        OptionScopeOperand::Unqualified => OptionRequestScope::Unqualified,
        OptionScopeOperand::Local => OptionRequestScope::Local,
        OptionScopeOperand::Global => OptionRequestScope::Global,
    }
}

fn bare_error(kind: RuntimeErrorKind, message: impl Into<String>) -> RuntimeError {
    RuntimeError {
        code: runtime_code(&kind).map(str::to_owned),
        kind,
        message: message.into(),
        span: None,
        stack_trace: Box::new([]),
        notes: Box::new([]),
    }
}

#[derive(Clone, Debug)]
pub struct ResourceLimits {
    pub max_instructions: Option<u64>,
    pub max_call_depth: usize,
    pub max_stack_size: usize,
    pub max_collection_size: usize,
    pub max_tasks: usize,
}
impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_instructions: Some(10_000_000),
            max_call_depth: 256,
            max_stack_size: 65_536,
            max_collection_size: 1_000_000,
            max_tasks: 1_024,
        }
    }
}
#[derive(Clone, Debug)]
pub struct LoadedFunction<'a> {
    pub prototype: &'a FunctionPrototype,
    pub module: &'a BytecodeModule,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::Compiler;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::resolver::{Resolver, ResolverConfig};
    use crate::source::SourceId;

    fn run(source: &str) -> RuntimeResult<Vm> {
        let lexed = Lexer::new(SourceId(0), source).lex();
        assert!(
            lexed.diagnostics.is_empty(),
            "lexer: {:?}",
            lexed.diagnostics
        );
        let parsed = Parser::new(&lexed.tokens).parse();
        assert!(
            parsed.diagnostics.is_empty(),
            "parser: {:?}",
            parsed.diagnostics
        );
        let resolved = Resolver::new(ResolverConfig::default()).resolve(parsed.program.unwrap());
        assert!(
            resolved.diagnostics.is_empty(),
            "resolver: {:?}",
            resolved.diagnostics
        );
        let compiled = Compiler::new(&resolved.program.unwrap()).compile();
        assert!(
            compiled.diagnostics.is_empty(),
            "compiler: {:?}",
            compiled.diagnostics
        );
        let mut vm = Vm::new(compiled.module.unwrap())?;
        vm.run()?;
        Ok(vm)
    }

    #[test]
    fn executes_arithmetic_conditionals_and_loops() {
        let vm = run("let x = 1\nwhile x < 5\nlet x = x + 1\nendwhile\nif x == 5\nlet result = x * 2\nelse\nlet result = 0\nendif\n").unwrap();
        assert_eq!(vm.globals.get(":result"), Some(&Value::Integer(10)));
    }

    #[test]
    fn unlet_removes_multiple_variables() {
        let vm = run("let first = 1\nlet second = 2\nunlet first second\n").unwrap();
        assert!(!vm.globals.contains_key(":first"));
        assert!(!vm.globals.contains_key(":second"));
    }

    #[test]
    fn unlet_reports_an_already_deleted_variable() {
        let error = run("let value = 1\nunlet value\nunlet value\n").unwrap_err();
        assert!(matches!(error.kind, RuntimeErrorKind::NameError));
        assert!(error.message.contains("cannot unlet undefined variable"));
    }

    #[test]
    fn executes_user_functions() {
        let vm = run("function s:Add(left, right)\nreturn left + right\nendfunction\nlet result = s:Add(20, 22)\n").unwrap();
        assert_eq!(vm.globals.get(":result"), Some(&Value::Integer(42)));
    }

    #[test]
    fn executes_builtin_functions_through_the_vm() {
        let vm = run("let result = join(range(1, 3), ',')\n").unwrap();
        assert_eq!(
            vm.globals.get(":result"),
            Some(&Value::String(Arc::from("1,2,3")))
        );
    }

    #[test]
    fn catches_builtin_errors_as_language_exceptions() {
        let vm = run("let result = 0\ntry\nlet ignored = len(1)\ncatch\nlet result = 1\nendtry\n")
            .unwrap();
        assert_eq!(vm.globals.get(":result"), Some(&Value::Integer(1)));
    }

    #[test]
    fn executes_for_loops_and_collection_indexing() {
        let vm = run("let total = 0\nfor item in [2, 3, 4]\nlet total = total + item\nendfor\nlet result = {'sum': total}.sum\n").unwrap();
        assert_eq!(vm.globals.get(":result"), Some(&Value::Integer(9)));
    }

    #[test]
    fn catches_thrown_values() {
        let vm = run("let result = 0\ntry\nthrow 'failure'\ncatch 'failure'\nlet result = 7\nfinally\nlet result = result + 1\nendtry\n").unwrap();
        assert_eq!(vm.globals.get(":result"), Some(&Value::Integer(8)));
    }

    #[test]
    fn reports_runtime_type_errors_with_a_stack_trace() {
        let error = run("function s:Bad()\nreturn 1 + 'x'\nendfunction\nlet result = s:Bad()\n")
            .unwrap_err();
        assert!(matches!(error.kind, RuntimeErrorKind::TypeError));
        assert!(!error.stack_trace.is_empty());
        assert!(error.span.is_some());
    }

    #[test]
    fn echo_requires_scheduler_at_runtime() {
        let error = run("echo 'hello'\n").unwrap_err();
        assert!(matches!(error.kind, RuntimeErrorKind::HostError));
        assert!(
            error
                .message
                .contains("requires a scheduler and host runtime")
        );
    }

    #[test]
    fn test_assertion_builtins_and_v_errors() {
        let script = r#"
call assert_equal(1, 1)
call assert_equal(1, 2, 'custom msg')
call assert_true(1)
call assert_true(0, 'should fail')
call assert_false(0)
call assert_false(1, 'should fail false')
call assert_inrange(1, 10, 5)
call assert_inrange(1, 10, 20)
call assert_match('^hello', 'hello world')
call assert_match('^xyz', 'hello world')
call assert_report('manual report')
"#;
        let vm = run(script).unwrap();
        let errors = vm.globals.get("v:errors").unwrap();
        if let Value::List(list) = errors {
            assert_eq!(list.len(), 6);
            assert!(list[0].to_string().contains("custom msg"));
            assert!(list[1].to_string().contains("should fail"));
            assert!(list[2].to_string().contains("should fail false"));
            assert!(list[3].to_string().contains("Expected range [1, 10] but got 20"));
            assert!(list[4].to_string().contains("Pattern '^xyz' does not match"));
            assert_eq!(list[5].to_string(), "manual report");
        } else {
            panic!("v:errors is not a list");
        }
    }

    #[test]
    fn test_v_exception_and_v_errmsg() {
        let script = r#"
let caught = ''
try
  throw 'CustomException'
catch
  let caught = v:exception
endtry
"#;
        let vm = run(script).unwrap();
        assert_eq!(
            vm.globals.get(":caught"),
            Some(&Value::String(Arc::from("CustomException")))
        );
    }
}
