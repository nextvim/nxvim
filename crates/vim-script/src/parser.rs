use crate::ast::*;
use crate::ex_parser::ExLineParser;
use crate::lexer::{Keyword, Operator, Token, TokenKind};
use crate::source::{Diagnostic, SourceId, Span};

type ParseResult<T> = Result<T, ()>;

#[derive(Clone, Debug)]
pub struct Parser<'a> {
    pub tokens: &'a [Token],
    pub cursor: usize,
    pub diagnostics: Vec<Diagnostic>,
    pub loop_depth: usize,
    pub function_depth: usize,
    source: Option<&'a str>,
    next_node_id: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ParseOutput {
    pub program: Option<Program>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParseContext {
    Script,
    Function,
    Lambda,
    Interpolation,
    Command,
}

impl<'a> Parser<'a> {
    pub fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            cursor: 0,
            diagnostics: Vec::new(),
            loop_depth: 0,
            function_depth: 0,
            source: None,
            next_node_id: 0,
        }
    }

    pub fn new_with_source(tokens: &'a [Token], source: &'a str) -> Self {
        let mut parser = Self::new(tokens);
        parser.source = Some(source);
        parser
    }

    pub fn parse(mut self) -> ParseOutput {
        let source = self
            .tokens
            .first()
            .map_or(SourceId(0), |token| token.span.source);
        let mut statements = Vec::new();
        self.skip_separators();
        while !self.at_end() {
            match self.statement() {
                Ok(statement) => statements.push(statement),
                Err(()) => self.synchronize(),
            }
            self.skip_separators();
        }
        ParseOutput {
            program: Some(Program { source, statements }),
            diagnostics: self.diagnostics,
        }
    }

    fn statement(&mut self) -> ParseResult<Stmt> {
        let start = self.current().span.start;
        let kind = match self.current().kind.clone() {
            TokenKind::Keyword(Keyword::Let) => {
                self.advance();
                StmtKind::Assignment(self.assignment(false)?)
            }
            TokenKind::Keyword(Keyword::Const) => {
                self.advance();
                StmtKind::Assignment(self.assignment(true)?)
            }
            TokenKind::Keyword(Keyword::Unlet) => {
                self.advance();
                StmtKind::Unlet(self.unlet_targets()?)
            }
            TokenKind::Keyword(Keyword::Echo) => {
                self.advance();
                StmtKind::Echo(self.expression_list_to_end()?)
            }
            TokenKind::Keyword(Keyword::Execute) => {
                self.advance();
                StmtKind::Execute(self.expression_list_to_end()?)
            }
            TokenKind::Keyword(Keyword::If) => {
                self.advance();
                return self.if_statement(start);
            }
            TokenKind::Keyword(Keyword::While) => {
                self.advance();
                return self.while_statement(start);
            }
            TokenKind::Keyword(Keyword::For) => {
                self.advance();
                return self.for_statement(start);
            }
            TokenKind::Keyword(Keyword::Try) => {
                self.advance();
                return self.try_statement(start);
            }
            TokenKind::Keyword(Keyword::Function) => {
                self.advance();
                return self.function_statement(start);
            }
            TokenKind::Keyword(Keyword::Throw) => {
                self.advance();
                StmtKind::Throw(self.expression(0)?)
            }
            TokenKind::Keyword(Keyword::Return) => {
                self.advance();
                if self.function_depth == 0 {
                    self.error_here("P020", "return is only valid inside a function");
                }
                StmtKind::Return(if self.at_statement_end() {
                    None
                } else {
                    Some(self.expression(0)?)
                })
            }
            TokenKind::Keyword(Keyword::Break) => {
                self.advance();
                if self.loop_depth == 0 {
                    self.error_here("P021", "break is only valid inside a loop");
                }
                StmtKind::Break
            }
            TokenKind::Keyword(Keyword::Continue) => {
                self.advance();
                if self.loop_depth == 0 {
                    self.error_here("P022", "continue is only valid inside a loop");
                }
                StmtKind::Continue
            }
            TokenKind::Keyword(Keyword::Finish) => {
                self.advance();
                StmtKind::Finish
            }
            TokenKind::Keyword(
                keyword @ (Keyword::Else
                | Keyword::ElseIf
                | Keyword::EndIf
                | Keyword::EndWhile
                | Keyword::EndFor
                | Keyword::Catch
                | Keyword::Finally
                | Keyword::EndTry
                | Keyword::EndFunction),
            ) => {
                self.error_here("P023", format!("unexpected block terminator {keyword:?}"));
                return Err(());
            }
            TokenKind::Identifier(_) if self.looks_like_assignment() => {
                StmtKind::Assignment(self.assignment(false)?)
            }
            TokenKind::Colon => {
                self.advance();
                StmtKind::ExCommand(self.ex_command()?)
            }
            TokenKind::Operator(Operator::Divide) | TokenKind::Question
                if self.source.is_some() =>
            {
                StmtKind::ExCommand(self.ex_command()?)
            }
            TokenKind::Identifier(_)
                if self.source.is_some() && self.looks_like_source_ex_command() =>
            {
                StmtKind::ExCommand(self.ex_command()?)
            }
            TokenKind::Identifier(_) if self.looks_like_ex_command() => {
                StmtKind::ExCommand(self.ex_command()?)
            }
            _ => StmtKind::Expression(self.expression(0)?),
        };
        let end = self.previous().span.end;
        self.require_statement_end()?;
        Ok(self.stmt(start, end, kind))
    }

    fn assignment(&mut self, is_const: bool) -> ParseResult<Assignment> {
        let target_expr = self.expression(90)?;
        let target = self.assignment_target(target_expr)?;
        let operator = match self.current().kind {
            TokenKind::Operator(Operator::Assign) => AssignmentOperator::Assign,
            TokenKind::Operator(Operator::AddAssign) => AssignmentOperator::Add,
            TokenKind::Operator(Operator::SubtractAssign) => AssignmentOperator::Subtract,
            TokenKind::Operator(Operator::MultiplyAssign) => AssignmentOperator::Multiply,
            TokenKind::Operator(Operator::DivideAssign) => AssignmentOperator::Divide,
            TokenKind::Operator(Operator::RemainderAssign) => AssignmentOperator::Remainder,
            TokenKind::Operator(Operator::ConcatenateAssign) => AssignmentOperator::Concatenate,
            _ => {
                self.error_here("P002", "expected an assignment operator");
                return Err(());
            }
        };
        self.advance();
        let value = self.expression(0)?;
        Ok(Assignment {
            target,
            operator,
            value,
            is_const,
        })
    }

    fn assignment_target(&mut self, expr: Expr) -> ParseResult<AssignmentTarget> {
        match expr.kind {
            ExprKind::Variable(name) => Ok(AssignmentTarget::Name(name)),
            ExprKind::Option(name) => Ok(AssignmentTarget::Option(name)),
            ExprKind::Index { target, index } => Ok(AssignmentTarget::Index { target, index }),
            ExprKind::Slice { target, start, end } => {
                Ok(AssignmentTarget::Slice { target, start, end })
            }
            ExprKind::List(values) => values
                .into_iter()
                .map(|value| self.assignment_target(value))
                .collect::<ParseResult<Vec<_>>>()
                .map(AssignmentTarget::Destructure),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "P003",
                    "invalid assignment target",
                    expr.span,
                ));
                Err(())
            }
        }
    }

    fn unlet_targets(&mut self) -> ParseResult<Vec<Expr>> {
        let mut targets = Vec::new();
        while !self.at_statement_end() {
            targets.push(self.expression(90)?);
            if !self.consume_simple(TokenKind::Comma) && self.at_statement_end() {
                break;
            }
        }
        if targets.is_empty() {
            self.error_here("P004", "unlet requires at least one variable");
            return Err(());
        }
        Ok(targets)
    }

    fn expression_list_to_end(&mut self) -> ParseResult<Vec<Expr>> {
        let mut values = Vec::new();
        while !self.at_statement_end() {
            values.push(self.expression(0)?);
            if !self.consume_simple(TokenKind::Comma) && !self.at_statement_end() {
                continue;
            }
        }
        Ok(values)
    }

    fn if_statement(&mut self, start: u32) -> ParseResult<Stmt> {
        let condition = self.expression(0)?;
        self.require_statement_end()?;
        self.skip_separators();
        let mut branches = vec![ConditionalBranch {
            condition,
            body: self.block_until(&[Keyword::ElseIf, Keyword::Else, Keyword::EndIf])?,
        }];
        while self.at_keyword(Keyword::ElseIf) {
            self.advance();
            let condition = self.expression(0)?;
            self.require_statement_end()?;
            self.skip_separators();
            let body = self.block_until(&[Keyword::ElseIf, Keyword::Else, Keyword::EndIf])?;
            branches.push(ConditionalBranch { condition, body });
        }
        let else_body = if self.at_keyword(Keyword::Else) {
            self.advance();
            self.require_statement_end()?;
            self.skip_separators();
            self.block_until(&[Keyword::EndIf])?
        } else {
            Vec::new()
        };
        let end = self
            .expect_keyword(Keyword::EndIf, "expected endif")?
            .span
            .end;
        self.require_statement_end()?;
        Ok(self.stmt(
            start,
            end,
            StmtKind::If(IfStmt {
                branches,
                else_body,
            }),
        ))
    }

    fn while_statement(&mut self, start: u32) -> ParseResult<Stmt> {
        let condition = self.expression(0)?;
        self.require_statement_end()?;
        self.skip_separators();
        self.loop_depth += 1;
        let body = self.block_until(&[Keyword::EndWhile]);
        self.loop_depth -= 1;
        let body = body?;
        let end = self
            .expect_keyword(Keyword::EndWhile, "expected endwhile")?
            .span
            .end;
        self.require_statement_end()?;
        Ok(self.stmt(start, end, StmtKind::While(WhileStmt { condition, body })))
    }

    fn for_statement(&mut self, start: u32) -> ParseResult<Stmt> {
        let binding_expr = self.expression(90)?;
        let binding = self.assignment_target(binding_expr)?;
        self.expect_keyword(Keyword::In, "expected 'in' after for binding")?;
        let iterable = self.expression(0)?;
        self.require_statement_end()?;
        self.skip_separators();
        self.loop_depth += 1;
        let body = self.block_until(&[Keyword::EndFor]);
        self.loop_depth -= 1;
        let body = body?;
        let end = self
            .expect_keyword(Keyword::EndFor, "expected endfor")?
            .span
            .end;
        self.require_statement_end()?;
        Ok(self.stmt(
            start,
            end,
            StmtKind::For(ForStmt {
                binding,
                iterable,
                body,
            }),
        ))
    }

    fn try_statement(&mut self, start: u32) -> ParseResult<Stmt> {
        self.require_statement_end()?;
        self.skip_separators();
        let body = self.block_until(&[Keyword::Catch, Keyword::Finally, Keyword::EndTry])?;
        let mut catches = Vec::new();
        while self.at_keyword(Keyword::Catch) {
            self.advance();
            let pattern = match self.current().kind.clone() {
                TokenKind::SingleQuotedString(value) | TokenKind::DoubleQuotedString(value) => {
                    self.advance();
                    Some(value)
                }
                _ if self.at_statement_end() => None,
                _ => {
                    let expr = self.expression(0)?;
                    match expr.kind {
                        ExprKind::Literal(Literal::String(value)) => Some(value),
                        _ => {
                            self.diagnostics.push(Diagnostic::error(
                                "P030",
                                "catch pattern must be a string",
                                expr.span,
                            ));
                            None
                        }
                    }
                }
            };
            self.require_statement_end()?;
            self.skip_separators();
            let catch_body =
                self.block_until(&[Keyword::Catch, Keyword::Finally, Keyword::EndTry])?;
            catches.push(CatchClause {
                pattern,
                binding: None,
                body: catch_body,
            });
        }
        let finally_body = if self.at_keyword(Keyword::Finally) {
            self.advance();
            self.require_statement_end()?;
            self.skip_separators();
            self.block_until(&[Keyword::EndTry])?
        } else {
            Vec::new()
        };
        let end = self
            .expect_keyword(Keyword::EndTry, "expected endtry")?
            .span
            .end;
        self.require_statement_end()?;
        Ok(self.stmt(
            start,
            end,
            StmtKind::Try(TryStmt {
                body,
                catches,
                finally_body,
            }),
        ))
    }

    fn function_statement(&mut self, start: u32) -> ParseResult<Stmt> {
        let name = self.expect_identifier("expected function name")?;
        let scoped_name = scoped_name(&name);
        self.expect_simple(TokenKind::LeftParen, "expected '(' after function name")?;
        let mut parameters = Vec::new();
        while !self.check_simple(&TokenKind::RightParen) && !self.at_end() {
            let token = self.current().clone();
            let parameter_name = self.expect_identifier("expected parameter name")?;
            let default = if self.consume_operator(Operator::Assign) {
                Some(self.expression(0)?)
            } else {
                None
            };
            parameters.push(Parameter {
                name: parameter_name,
                default,
                span: token.span,
            });
            if !self.consume_simple(TokenKind::Comma) {
                break;
            }
        }
        self.expect_simple(TokenKind::RightParen, "expected ')' after parameters")?;
        let mut attributes = FunctionAttributes::default();
        while let TokenKind::Identifier(attribute) = &self.current().kind {
            match attribute.as_str() {
                "abort" => attributes.abort = true,
                "closure" => attributes.closure = true,
                "dict" => attributes.dict = true,
                "range" => attributes.range = true,
                "async" => attributes.asynchronous = true,
                _ => break,
            }
            self.advance();
        }
        self.require_statement_end()?;
        self.skip_separators();
        self.function_depth += 1;
        let body = self.block_until(&[Keyword::EndFunction]);
        self.function_depth -= 1;
        let body = body?;
        let end = self
            .expect_keyword(Keyword::EndFunction, "expected endfunction")?
            .span
            .end;
        self.require_statement_end()?;
        Ok(self.stmt(
            start,
            end,
            StmtKind::Function(FunctionDecl {
                name: scoped_name,
                parameters,
                varargs: None,
                body,
                attributes,
            }),
        ))
    }

    fn block_until(&mut self, terminators: &[Keyword]) -> ParseResult<Vec<Stmt>> {
        let mut statements = Vec::new();
        while !self.at_end() && !terminators.iter().any(|keyword| self.at_keyword(*keyword)) {
            match self.statement() {
                Ok(statement) => statements.push(statement),
                Err(()) => self.synchronize(),
            }
            self.skip_separators();
        }
        if self.at_end() {
            self.error_here("P031", "unterminated block");
            return Err(());
        }
        Ok(statements)
    }

    pub fn parse_expression(mut self) -> (Option<Expr>, Vec<Diagnostic>) {
        let expression = self.expression(0).ok();
        if expression.is_some() && !self.at_end() && !self.at_statement_end() {
            self.error_here("P005", "unexpected token after expression");
        }
        (expression, self.diagnostics)
    }

    fn expression(&mut self, min_binding_power: u8) -> ParseResult<Expr> {
        self.skip_continuations();
        let mut left = self.prefix()?;
        loop {
            self.skip_continuations();
            if let Some(postfix) = self.postfix(left.clone())? {
                left = postfix;
                continue;
            }
            if self.check_simple(&TokenKind::Question) && min_binding_power == 0 {
                self.advance();
                let then_expr = self.expression(0)?;
                self.expect_simple(TokenKind::Colon, "expected ':' in ternary expression")?;
                let else_expr = self.expression(0)?;
                let span = left.span.merge(else_expr.span);
                left = self.expr(
                    span,
                    ExprKind::Ternary {
                        condition: Box::new(left),
                        then_expr: Box::new(then_expr),
                        else_expr: Box::new(else_expr),
                    },
                );
                continue;
            }
            let Some((operator, left_bp, right_bp)) = binary_operator(&self.current().kind) else {
                break;
            };
            if left_bp < min_binding_power {
                break;
            }
            self.advance();
            let right = self.expression(right_bp)?;
            let span = left.span.merge(right.span);
            left = self.expr(
                span,
                ExprKind::Binary {
                    left: Box::new(left),
                    operator,
                    right: Box::new(right),
                },
            );
        }
        Ok(left)
    }

    fn prefix(&mut self) -> ParseResult<Expr> {
        let token = self.current().clone();
        self.advance();
        let expr = match token.kind {
            TokenKind::Integer(value) => {
                self.expr(token.span, ExprKind::Literal(Literal::Integer(value)))
            }
            TokenKind::Float(value) => {
                self.expr(token.span, ExprKind::Literal(Literal::Float(value)))
            }
            TokenKind::SingleQuotedString(value) | TokenKind::DoubleQuotedString(value) => {
                self.expr(token.span, ExprKind::Literal(Literal::String(value)))
            }
            TokenKind::Heredoc { content, .. } => {
                self.expr(token.span, ExprKind::Literal(Literal::String(content)))
            }
            TokenKind::Keyword(Keyword::True) => {
                self.expr(token.span, ExprKind::Literal(Literal::Bool(true)))
            }
            TokenKind::Keyword(Keyword::False) => {
                self.expr(token.span, ExprKind::Literal(Literal::Bool(false)))
            }
            TokenKind::Keyword(Keyword::Null) => {
                self.expr(token.span, ExprKind::Literal(Literal::Null))
            }
            TokenKind::Identifier(name) => {
                self.expr(token.span, ExprKind::Variable(scoped_name(&name)))
            }
            TokenKind::Ampersand => {
                let option = self.current().clone();
                let TokenKind::Identifier(raw) = option.kind else {
                    self.error_here("P041", "expected an option name after '&'");
                    return Err(());
                };
                self.advance();
                let (scope, name) = if let Some(name) = raw.strip_prefix("l:") {
                    (OptionScope::Local, name)
                } else if let Some(name) = raw.strip_prefix("g:") {
                    (OptionScope::Global, name)
                } else {
                    (OptionScope::Unqualified, raw.as_str())
                };
                if name.is_empty() || name.contains(':') {
                    self.error_here("P041", "invalid option scope or name");
                    return Err(());
                }
                self.expr(
                    token.span.merge(option.span),
                    ExprKind::Option(OptionName {
                        scope,
                        name: name.to_owned(),
                    }),
                )
            }
            TokenKind::Operator(Operator::Subtract) => {
                let operand = self.expression(80)?;
                let span = token.span.merge(operand.span);
                self.expr(
                    span,
                    ExprKind::Unary {
                        operator: UnaryOperator::Negate,
                        operand: Box::new(operand),
                    },
                )
            }
            TokenKind::Operator(Operator::Add) => {
                let operand = self.expression(80)?;
                let span = token.span.merge(operand.span);
                self.expr(
                    span,
                    ExprKind::Unary {
                        operator: UnaryOperator::Positive,
                        operand: Box::new(operand),
                    },
                )
            }
            TokenKind::Operator(Operator::Not) => {
                let operand = self.expression(80)?;
                let span = token.span.merge(operand.span);
                self.expr(
                    span,
                    ExprKind::Unary {
                        operator: UnaryOperator::Not,
                        operand: Box::new(operand),
                    },
                )
            }
            TokenKind::Keyword(Keyword::Await) => {
                let operand = self.expression(80)?;
                let span = token.span.merge(operand.span);
                self.expr(span, ExprKind::Await(Box::new(operand)))
            }
            TokenKind::LeftParen => {
                let inner = self.expression(0)?;
                let end = self
                    .expect_simple(TokenKind::RightParen, "expected ')'")?
                    .span;
                Expr {
                    span: token.span.merge(end),
                    ..inner
                }
            }
            TokenKind::LeftBracket => return self.list(token.span),
            TokenKind::LeftBrace => return self.dictionary(token.span),
            _ => {
                self.diagnostics
                    .push(Diagnostic::error("P001", "expected expression", token.span));
                return Err(());
            }
        };
        Ok(expr)
    }

    fn postfix(&mut self, target: Expr) -> ParseResult<Option<Expr>> {
        if self.consume_simple(TokenKind::LeftParen) {
            let mut arguments = Vec::new();
            while !self.check_simple(&TokenKind::RightParen) && !self.at_end() {
                arguments.push(self.expression(0)?);
                if !self.consume_simple(TokenKind::Comma) {
                    break;
                }
            }
            let end = self
                .expect_simple(TokenKind::RightParen, "expected ')' after arguments")?
                .span;
            let span = target.span.merge(end);
            return Ok(Some(self.expr(
                span,
                ExprKind::Call {
                    callee: Box::new(target),
                    arguments,
                },
            )));
        }
        if self.consume_simple(TokenKind::LeftBracket) {
            let start = if self.check_simple(&TokenKind::Colon) {
                None
            } else {
                Some(Box::new(self.expression(0)?))
            };
            if self.consume_simple(TokenKind::Colon) {
                let end_expr = if self.check_simple(&TokenKind::RightBracket) {
                    None
                } else {
                    Some(Box::new(self.expression(0)?))
                };
                let end = self
                    .expect_simple(TokenKind::RightBracket, "expected ']' after slice")?
                    .span;
                let span = target.span.merge(end);
                return Ok(Some(self.expr(
                    span,
                    ExprKind::Slice {
                        target: Box::new(target),
                        start,
                        end: end_expr,
                    },
                )));
            }
            let Some(index) = start else {
                self.error_here("P007", "expected index expression");
                return Err(());
            };
            let end = self
                .expect_simple(TokenKind::RightBracket, "expected ']' after index")?
                .span;
            let span = target.span.merge(end);
            return Ok(Some(self.expr(
                span,
                ExprKind::Index {
                    target: Box::new(target),
                    index,
                },
            )));
        }
        if self.consume_simple(TokenKind::Dot) {
            let name = self.expect_identifier("expected member name after '.'")?;
            let end = self.previous().span;
            let span = target.span.merge(end);
            return Ok(Some(self.expr(
                span,
                ExprKind::Member {
                    target: Box::new(target),
                    name,
                },
            )));
        }
        Ok(None)
    }

    fn list(&mut self, opening: Span) -> ParseResult<Expr> {
        let mut values = Vec::new();
        while !self.check_simple(&TokenKind::RightBracket) && !self.at_end() {
            values.push(self.expression(0)?);
            if !self.consume_simple(TokenKind::Comma) {
                break;
            }
            if self.check_simple(&TokenKind::RightBracket) {
                break;
            }
        }
        let end = self
            .expect_simple(TokenKind::RightBracket, "expected ']' after list")?
            .span;
        Ok(self.expr(opening.merge(end), ExprKind::List(values)))
    }

    fn dictionary(&mut self, opening: Span) -> ParseResult<Expr> {
        let mut entries = Vec::new();
        while !self.check_simple(&TokenKind::RightBrace) && !self.at_end() {
            let key = self.expression(1)?;
            self.expect_simple(
                TokenKind::Colon,
                "expected ':' between dictionary key and value",
            )?;
            let value = self.expression(0)?;
            entries.push(DictEntry { key, value });
            if !self.consume_simple(TokenKind::Comma) {
                break;
            }
            if self.check_simple(&TokenKind::RightBrace) {
                break;
            }
        }
        let end = self
            .expect_simple(TokenKind::RightBrace, "expected '}' after dictionary")?
            .span;
        Ok(self.expr(opening.merge(end), ExprKind::Dictionary(entries)))
    }

    fn ex_command(&mut self) -> ParseResult<ExCommand> {
        if let Some(source) = self.source {
            let offset = self.current().span.start as usize;
            let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
            let line_end = source[offset..]
                .find(['\r', '\n'])
                .map_or(source.len(), |index| offset + index);
            let parsed = match ExLineParser::new(
                self.current().span.source,
                &source[line_start..line_end],
                line_start,
            )
            .parse()
            {
                Ok(parsed) => parsed,
                Err(diagnostic) => {
                    self.diagnostics.push(*diagnostic);
                    return Err(());
                }
            };
            let stop = parsed.next_command.map_or(line_end, |span| {
                source[line_start..span.start as usize]
                    .rfind('|')
                    .map_or(span.start as usize, |index| line_start + index)
            });
            while !self.at_end() && self.current().span.start < stop as u32 {
                self.advance();
            }
            return Ok(parsed.command);
        }
        let name = self.expect_identifier("expected Ex command name")?;
        let bang = self.consume_operator(Operator::Not);
        let mut parts = Vec::new();
        while !self.at_statement_end() {
            parts.push(token_text(&self.current().kind));
            self.advance();
        }
        Ok(ExCommand {
            modifiers: Vec::new(),
            range: None,
            name,
            bang,
            count: None,
            register: None,
            arguments: parts.join(" "),
        })
    }

    fn looks_like_source_ex_command(&self) -> bool {
        let Some(next) = self.tokens.get(self.cursor + 1) else {
            return true;
        };
        !matches!(
            next.kind,
            TokenKind::LeftParen
                | TokenKind::LeftBracket
                | TokenKind::Dot
                | TokenKind::Operator(
                    Operator::Assign
                        | Operator::AddAssign
                        | Operator::SubtractAssign
                        | Operator::MultiplyAssign
                        | Operator::DivideAssign
                        | Operator::RemainderAssign
                        | Operator::ConcatenateAssign
                        | Operator::Add
                        | Operator::Subtract
                        | Operator::Multiply
                        | Operator::Divide
                        | Operator::Remainder
                        | Operator::Concatenate
                        | Operator::Equal
                        | Operator::NotEqual
                        | Operator::Match
                        | Operator::NoMatch
                        | Operator::LogicalAnd
                        | Operator::LogicalOr
                        | Operator::Coalesce
                )
        )
    }

    fn looks_like_ex_command(&self) -> bool {
        let Some(next) = self.tokens.get(self.cursor + 1) else {
            return false;
        };
        !matches!(
            next.kind,
            TokenKind::Newline
                | TokenKind::Semicolon
                | TokenKind::EndOfFile
                | TokenKind::LeftParen
                | TokenKind::LeftBracket
                | TokenKind::Dot
                | TokenKind::Operator(
                    Operator::Add
                        | Operator::Subtract
                        | Operator::Multiply
                        | Operator::Divide
                        | Operator::Remainder
                        | Operator::Concatenate
                        | Operator::Equal
                        | Operator::NotEqual
                        | Operator::Match
                        | Operator::NoMatch
                        | Operator::Less
                        | Operator::LessEqual
                        | Operator::Greater
                        | Operator::GreaterEqual
                        | Operator::LogicalAnd
                        | Operator::LogicalOr
                        | Operator::Coalesce
                )
        )
    }

    fn looks_like_assignment(&self) -> bool {
        self.tokens.get(self.cursor + 1).is_some_and(|token| {
            matches!(
                token.kind,
                TokenKind::Operator(
                    Operator::Assign
                        | Operator::AddAssign
                        | Operator::SubtractAssign
                        | Operator::MultiplyAssign
                        | Operator::DivideAssign
                        | Operator::RemainderAssign
                        | Operator::ConcatenateAssign
                )
            )
        })
    }
    fn require_statement_end(&mut self) -> ParseResult<()> {
        if self.at_statement_end() {
            Ok(())
        } else {
            self.error_here("P006", "expected end of statement");
            Err(())
        }
    }
    fn at_statement_end(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Semicolon | TokenKind::EndOfFile
        )
    }
    fn skip_separators(&mut self) {
        while matches!(
            self.current().kind,
            TokenKind::Newline | TokenKind::Semicolon | TokenKind::LineContinuation
        ) {
            self.advance();
        }
    }
    fn skip_continuations(&mut self) {
        while matches!(self.current().kind, TokenKind::LineContinuation) {
            self.advance();
        }
    }
    fn synchronize(&mut self) {
        while !self.at_end() && !self.at_statement_end() {
            self.advance();
        }
    }
    fn at_end(&self) -> bool {
        matches!(self.current().kind, TokenKind::EndOfFile)
    }
    fn at_keyword(&self, keyword: Keyword) -> bool {
        self.current().kind == TokenKind::Keyword(keyword)
    }
    fn consume_operator(&mut self, operator: Operator) -> bool {
        if self.current().kind == TokenKind::Operator(operator) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn consume_simple(&mut self, kind: TokenKind) -> bool {
        if self.check_simple(&kind) {
            self.advance();
            true
        } else {
            false
        }
    }
    fn check_simple(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }
    fn expect_simple(&mut self, kind: TokenKind, message: &str) -> ParseResult<Token> {
        if self.check_simple(&kind) {
            Ok(self.advance().clone())
        } else {
            self.error_here("P010", message);
            Err(())
        }
    }
    fn expect_keyword(&mut self, keyword: Keyword, message: &str) -> ParseResult<Token> {
        if self.at_keyword(keyword) {
            Ok(self.advance().clone())
        } else {
            self.error_here("P011", message);
            Err(())
        }
    }
    fn expect_identifier(&mut self, message: &str) -> ParseResult<String> {
        if let TokenKind::Identifier(name) = self.current().kind.clone() {
            self.advance();
            Ok(name)
        } else {
            self.error_here("P012", message);
            Err(())
        }
    }
    fn error_here(&mut self, code: &str, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::error(code, message, self.current().span));
    }
    fn current(&self) -> &Token {
        self.tokens.get(self.cursor).unwrap_or_else(|| {
            self.tokens
                .last()
                .expect("parser requires at least an EOF token")
        })
    }
    fn previous(&self) -> &Token {
        self.tokens
            .get(self.cursor.saturating_sub(1))
            .unwrap_or_else(|| self.current())
    }
    fn advance(&mut self) -> &Token {
        if !self.at_end() {
            self.cursor += 1;
        }
        self.previous()
    }
    fn node_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }
    fn expr(&mut self, span: Span, kind: ExprKind) -> Expr {
        Expr {
            id: self.node_id(),
            span,
            kind,
        }
    }
    fn stmt(&mut self, start: u32, end: u32, kind: StmtKind) -> Stmt {
        Stmt {
            id: self.node_id(),
            span: Span::new(self.current().span.source, start, end),
            kind,
        }
    }
}

fn token_text(kind: &TokenKind) -> String {
    match kind {
        TokenKind::Identifier(value) => value.clone(),
        TokenKind::Integer(value) => value.to_string(),
        TokenKind::Float(value) => value.to_string(),
        TokenKind::SingleQuotedString(value) => format!("'{value}'"),
        TokenKind::DoubleQuotedString(value) => format!("\"{value}\""),
        TokenKind::Keyword(value) => format!("{value:?}").to_ascii_lowercase(),
        TokenKind::Operator(value) => format!("{value:?}"),
        TokenKind::LeftParen => "(".into(),
        TokenKind::RightParen => ")".into(),
        TokenKind::LeftBracket => "[".into(),
        TokenKind::RightBracket => "]".into(),
        TokenKind::LeftBrace => "{".into(),
        TokenKind::RightBrace => "}".into(),
        TokenKind::Comma => ",".into(),
        TokenKind::Colon => ":".into(),
        TokenKind::Question => "?".into(),
        TokenKind::Ampersand => "&".into(),
        TokenKind::Dot => ".".into(),
        TokenKind::Heredoc { content, .. } => content.clone(),
        TokenKind::Semicolon
        | TokenKind::Newline
        | TokenKind::LineContinuation
        | TokenKind::EndOfFile => String::new(),
    }
}

fn scoped_name(text: &str) -> ScopedName {
    let (scope, name) =
        text.split_once(':')
            .map_or((Scope::Unqualified, text), |(prefix, name)| {
                (
                    match prefix {
                        "g" => Scope::Global,
                        "l" => Scope::Local,
                        "s" => Scope::Script,
                        "a" => Scope::Argument,
                        "b" => Scope::Buffer,
                        "w" => Scope::Window,
                        "t" => Scope::Tab,
                        "v" => Scope::Vim,
                        _ => Scope::Unqualified,
                    },
                    name,
                )
            });
    ScopedName {
        scope,
        name: name.to_owned(),
    }
}

fn binary_operator(kind: &TokenKind) -> Option<(BinaryOperator, u8, u8)> {
    let operator = match kind {
        TokenKind::Operator(Operator::LogicalOr) => (BinaryOperator::LogicalOr, 10),
        TokenKind::Operator(Operator::LogicalAnd) => (BinaryOperator::LogicalAnd, 20),
        TokenKind::Operator(Operator::Coalesce) => (BinaryOperator::Coalesce, 25),
        TokenKind::Operator(
            Operator::Equal | Operator::CaseSensitive | Operator::CaseInsensitive,
        ) => (BinaryOperator::Equal, 30),
        TokenKind::Operator(Operator::NotEqual) => (BinaryOperator::NotEqual, 30),
        TokenKind::Operator(Operator::Match) => (BinaryOperator::Match, 30),
        TokenKind::Operator(Operator::NoMatch) => (BinaryOperator::NoMatch, 30),
        TokenKind::Operator(Operator::Less) => (BinaryOperator::Less, 30),
        TokenKind::Operator(Operator::LessEqual) => (BinaryOperator::LessEqual, 30),
        TokenKind::Operator(Operator::Greater) => (BinaryOperator::Greater, 30),
        TokenKind::Operator(Operator::GreaterEqual) => (BinaryOperator::GreaterEqual, 30),
        TokenKind::Keyword(Keyword::Is) => (BinaryOperator::Is, 30),
        TokenKind::Keyword(Keyword::IsNot) => (BinaryOperator::IsNot, 30),
        TokenKind::Operator(Operator::Add) => (BinaryOperator::Add, 40),
        TokenKind::Operator(Operator::Subtract) => (BinaryOperator::Subtract, 40),
        TokenKind::Operator(Operator::Concatenate) => (BinaryOperator::Concatenate, 40),
        TokenKind::Operator(Operator::Multiply) => (BinaryOperator::Multiply, 50),
        TokenKind::Operator(Operator::Divide) => (BinaryOperator::Divide, 50),
        TokenKind::Operator(Operator::Remainder) => (BinaryOperator::Remainder, 50),
        _ => return None,
    };
    Some((operator.0, operator.1, operator.1 + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    fn parse(source: &str) -> ParseOutput {
        let tokens = Lexer::new(SourceId(0), source).lex();
        assert!(
            tokens.diagnostics.is_empty(),
            "lexer diagnostics: {:?}",
            tokens.diagnostics
        );
        Parser::new(&tokens.tokens).parse()
    }

    #[test]
    fn observes_expression_precedence_and_postfix() {
        let output = parse("let g:x = add(1, 2 * 3)[0]\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let StmtKind::Assignment(assignment) = &output.program.unwrap().statements[0].kind else {
            panic!()
        };
        assert!(matches!(assignment.value.kind, ExprKind::Index { .. }));
    }

    #[test]
    fn parses_collections_and_ternary() {
        let output = parse("let x = v:true ? {'a': [1, 2]} : {}\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let StmtKind::Assignment(assignment) = &output.program.unwrap().statements[0].kind else {
            panic!()
        };
        assert!(matches!(assignment.value.kind, ExprKind::Ternary { .. }));
    }

    #[test]
    fn parses_nested_control_flow() {
        let output = parse(
            "if g:ready\n  for item in [1, 2]\n    echo item\n  endfor\nelse\n  throw 'bad'\nendif\n",
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(matches!(
            output.program.unwrap().statements[0].kind,
            StmtKind::If(_)
        ));
    }

    #[test]
    fn parses_functions_and_try_blocks() {
        let output = parse(
            "function s:Work(x, fallback = 1) abort\ntry\nreturn x + fallback\ncatch 'failure'\nreturn 0\nfinally\necho 'done'\nendtry\nendfunction\n",
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert!(matches!(
            output.program.unwrap().statements[0].kind,
            StmtKind::Function(_)
        ));
    }

    #[test]
    fn source_aware_ex_commands_preserve_raw_syntax_and_bars() {
        let source = ":silent! 1,2write! file name.txt | echo \"done\"\nnnoremap <silent> <leader>w :write<CR>\n";
        let lexed = Lexer::new(SourceId(0), source).lex();
        assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);
        let output = Parser::new_with_source(&lexed.tokens, source).parse();
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let program = output.program.unwrap();
        let StmtKind::ExCommand(write) = &program.statements[0].kind else {
            panic!("expected write command")
        };
        assert_eq!(write.name, "write");
        assert!(write.bang);
        assert_eq!(write.arguments, "file name.txt");
        assert!(matches!(program.statements[1].kind, StmtKind::Echo(_)));
        let StmtKind::ExCommand(mapping) = &program.statements[2].kind else {
            panic!("expected mapping command")
        };
        assert_eq!(mapping.name, "nnoremap");
        assert_eq!(mapping.arguments, "<silent> <leader>w :write<CR>");
    }

    #[test]
    fn parses_generic_ex_commands() {
        let output = parse(":set number\ncall plug#run()\nfinish\n");
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let statements = &output.program.unwrap().statements;
        assert!(matches!(statements[0].kind, StmtKind::ExCommand(_)));
        assert!(matches!(statements[1].kind, StmtKind::ExCommand(_)));
        assert!(matches!(statements[2].kind, StmtKind::Finish));
    }

    #[test]
    fn recovers_after_a_bad_statement() {
        let output = parse("let = 1\nlet good = 2\n");
        assert!(!output.diagnostics.is_empty());
        assert_eq!(output.program.unwrap().statements.len(), 1);
    }
}
