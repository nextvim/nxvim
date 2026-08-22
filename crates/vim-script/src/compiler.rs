use std::collections::HashSet;

use crate::ast::*;
use crate::bytecode::{
    BytecodeModule, Constant, ConstantId, ExceptionHandler, FunctionPrototype, Instruction,
    OptionScopeOperand,
};
use crate::resolver::{FunctionId, ResolvedFunction, ResolvedProgram, ScopeId, Symbol, SymbolId};
use crate::source::{Diagnostic, Span};

#[derive(Clone, Debug)]
pub struct Compiler<'a> {
    pub program: &'a ResolvedProgram,
    pub functions: Vec<FunctionBuilder>,
    pub current_function: usize,
    pub diagnostics: Vec<Diagnostic>,
    compiled: HashSet<FunctionId>,
    entrypoint: FunctionId,
}

#[derive(Clone, Debug)]
pub struct FunctionBuilder {
    pub id: FunctionId,
    pub name: Option<String>,
    pub arity: u16,
    pub optional_parameters: u16,
    pub variadic: bool,
    pub local_count: u32,
    pub capture_count: u16,
    pub asynchronous: bool,
    pub code: Vec<Instruction>,
    pub constants: Vec<Constant>,
    pub spans: Vec<(u32, Span)>,
    pub loops: Vec<LoopContext>,
    pub exception_handlers: Vec<ExceptionContext>,
    pub handlers: Vec<ExceptionHandler>,
}

#[derive(Clone, Debug)]
pub struct LoopContext {
    pub start_offset: u32,
    pub break_jumps: Vec<u32>,
    pub continue_target: u32,
}
#[derive(Clone, Debug)]
pub struct ExceptionContext {
    pub try_start: u32,
    pub catch_jump: Option<u32>,
    pub finally_jump: Option<u32>,
}
#[derive(Clone, Debug)]
pub struct CompileOutput {
    pub module: Option<BytecodeModule>,
    pub diagnostics: Vec<Diagnostic>,
}

impl<'a> Compiler<'a> {
    pub fn new(program: &'a ResolvedProgram) -> Self {
        let entrypoint = FunctionId(
            program
                .functions
                .iter()
                .map(|function| function.id.0)
                .max()
                .map_or(0, |id| id + 1),
        );
        let mut functions: Vec<_> = program
            .functions
            .iter()
            .map(|function| {
                FunctionBuilder::new(
                    function.id,
                    function.local_count,
                    function.captures.len() as u16,
                )
            })
            .collect();
        functions.push(FunctionBuilder::new(
            entrypoint,
            script_local_count(program),
            0,
        ));
        let current_function = functions.len() - 1;
        Self {
            program,
            functions,
            current_function,
            diagnostics: Vec::new(),
            compiled: HashSet::new(),
            entrypoint,
        }
    }

    pub fn compile(mut self) -> CompileOutput {
        for statement in &self.program.program.statements {
            self.compile_stmt(statement);
        }
        self.emit(Instruction::LoadNull, Span::default());
        self.emit(Instruction::Return, Span::default());
        let module = BytecodeModule {
            source: self.program.program.source,
            entrypoint: self.entrypoint,
            functions: self
                .functions
                .into_iter()
                .map(FunctionBuilder::finish)
                .collect(),
        };
        CompileOutput {
            module: Some(module),
            diagnostics: self.diagnostics,
        }
    }

    fn compile_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Assignment(assignment) => {
                if let AssignmentTarget::Option(option) = &assignment.target {
                    self.compile_option_assignment(option, assignment, stmt.span);
                } else {
                    self.compile_expr(&assignment.value);
                    self.store_target(&assignment.target, stmt.span, Some(stmt.id));
                }
            }
            StmtKind::Unlet(_) => {}
            StmtKind::Expression(expr) => {
                self.compile_expr(expr);
                self.emit(Instruction::Pop, stmt.span);
            }
            StmtKind::Echo(values) => {
                for value in values {
                    self.compile_expr(value);
                    let name = self.constant(Constant::String("echo".to_owned()));
                    self.emit(
                        Instruction::CallNamed {
                            name,
                            argc: 1,
                        },
                        value.span,
                    );
                    self.emit(Instruction::Pop, value.span);
                }
            }
            StmtKind::Execute(values) => {
                for value in values {
                    self.compile_expr(value);
                    self.emit(Instruction::Pop, value.span);
                }
            }
            StmtKind::If(value) => self.compile_if(value, stmt.span),
            StmtKind::While(value) => self.compile_while(value, stmt.span),
            StmtKind::For(value) => self.compile_for(value, stmt),
            StmtKind::Try(value) => self.compile_try(value, stmt.span),
            StmtKind::Throw(expr) => {
                self.compile_expr(expr);
                self.emit(Instruction::Throw, stmt.span);
            }
            StmtKind::Function(function) => self.compile_function_declaration(stmt, function),
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.compile_expr(value);
                } else {
                    self.emit(Instruction::LoadNull, stmt.span);
                }
                self.emit(Instruction::Return, stmt.span);
            }
            StmtKind::Break => {
                if self.current().loops.is_empty() {
                    return;
                }
                let jump = self.emit(Instruction::Jump(u32::MAX), stmt.span);
                self.current_mut()
                    .loops
                    .last_mut()
                    .expect("loop exists")
                    .break_jumps
                    .push(jump);
            }
            StmtKind::Continue => {
                if let Some(target) = self
                    .current()
                    .loops
                    .last()
                    .map(|context| context.continue_target)
                {
                    self.emit(Instruction::Jump(target), stmt.span);
                }
            }
            StmtKind::Finish => {
                self.emit(Instruction::LoadNull, stmt.span);
                self.emit(Instruction::Return, stmt.span);
            }
            StmtKind::ExCommand(command) => {
                let constant = self.constant(Constant::Command(Box::new(command.clone())));
                self.emit(Instruction::ExecuteCommand(constant), stmt.span);
                self.emit(Instruction::Pop, stmt.span);
            }
            StmtKind::Mapping(mapping) => {
                if let MappingRhs::Expression(expr) = &mapping.rhs {
                    self.compile_expr(expr);
                    self.emit(Instruction::Pop, expr.span);
                }
            }
            StmtKind::OptionSet(set) => {
                for operation in &set.operations {
                    if let Some(value) = option_value(operation) {
                        self.compile_expr(value);
                        self.emit(Instruction::Pop, value.span);
                    }
                }
            }
            StmtKind::Autocmd(command) => {
                for statement in &command.body {
                    self.compile_stmt(statement);
                }
            }
            StmtKind::Augroup(group) => {
                for statement in &group.body {
                    self.compile_stmt(statement);
                }
            }
            StmtKind::UserCommand(_) => {}
        }
    }

    fn compile_if(&mut self, value: &IfStmt, span: Span) {
        let mut exits = Vec::new();
        for branch in &value.branches {
            self.compile_expr(&branch.condition);
            let next = self.emit(Instruction::JumpIfFalse(u32::MAX), branch.condition.span);
            for statement in &branch.body {
                self.compile_stmt(statement);
            }
            exits.push(self.emit(Instruction::Jump(u32::MAX), span));
            let target = self.offset();
            self.patch_jump(next, target);
        }
        for statement in &value.else_body {
            self.compile_stmt(statement);
        }
        let end = self.offset();
        for exit in exits {
            self.patch_jump(exit, end);
        }
    }

    fn compile_while(&mut self, value: &WhileStmt, span: Span) {
        let start = self.offset();
        self.compile_expr(&value.condition);
        let exit = self.emit(Instruction::JumpIfFalse(u32::MAX), value.condition.span);
        self.current_mut().loops.push(LoopContext {
            start_offset: start,
            break_jumps: Vec::new(),
            continue_target: start,
        });
        for statement in &value.body {
            self.compile_stmt(statement);
        }
        self.emit(Instruction::Loop(start), span);
        let end = self.offset();
        self.patch_jump(exit, end);
        let context = self.current_mut().loops.pop().expect("loop context");
        for jump in context.break_jumps {
            self.patch_jump(jump, end);
        }
    }

    fn compile_for(&mut self, value: &ForStmt, stmt: &Stmt) {
        self.compile_expr(&value.iterable);
        self.emit(Instruction::IterStart, stmt.span);
        let start = self.offset();
        let next = self.emit(Instruction::IterNext { end: u32::MAX }, stmt.span);
        self.store_target(&value.binding, stmt.span, Some(stmt.id));
        self.current_mut().loops.push(LoopContext {
            start_offset: start,
            break_jumps: Vec::new(),
            continue_target: start,
        });
        for statement in &value.body {
            self.compile_stmt(statement);
        }
        self.emit(Instruction::Loop(start), stmt.span);
        let cleanup = self.offset();
        self.patch_jump(next, cleanup);
        self.emit(Instruction::IterEnd, stmt.span);
        let end = self.offset();
        let context = self.current_mut().loops.pop().expect("loop context");
        for jump in context.break_jumps {
            self.patch_jump(jump, cleanup);
        }
        let _ = end;
    }

    fn compile_try(&mut self, value: &TryStmt, span: Span) {
        let begin = self.emit(
            Instruction::TryBegin {
                handler: u32::MAX,
                stack_depth: 0,
            },
            span,
        );
        let try_start = self.offset();
        for statement in &value.body {
            self.compile_stmt(statement);
        }
        self.emit(Instruction::TryEnd, span);
        let normal_jump = self.emit(Instruction::Jump(u32::MAX), span);
        let handler = self.offset();
        self.patch_handler(begin, handler);
        if let Some(catch) = value.catches.first() {
            self.emit(Instruction::Pop, span);
            for statement in &catch.body {
                self.compile_stmt(statement);
            }
        } else {
            self.emit(Instruction::Throw, span);
        }
        let finally = self.offset();
        self.patch_jump(normal_jump, finally);
        for statement in &value.finally_body {
            self.compile_stmt(statement);
        }
        let end = self.offset();
        self.current_mut().handlers.push(ExceptionHandler {
            start: try_start,
            end: handler,
            handler,
            finally: (!value.finally_body.is_empty()).then_some(finally),
        });
        let _ = end;
    }

    fn compile_function_declaration(&mut self, stmt: &Stmt, function: &FunctionDecl) {
        let Some(resolved) = self
            .program
            .functions
            .iter()
            .find(|resolved| resolved.node == stmt.id)
            .cloned()
        else {
            self.error("C001", "missing resolved function", stmt.span);
            return;
        };
        self.compile_function_body(&resolved, function);
        for capture in &resolved.captures {
            self.load_symbol(capture.symbol, stmt.span);
        }
        self.emit(
            Instruction::MakeClosure {
                function: resolved.id,
                captures: resolved.captures.len() as u16,
            },
            stmt.span,
        );
        if let Some(symbol) = self
            .program
            .declarations
            .get(&stmt.id)
            .and_then(|symbols| symbols.first())
            .copied()
        {
            self.store_symbol(symbol, stmt.span);
        } else {
            self.emit(Instruction::Pop, stmt.span);
        }
    }

    fn compile_function_body(&mut self, resolved: &ResolvedFunction, function: &FunctionDecl) {
        if !self.compiled.insert(resolved.id) {
            return;
        }
        let parent = self.current_function;
        self.current_function = self
            .builder_index(resolved.id)
            .expect("resolved function builder");
        {
            let builder = self.current_mut();
            builder.name = Some(display_scoped(&function.name));
            builder.arity = function.parameters.len() as u16;
            builder.optional_parameters = function
                .parameters
                .iter()
                .filter(|parameter| parameter.default.is_some())
                .count() as u16;
            builder.variadic = function.varargs.is_some();
            builder.asynchronous = function.attributes.asynchronous;
        }
        for statement in &function.body {
            self.compile_stmt(statement);
        }
        if !matches!(self.current().code.last(), Some(Instruction::Return)) {
            self.emit(Instruction::LoadNull, Span::default());
            self.emit(Instruction::Return, Span::default());
        }
        self.current_function = parent;
    }

    fn compile_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Literal(literal) => {
                let constant = self.constant(literal_constant(literal));
                self.emit(Instruction::LoadConstant(constant), expr.span);
            }
            ExprKind::Variable(_) => {
                if let Some(symbol) = self.program.bindings.get(&expr.id).copied() {
                    self.load_symbol(symbol, expr.span);
                } else {
                    self.error("C002", "unresolved variable", expr.span);
                    self.emit(Instruction::LoadNull, expr.span);
                }
            }
            ExprKind::Unary { operator, operand } => {
                self.compile_expr(operand);
                self.emit(Instruction::Unary(*operator), expr.span);
            }
            ExprKind::Binary {
                left,
                operator,
                right,
            } => {
                self.compile_expr(left);
                self.compile_expr(right);
                self.emit(Instruction::Binary(*operator), expr.span);
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.compile_expr(condition);
                let otherwise = self.emit(Instruction::JumpIfFalse(u32::MAX), condition.span);
                self.compile_expr(then_expr);
                let end_jump = self.emit(Instruction::Jump(u32::MAX), then_expr.span);
                let otherwise_target = self.offset();
                self.patch_jump(otherwise, otherwise_target);
                self.compile_expr(else_expr);
                let end = self.offset();
                self.patch_jump(end_jump, end);
            }
            ExprKind::Call { callee, arguments } => {
                self.compile_expr(callee);
                for argument in arguments {
                    self.compile_expr(argument);
                }
                self.emit(Instruction::Call(arguments.len() as u16), expr.span);
            }
            ExprKind::MethodCall {
                receiver,
                method,
                arguments,
            } => {
                self.compile_expr(receiver);
                for argument in arguments {
                    self.compile_expr(argument);
                }
                let name = self.constant(Constant::String(method.clone()));
                self.emit(
                    Instruction::CallNamed {
                        name,
                        argc: arguments.len() as u16 + 1,
                    },
                    expr.span,
                );
            }
            ExprKind::Index { target, index } => {
                self.compile_expr(target);
                self.compile_expr(index);
                self.emit(Instruction::GetIndex, expr.span);
            }
            ExprKind::Slice { .. } => {
                self.error("C003", "slice execution is not implemented yet", expr.span);
                self.emit(Instruction::LoadNull, expr.span);
            }
            ExprKind::Member { target, name } => {
                self.compile_expr(target);
                let name = self.constant(Constant::String(name.clone()));
                self.emit(Instruction::GetMember(name), expr.span);
            }
            ExprKind::List(values) => {
                for value in values {
                    self.compile_expr(value);
                }
                self.emit(Instruction::BuildList(values.len() as u32), expr.span);
            }
            ExprKind::Dictionary(entries) => {
                for entry in entries {
                    self.compile_expr(&entry.key);
                    self.compile_expr(&entry.value);
                }
                self.emit(
                    Instruction::BuildDictionary(entries.len() as u32),
                    expr.span,
                );
            }
            ExprKind::Lambda(_) => {
                if let Some(resolved) = self
                    .program
                    .functions
                    .iter()
                    .find(|function| function.node == expr.id)
                    .cloned()
                {
                    for capture in &resolved.captures {
                        self.load_symbol(capture.symbol, expr.span);
                    }
                    self.emit(
                        Instruction::MakeClosure {
                            function: resolved.id,
                            captures: resolved.captures.len() as u16,
                        },
                        expr.span,
                    );
                } else {
                    self.error("C004", "unresolved lambda", expr.span);
                    self.emit(Instruction::LoadNull, expr.span);
                }
            }
            ExprKind::Await(value) => {
                self.compile_expr(value);
                self.emit(Instruction::Await, expr.span);
            }
            ExprKind::InterpolatedString(parts) => {
                let text = parts
                    .iter()
                    .filter_map(|part| {
                        if let StringPart::Text(text) = part {
                            Some(text.as_str())
                        } else {
                            None
                        }
                    })
                    .collect::<String>();
                let constant = self.constant(Constant::String(text));
                self.emit(Instruction::LoadConstant(constant), expr.span);
            }
            ExprKind::Option(option) => {
                let name = self.constant(Constant::String(option.name.clone()));
                self.emit(
                    Instruction::LoadOption {
                        scope: option_scope(option.scope),
                        name,
                    },
                    expr.span,
                );
                self.emit(Instruction::Await, expr.span);
            }
            ExprKind::Register(_) | ExprKind::Environment(_) => {
                self.error(
                    "C005",
                    "host-backed value is unavailable in the synchronous core",
                    expr.span,
                );
                self.emit(Instruction::LoadNull, expr.span);
            }
        }
    }

    fn store_target(
        &mut self,
        target: &AssignmentTarget,
        span: Span,
        declaration_node: Option<NodeId>,
    ) {
        match target {
            AssignmentTarget::Option(_) => {
                self.error("C009", "option assignment was not lowered", span);
                self.emit(Instruction::Pop, span);
            }
            AssignmentTarget::Name(_) => {
                let statement = declaration_node
                    .and_then(|node| self.program.declarations.get(&node))
                    .and_then(|symbols| {
                        symbols
                            .iter()
                            .find(|symbol| self.symbol_matches_target(**symbol, target))
                            .copied()
                    });
                if let Some(symbol) = statement {
                    self.store_symbol(symbol, span);
                } else {
                    self.error("C006", "missing assignment binding", span);
                    self.emit(Instruction::Pop, span);
                }
            }
            AssignmentTarget::Destructure(_) => {
                self.error(
                    "C007",
                    "destructuring execution is not implemented yet",
                    span,
                );
                self.emit(Instruction::Pop, span);
            }
            AssignmentTarget::Index { .. } | AssignmentTarget::Slice { .. } => {
                self.error(
                    "C008",
                    "indexed assignment execution is not implemented yet",
                    span,
                );
                self.emit(Instruction::Pop, span);
            }
        }
    }

    fn compile_option_assignment(
        &mut self,
        option: &OptionName,
        assignment: &Assignment,
        span: Span,
    ) {
        if assignment.operator != AssignmentOperator::Assign {
            let name = self.constant(Constant::String(option.name.clone()));
            self.emit(
                Instruction::LoadOption {
                    scope: option_scope(option.scope),
                    name,
                },
                span,
            );
            self.emit(Instruction::Await, span);
        }
        self.compile_expr(&assignment.value);
        let binary = match assignment.operator {
            AssignmentOperator::Assign => None,
            AssignmentOperator::Add => Some(BinaryOperator::Add),
            AssignmentOperator::Subtract => Some(BinaryOperator::Subtract),
            AssignmentOperator::Multiply => Some(BinaryOperator::Multiply),
            AssignmentOperator::Divide => Some(BinaryOperator::Divide),
            AssignmentOperator::Remainder => Some(BinaryOperator::Remainder),
            AssignmentOperator::Concatenate => Some(BinaryOperator::Concatenate),
        };
        if let Some(binary) = binary {
            self.emit(Instruction::Binary(binary), span);
        }
        let name = self.constant(Constant::String(option.name.clone()));
        self.emit(
            Instruction::StoreOption {
                scope: option_scope(option.scope),
                name,
            },
            span,
        );
        self.emit(Instruction::Await, span);
        self.emit(Instruction::Pop, span);
    }

    fn symbol_matches_target(&self, symbol: SymbolId, target: &AssignmentTarget) -> bool {
        match target {
            AssignmentTarget::Name(name) => {
                let symbol = &self.program.symbols[symbol.0 as usize];
                symbol.name == name.name
            }
            _ => false,
        }
    }
    fn load_symbol(&mut self, id: SymbolId, span: Span) {
        let symbol = &self.program.symbols[id.0 as usize];
        let instruction = if let Some(slot) = self.capture_slot(id) {
            Instruction::LoadCapture(slot)
        } else if self.is_current_local(symbol) {
            Instruction::LoadLocal(symbol.slot)
        } else {
            let name = self.constant(Constant::String(self.symbol_runtime_name(symbol)));
            Instruction::LoadGlobal(name)
        };
        self.emit(instruction, span);
    }
    fn store_symbol(&mut self, id: SymbolId, span: Span) {
        let symbol = &self.program.symbols[id.0 as usize];
        let instruction = if let Some(slot) = self.capture_slot(id) {
            Instruction::StoreCapture(slot)
        } else if self.is_current_local(symbol) {
            Instruction::StoreLocal(symbol.slot)
        } else {
            let name = self.constant(Constant::String(self.symbol_runtime_name(symbol)));
            Instruction::StoreGlobal(name)
        };
        self.emit(instruction, span);
    }
    fn symbol_runtime_name(&self, symbol: &Symbol) -> String {
        let prefix = match symbol.namespace {
            Scope::Global => "g".to_owned(),
            Scope::Local => "l".to_owned(),
            Scope::Script => format!("s{}", self.program.program.source.0),
            Scope::Argument => "a".to_owned(),
            Scope::Buffer => "b".to_owned(),
            Scope::Window => "w".to_owned(),
            Scope::Tab => "t".to_owned(),
            Scope::Vim => "v".to_owned(),
            Scope::Unqualified => String::new(),
        };
        format!("{prefix}:{}", symbol.name)
    }

    fn capture_slot(&self, id: SymbolId) -> Option<u32> {
        self.current_resolved().and_then(|function| {
            function
                .captures
                .iter()
                .find(|capture| capture.symbol == id)
                .map(|capture| capture.capture_slot)
        })
    }
    fn is_current_local(&self, symbol: &Symbol) -> bool {
        self.current_resolved()
            .is_some_and(|function| symbol.owner == function.scope)
    }
    fn current_resolved(&self) -> Option<&ResolvedFunction> {
        let id = self.current().id;
        self.program
            .functions
            .iter()
            .find(|function| function.id == id)
    }
    fn builder_index(&self, id: FunctionId) -> Option<usize> {
        self.functions.iter().position(|builder| builder.id == id)
    }
    fn constant(&mut self, constant: Constant) -> ConstantId {
        if let Some(index) = self
            .current()
            .constants
            .iter()
            .position(|value| value == &constant)
        {
            return ConstantId(index as u32);
        }
        let id = ConstantId(self.current().constants.len() as u32);
        self.current_mut().constants.push(constant);
        id
    }
    fn emit(&mut self, instruction: Instruction, span: Span) -> u32 {
        let offset = self.offset();
        self.current_mut().code.push(instruction);
        self.current_mut().spans.push((offset, span));
        offset
    }
    fn offset(&self) -> u32 {
        self.current().code.len() as u32
    }
    fn patch_jump(&mut self, offset: u32, target: u32) {
        match &mut self.current_mut().code[offset as usize] {
            Instruction::Jump(value)
            | Instruction::JumpIfFalse(value)
            | Instruction::JumpIfTrue(value)
            | Instruction::Loop(value) => *value = target,
            Instruction::IterNext { end } => *end = target,
            _ => panic!("instruction is not a jump"),
        }
    }
    fn patch_handler(&mut self, offset: u32, target: u32) {
        if let Instruction::TryBegin { handler, .. } = &mut self.current_mut().code[offset as usize]
        {
            *handler = target;
        }
    }
    fn current(&self) -> &FunctionBuilder {
        &self.functions[self.current_function]
    }
    fn current_mut(&mut self) -> &mut FunctionBuilder {
        &mut self.functions[self.current_function]
    }
    fn error(&mut self, code: &str, message: &str, span: Span) {
        self.diagnostics
            .push(Diagnostic::error(code, message, span));
    }
}

impl FunctionBuilder {
    fn new(id: FunctionId, local_count: u32, capture_count: u16) -> Self {
        Self {
            id,
            name: None,
            arity: 0,
            optional_parameters: 0,
            variadic: false,
            local_count,
            capture_count,
            asynchronous: false,
            code: Vec::new(),
            constants: Vec::new(),
            spans: Vec::new(),
            loops: Vec::new(),
            exception_handlers: Vec::new(),
            handlers: Vec::new(),
        }
    }
    fn finish(self) -> FunctionPrototype {
        FunctionPrototype {
            id: self.id,
            name: self.name,
            arity: self.arity,
            optional_parameters: self.optional_parameters,
            variadic: self.variadic,
            local_count: self.local_count,
            capture_count: self.capture_count,
            asynchronous: self.asynchronous,
            code: self.code,
            constants: self.constants,
            spans: self.spans,
            handlers: self.handlers,
        }
    }
}

fn option_scope(scope: OptionScope) -> OptionScopeOperand {
    match scope {
        OptionScope::Unqualified | OptionScope::GlobalLocal => OptionScopeOperand::Unqualified,
        OptionScope::Local => OptionScopeOperand::Local,
        OptionScope::Global => OptionScopeOperand::Global,
    }
}

fn script_local_count(program: &ResolvedProgram) -> u32 {
    program
        .symbols
        .iter()
        .filter(|symbol| symbol.owner == ScopeId(0))
        .map(|symbol| symbol.slot + 1)
        .max()
        .unwrap_or(0)
}
fn literal_constant(literal: &Literal) -> Constant {
    match literal {
        Literal::Null => Constant::Null,
        Literal::Bool(value) => Constant::Bool(*value),
        Literal::Integer(value) => Constant::Integer(*value),
        Literal::Float(value) => Constant::Float(*value),
        Literal::String(value) => Constant::String(value.clone()),
        Literal::Blob(value) => Constant::Blob(value.clone()),
    }
}

fn display_scoped(name: &ScopedName) -> String {
    if name.scope == Scope::Unqualified {
        name.name.clone()
    } else {
        format!(
            "{}:{}",
            match name.scope {
                Scope::Global => "g",
                Scope::Local => "l",
                Scope::Script => "s",
                Scope::Argument => "a",
                Scope::Buffer => "b",
                Scope::Window => "w",
                Scope::Tab => "t",
                Scope::Vim => "v",
                Scope::Unqualified => "",
            },
            name.name
        )
    }
}
fn option_value(operation: &OptionOperation) -> Option<&Expr> {
    match operation {
        OptionOperation::Set { value, .. } => value.as_ref(),
        OptionOperation::Append { value, .. }
        | OptionOperation::Prepend { value, .. }
        | OptionOperation::Remove { value, .. } => Some(value),
        _ => None,
    }
}
