use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::source::{Diagnostic, Span};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SymbolId(pub u32);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ScopeId(pub u32);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FunctionId(pub u32);

#[derive(Clone, Debug)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub namespace: Scope,
    pub kind: SymbolKind,
    pub declaration: Span,
    pub mutable: bool,
    pub slot: u32,
    pub owner: ScopeId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolKind {
    Variable,
    Parameter,
    Function,
    Builtin,
    Captured,
    UserCommand,
}

#[derive(Clone, Debug)]
pub struct LexicalScope {
    pub id: ScopeId,
    pub parent: Option<ScopeId>,
    pub kind: ScopeKind,
    pub symbols: HashMap<String, SymbolId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScopeKind {
    Script,
    Function,
    Lambda,
    Block,
    Catch,
}

#[derive(Clone, Debug)]
pub struct Capture {
    pub symbol: SymbolId,
    pub source_slot: u32,
    pub capture_slot: u32,
}

#[derive(Clone, Debug)]
pub struct ResolvedFunction {
    pub id: FunctionId,
    pub node: NodeId,
    pub scope: ScopeId,
    pub captures: Vec<Capture>,
    pub local_count: u32,
}

#[derive(Clone, Debug)]
pub struct ResolvedProgram {
    pub program: Program,
    pub scopes: Vec<LexicalScope>,
    pub symbols: Vec<Symbol>,
    pub functions: Vec<ResolvedFunction>,
    /// Variable/function references keyed by expression node.
    pub bindings: HashMap<NodeId, SymbolId>,
    /// Symbols introduced or assigned by a statement. Destructuring may bind several.
    pub declarations: HashMap<NodeId, Vec<SymbolId>>,
}

#[derive(Clone, Debug)]
pub struct ResolverConfig {
    pub builtins: HashSet<String>,
    /// Treat unresolved `name#part#function` references as lazy autoload functions.
    pub allow_autoload: bool,
    /// Hosts often supply globals dynamically. When enabled, an unresolved explicit
    /// global is materialized instead of diagnosed.
    pub allow_dynamic_globals: bool,
    /// Treat unqualified variables at the top-level as global.
    pub unqualified_is_global: bool,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            builtins: crate::runtime::builtins::BuiltinRegistry::with_defaults()
                .names()
                .map(str::to_owned)
                .collect(),
            allow_autoload: true,
            allow_dynamic_globals: true,
            unqualified_is_global: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Resolver {
    pub scopes: Vec<LexicalScope>,
    pub symbols: Vec<Symbol>,
    pub functions: Vec<ResolvedFunction>,
    pub bindings: HashMap<NodeId, SymbolId>,
    pub declarations: HashMap<NodeId, Vec<SymbolId>>,
    pub current_scope: ScopeId,
    pub diagnostics: Vec<Diagnostic>,
    pub config: ResolverConfig,
    function_stack: Vec<FunctionId>,
}

#[derive(Clone, Debug)]
pub struct ResolveOutput {
    pub program: Option<ResolvedProgram>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Resolver {
    pub fn new(config: ResolverConfig) -> Self {
        let root = LexicalScope {
            id: ScopeId(0),
            parent: None,
            kind: ScopeKind::Script,
            symbols: HashMap::new(),
        };
        let mut resolver = Self {
            scopes: vec![root],
            symbols: Vec::new(),
            functions: Vec::new(),
            bindings: HashMap::new(),
            declarations: HashMap::new(),
            current_scope: ScopeId(0),
            diagnostics: Vec::new(),
            config,
            function_stack: Vec::new(),
        };
        let builtins: Vec<_> = resolver.config.builtins.iter().cloned().collect();
        for name in builtins {
            resolver.define_in(
                ScopeId(0),
                name,
                Scope::Unqualified,
                SymbolKind::Builtin,
                Span::default(),
                false,
            );
        }
        resolver
    }

    pub fn resolve(mut self, program: Program) -> ResolveOutput {
        for statement in &program.statements {
            self.resolve_stmt(statement);
        }
        let diagnostics = self.diagnostics.clone();
        ResolveOutput {
            program: Some(ResolvedProgram {
                program,
                scopes: self.scopes,
                symbols: self.symbols,
                functions: self.functions,
                bindings: self.bindings,
                declarations: self.declarations,
            }),
            diagnostics,
        }
    }

    fn resolve_stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Assignment(assignment) => {
                self.resolve_expr(&assignment.value);
                let mut declared = Vec::new();
                self.resolve_target(
                    &assignment.target,
                    assignment.is_const,
                    stmt.span,
                    &mut declared,
                );
                self.declarations.insert(stmt.id, declared);
            }
            StmtKind::Unlet(values) => {
                for value in values {
                    self.resolve_expr(value);
                }
            }
            StmtKind::Expression(expr) | StmtKind::Throw(expr) => self.resolve_expr(expr),
            StmtKind::Echo(values) | StmtKind::Execute(values) => {
                for value in values {
                    self.resolve_expr(value);
                }
            }
            StmtKind::If(value) => {
                for branch in &value.branches {
                    self.resolve_expr(&branch.condition);
                    self.with_scope(ScopeKind::Block, |this| {
                        for statement in &branch.body {
                            this.resolve_stmt(statement);
                        }
                    });
                }
                self.with_scope(ScopeKind::Block, |this| {
                    for statement in &value.else_body {
                        this.resolve_stmt(statement);
                    }
                });
            }
            StmtKind::While(value) => {
                self.resolve_expr(&value.condition);
                self.with_scope(ScopeKind::Block, |this| {
                    for statement in &value.body {
                        this.resolve_stmt(statement);
                    }
                });
            }
            StmtKind::For(value) => {
                self.resolve_expr(&value.iterable);
                self.with_scope(ScopeKind::Block, |this| {
                    let mut declared = Vec::new();
                    this.resolve_target(&value.binding, false, stmt.span, &mut declared);
                    this.declarations.insert(stmt.id, declared);
                    for statement in &value.body {
                        this.resolve_stmt(statement);
                    }
                });
            }
            StmtKind::Try(value) => {
                self.with_scope(ScopeKind::Block, |this| {
                    for statement in &value.body {
                        this.resolve_stmt(statement);
                    }
                });
                for clause in &value.catches {
                    self.with_scope(ScopeKind::Catch, |this| {
                        if let Some(name) = &clause.binding {
                            this.declare_name(name, false, stmt.span);
                        }
                        for statement in &clause.body {
                            this.resolve_stmt(statement);
                        }
                    });
                }
                self.with_scope(ScopeKind::Block, |this| {
                    for statement in &value.finally_body {
                        this.resolve_stmt(statement);
                    }
                });
            }
            StmtKind::Function(function) => self.resolve_function(stmt, function),
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.resolve_expr(value);
                }
            }
            StmtKind::UserCommand(command) => {
                let symbol = self.define_or_diagnose(
                    self.current_scope,
                    command.name.clone(),
                    Scope::Unqualified,
                    SymbolKind::UserCommand,
                    stmt.span,
                    false,
                );
                self.declarations.insert(stmt.id, vec![symbol]);
            }
            StmtKind::Mapping(mapping) => {
                if let MappingRhs::Expression(expr) = &mapping.rhs {
                    self.resolve_expr(expr);
                }
            }
            StmtKind::OptionSet(set) => {
                for operation in &set.operations {
                    match operation {
                        OptionOperation::Set {
                            value: Some(value), ..
                        }
                        | OptionOperation::Append { value, .. }
                        | OptionOperation::Prepend { value, .. }
                        | OptionOperation::Remove { value, .. } => self.resolve_expr(value),
                        _ => {}
                    }
                }
            }
            StmtKind::Autocmd(command) => self.with_scope(ScopeKind::Block, |this| {
                for statement in &command.body {
                    this.resolve_stmt(statement);
                }
            }),
            StmtKind::Augroup(group) => self.with_scope(ScopeKind::Block, |this| {
                for statement in &group.body {
                    this.resolve_stmt(statement);
                }
            }),
            StmtKind::Break | StmtKind::Continue | StmtKind::Finish | StmtKind::ExCommand(_) => {}
        }
    }

    fn resolve_function(&mut self, stmt: &Stmt, function: &FunctionDecl) {
        let owner = self.scope_for_name(&function.name, true);
        let symbol = self.define_or_diagnose(
            owner,
            function.name.name.clone(),
            function.name.scope,
            SymbolKind::Function,
            stmt.span,
            false,
        );
        self.declarations.insert(stmt.id, vec![symbol]);
        let scope = self.push_scope(ScopeKind::Function);
        let id = FunctionId(self.functions.len() as u32);
        self.functions.push(ResolvedFunction {
            id,
            node: stmt.id,
            scope,
            captures: Vec::new(),
            local_count: 0,
        });
        self.function_stack.push(id);
        for parameter in &function.parameters {
            if let Some(default) = &parameter.default {
                self.resolve_expr(default);
            }
            self.define_or_diagnose(
                scope,
                parameter.name.clone(),
                Scope::Argument,
                SymbolKind::Parameter,
                parameter.span,
                true,
            );
        }
        if let Some(varargs) = &function.varargs {
            self.define_or_diagnose(
                scope,
                varargs.clone(),
                Scope::Argument,
                SymbolKind::Parameter,
                stmt.span,
                true,
            );
        }
        for statement in &function.body {
            self.resolve_stmt(statement);
        }
        let local_count = self
            .symbols
            .iter()
            .filter(|symbol| {
                symbol.owner == scope
                    && matches!(
                        symbol.kind,
                        SymbolKind::Variable | SymbolKind::Parameter | SymbolKind::Function
                    )
            })
            .count() as u32;
        self.functions[id.0 as usize].local_count = local_count;
        self.function_stack.pop();
        self.pop_scope();
    }

    fn resolve_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Variable(name) => {
                if let Some(symbol) = self.lookup_name(name) {
                    self.bindings.insert(expr.id, symbol);
                    self.capture_if_needed(symbol);
                } else {
                    self.undefined(name, expr.span);
                }
            }
            ExprKind::Unary { operand, .. } | ExprKind::Await(operand) => {
                self.resolve_expr(operand)
            }
            ExprKind::Binary { left, right, .. } => {
                self.resolve_expr(left);
                self.resolve_expr(right);
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.resolve_expr(condition);
                self.resolve_expr(then_expr);
                self.resolve_expr(else_expr);
            }
            ExprKind::Call { callee, arguments } => {
                self.resolve_expr(callee);
                for argument in arguments {
                    self.resolve_expr(argument);
                }
            }
            ExprKind::MethodCall {
                receiver,
                arguments,
                ..
            } => {
                self.resolve_expr(receiver);
                for argument in arguments {
                    self.resolve_expr(argument);
                }
            }
            ExprKind::Index { target, index } => {
                self.resolve_expr(target);
                self.resolve_expr(index);
            }
            ExprKind::Slice { target, start, end } => {
                self.resolve_expr(target);
                if let Some(value) = start {
                    self.resolve_expr(value);
                }
                if let Some(value) = end {
                    self.resolve_expr(value);
                }
            }
            ExprKind::Member { target, .. } => self.resolve_expr(target),
            ExprKind::List(values) => {
                for value in values {
                    self.resolve_expr(value);
                }
            }
            ExprKind::Dictionary(entries) => {
                for entry in entries {
                    self.resolve_expr(&entry.key);
                    self.resolve_expr(&entry.value);
                }
            }
            ExprKind::Lambda(lambda) => self.resolve_lambda(expr.id, expr.span, lambda),
            ExprKind::InterpolatedString(parts) => {
                for part in parts {
                    if let StringPart::Expression(value) = part {
                        self.resolve_expr(value);
                    }
                }
            }
            ExprKind::Literal(_)
            | ExprKind::Register(_)
            | ExprKind::Option(_)
            | ExprKind::Environment(_) => {}
        }
    }

    fn resolve_lambda(&mut self, node: NodeId, span: Span, lambda: &LambdaExpr) {
        let scope = self.push_scope(ScopeKind::Lambda);
        let id = FunctionId(self.functions.len() as u32);
        self.functions.push(ResolvedFunction {
            id,
            node,
            scope,
            captures: Vec::new(),
            local_count: 0,
        });
        self.function_stack.push(id);
        for parameter in &lambda.parameters {
            if let Some(default) = &parameter.default {
                self.resolve_expr(default);
            }
            self.define_or_diagnose(
                scope,
                parameter.name.clone(),
                Scope::Argument,
                SymbolKind::Parameter,
                parameter.span,
                true,
            );
        }
        match &lambda.body {
            LambdaBody::Expression(expr) => self.resolve_expr(expr),
            LambdaBody::Block(body) => {
                for statement in body {
                    self.resolve_stmt(statement);
                }
            }
        }
        self.functions[id.0 as usize].local_count = self
            .symbols
            .iter()
            .filter(|symbol| symbol.owner == scope)
            .count() as u32;
        self.function_stack.pop();
        self.pop_scope();
        let _ = span;
    }

    fn resolve_target(
        &mut self,
        target: &AssignmentTarget,
        is_const: bool,
        span: Span,
        output: &mut Vec<SymbolId>,
    ) {
        match target {
            AssignmentTarget::Option(_) => {}
            AssignmentTarget::Name(name) => {
                let symbol = self.declare_name(name, is_const, span);
                output.push(symbol);
            }
            AssignmentTarget::Index { target, index } => {
                self.resolve_expr(target);
                self.resolve_expr(index);
            }
            AssignmentTarget::Slice { target, start, end } => {
                self.resolve_expr(target);
                if let Some(value) = start {
                    self.resolve_expr(value);
                }
                if let Some(value) = end {
                    self.resolve_expr(value);
                }
            }
            AssignmentTarget::Destructure(targets) => {
                for target in targets {
                    self.resolve_target(target, is_const, span, output);
                }
            }
        }
    }

    fn declare_name(&mut self, name: &ScopedName, is_const: bool, span: Span) -> SymbolId {
        let mut name = name.clone();
        let scope = self.scope_for_name(&name, true);
        if self.config.unqualified_is_global && name.scope == Scope::Unqualified && scope == ScopeId(0) {
            name.scope = Scope::Global;
        }
        let key = symbol_key(name.scope, &name.name);
        if let Some(symbol) = self.scopes[scope.0 as usize].symbols.get(&key).copied() {
            if self.symbols[symbol.0 as usize].kind != SymbolKind::Builtin {
                if !self.symbols[symbol.0 as usize].mutable {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "R003",
                            format!("cannot assign to immutable variable {}", display_name(&name)),
                            span,
                        )
                        .with_label(
                            self.symbols[symbol.0 as usize].declaration,
                            "declared immutable here",
                        ),
                    );
                } else if is_const {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "R002",
                            format!("{} is already declared", display_name(&name)),
                            span,
                        )
                        .with_label(
                            self.symbols[symbol.0 as usize].declaration,
                            "previous declaration here",
                        ),
                    );
                }
                return symbol;
            }
        }
        self.define_in(
            scope,
            name.name.clone(),
            name.scope,
            SymbolKind::Variable,
            span,
            !is_const,
        )
    }

    fn lookup_name(&mut self, name: &ScopedName) -> Option<SymbolId> {
        let mut name = name.clone();
        if self.config.unqualified_is_global
            && name.scope == Scope::Unqualified
            && self.nearest_callable_scope().is_none()
            && !self.config.builtins.contains(&name.name)
        {
            name.scope = Scope::Global;
        }
        let key = symbol_key(name.scope, &name.name);
        if name.scope != Scope::Unqualified {
            let scope = self.scope_for_name(&name, false);
            if let Some(symbol) = self.scopes[scope.0 as usize].symbols.get(&key).copied() {
                return Some(symbol);
            }
            if name.scope == Scope::Global && self.config.allow_dynamic_globals {
                return Some(self.define_in(
                    ScopeId(0),
                    name.name.clone(),
                    Scope::Global,
                    SymbolKind::Variable,
                    Span::default(),
                    true,
                ));
            }
            return None;
        }
        let argument_key = symbol_key(Scope::Argument, &name.name);
        let local_key = symbol_key(Scope::Local, &name.name);
        let mut scope = Some(self.current_scope);
        while let Some(id) = scope {
            let lexical_scope = &self.scopes[id.0 as usize];
            if let Some(symbol) = lexical_scope.symbols.get(&key) {
                return Some(*symbol);
            }
            if matches!(lexical_scope.kind, ScopeKind::Function | ScopeKind::Lambda)
                && let Some(symbol) = lexical_scope
                    .symbols
                    .get(&argument_key)
                    .or_else(|| lexical_scope.symbols.get(&local_key))
            {
                return Some(*symbol);
            }
            scope = lexical_scope.parent;
        }
        if name.name == "version" {
            return Some(self.define_in(
                ScopeId(0),
                "version".into(),
                Scope::Vim,
                SymbolKind::Variable,
                Span::default(),
                false,
            ));
        }
        if self.config.allow_autoload && name.name.contains('#') {
            return Some(self.define_in(
                ScopeId(0),
                name.name.clone(),
                Scope::Unqualified,
                SymbolKind::Function,
                Span::default(),
                false,
            ));
        }
        None
    }

    fn capture_if_needed(&mut self, symbol: SymbolId) {
        let Some(current_function) = self.function_stack.last().copied() else {
            return;
        };
        let owner = self.symbols[symbol.0 as usize].owner;
        let Some(owner_function) = self.enclosing_function(owner) else {
            return;
        };
        if owner_function == current_function
            || matches!(
                self.symbols[symbol.0 as usize].namespace,
                Scope::Global
                    | Scope::Script
                    | Scope::Buffer
                    | Scope::Window
                    | Scope::Tab
                    | Scope::Vim
            )
        {
            return;
        }
        let function = &mut self.functions[current_function.0 as usize];
        if function
            .captures
            .iter()
            .any(|capture| capture.symbol == symbol)
        {
            return;
        }
        let capture_slot = function.captures.len() as u32;
        function.captures.push(Capture {
            symbol,
            source_slot: self.symbols[symbol.0 as usize].slot,
            capture_slot,
        });
    }

    fn enclosing_function(&self, mut scope: ScopeId) -> Option<FunctionId> {
        loop {
            if let Some(function) = self
                .functions
                .iter()
                .find(|function| function.scope == scope)
            {
                return Some(function.id);
            }
            scope = self.scopes[scope.0 as usize].parent?;
        }
    }

    fn scope_for_name(&self, name: &ScopedName, declaration: bool) -> ScopeId {
        match name.scope {
            Scope::Global
            | Scope::Script
            | Scope::Buffer
            | Scope::Window
            | Scope::Tab
            | Scope::Vim => ScopeId(0),
            Scope::Local | Scope::Argument => {
                self.nearest_callable_scope().unwrap_or(self.current_scope)
            }
            Scope::Unqualified if declaration => {
                self.nearest_callable_scope().unwrap_or(ScopeId(0))
            }
            Scope::Unqualified => self.current_scope,
        }
    }

    fn nearest_callable_scope(&self) -> Option<ScopeId> {
        let mut scope = Some(self.current_scope);
        while let Some(id) = scope {
            if matches!(
                self.scopes[id.0 as usize].kind,
                ScopeKind::Function | ScopeKind::Lambda
            ) {
                return Some(id);
            }
            scope = self.scopes[id.0 as usize].parent;
        }
        None
    }

    fn define_or_diagnose(
        &mut self,
        owner: ScopeId,
        name: String,
        namespace: Scope,
        kind: SymbolKind,
        span: Span,
        mutable: bool,
    ) -> SymbolId {
        let key = symbol_key(namespace, &name);
        if let Some(existing) = self.scopes[owner.0 as usize].symbols.get(&key).copied() {
            self.diagnostics.push(
                Diagnostic::error("R002", format!("{} is already declared", name), span)
                    .with_label(
                        self.symbols[existing.0 as usize].declaration,
                        "previous declaration here",
                    ),
            );
            return existing;
        }
        self.define_in(owner, name, namespace, kind, span, mutable)
    }

    fn define_in(
        &mut self,
        owner: ScopeId,
        name: String,
        namespace: Scope,
        kind: SymbolKind,
        declaration: Span,
        mutable: bool,
    ) -> SymbolId {
        let id = SymbolId(self.symbols.len() as u32);
        let slot = self
            .symbols
            .iter()
            .filter(|symbol| {
                symbol.owner == owner
                    && matches!(
                        symbol.kind,
                        SymbolKind::Variable | SymbolKind::Parameter | SymbolKind::Function
                    )
            })
            .count() as u32;
        self.symbols.push(Symbol {
            id,
            name: name.clone(),
            namespace,
            kind,
            declaration,
            mutable,
            slot,
            owner,
        });
        self.scopes[owner.0 as usize]
            .symbols
            .insert(symbol_key(namespace, &name), id);
        id
    }

    fn undefined(&mut self, name: &ScopedName, span: Span) {
        self.diagnostics.push(
            Diagnostic::error(
                "R001",
                format!("undefined variable or function {}", display_name(name)),
                span,
            )
            .with_note("declare it before use or register it as a host builtin"),
        );
    }
    fn push_scope(&mut self, kind: ScopeKind) -> ScopeId {
        let id = ScopeId(self.scopes.len() as u32);
        self.scopes.push(LexicalScope {
            id,
            parent: Some(self.current_scope),
            kind,
            symbols: HashMap::new(),
        });
        self.current_scope = id;
        id
    }
    fn pop_scope(&mut self) {
        self.current_scope = self.scopes[self.current_scope.0 as usize]
            .parent
            .expect("cannot pop script scope");
    }
    fn with_scope(&mut self, kind: ScopeKind, body: impl FnOnce(&mut Self)) {
        self.push_scope(kind);
        body(self);
        self.pop_scope();
    }
}

fn symbol_key(scope: Scope, name: &str) -> String {
    format!("{}:{name}", scope_prefix(scope))
}
fn display_name(name: &ScopedName) -> String {
    if name.scope == Scope::Unqualified {
        name.name.clone()
    } else {
        format!("{}:{}", scope_prefix(name.scope), name.name)
    }
}
fn scope_prefix(scope: Scope) -> &'static str {
    match scope {
        Scope::Global => "g",
        Scope::Local => "l",
        Scope::Script => "s",
        Scope::Argument => "a",
        Scope::Buffer => "b",
        Scope::Window => "w",
        Scope::Tab => "t",
        Scope::Vim => "v",
        Scope::Unqualified => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use crate::source::SourceId;
    fn resolve(source: &str) -> ResolveOutput {
        let lexed = Lexer::new(SourceId(0), source).lex();
        assert!(lexed.diagnostics.is_empty());
        let parsed = Parser::new(&lexed.tokens).parse();
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
        Resolver::new(ResolverConfig::default()).resolve(parsed.program.unwrap())
    }

    #[test]
    fn resolves_script_and_function_locals() {
        let output = resolve(
            "let script_value = 1\nfunction s:Get(arg)\nlet local = arg + script_value\nreturn local\nendfunction\n",
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let program = output.program.unwrap();
        assert_eq!(program.functions.len(), 1);
        assert!(program.bindings.len() >= 3);
        assert_eq!(program.functions[0].local_count, 2);
    }

    #[test]
    fn reports_undefined_and_immutable_names() {
        let output = resolve("const answer = 42\nlet answer = 0\necho missing\n");
        assert_eq!(
            output
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.as_deref() == Some("R003"))
                .count(),
            1
        );
        assert_eq!(
            output
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code.as_deref() == Some("R001"))
                .count(),
            1
        );
    }

    #[test]
    fn discovers_nested_function_captures() {
        let output = resolve(
            "function s:Outer(x) closure\nfunction s:Inner() closure\nreturn x\nendfunction\nreturn 0\nendfunction\n",
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let program = output.program.unwrap();
        assert_eq!(program.functions.len(), 2);
        assert_eq!(program.functions[1].captures.len(), 1);
    }

    #[test]
    fn recognizes_builtins_and_dynamic_globals() {
        let output = resolve("let x = len(g:items)\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    }
}
