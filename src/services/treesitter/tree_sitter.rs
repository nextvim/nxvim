use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::sync::{Arc, Mutex, OnceLock};

use text::BufferSnapshot;
use tree_sitter::{
    LanguageError, Node, Parser, Point, Query, QueryCursor, QueryError, StreamingIterator, Tree,
};

use super::grammars::Grammar;

/// Named declaration-like nodes that form meaningful editor scopes.
pub const SCOPE_KINDS: &[&str] = &[
    // Rust
    "function_item",
    "impl_item",
    "trait_item",
    "mod_item",
    "struct_item",
    "enum_item",
    "union_item",
    "type_item",
    "const_item",
    "static_item",
    "macro_definition",
    "macro_invocation",
    "closure_expression",
    // Bash
    "function_definition",
    // C
    "struct_specifier",
    "union_specifier",
    "enum_specifier",
    "type_definition",
    // Go
    "function_declaration",
    "method_declaration",
    "type_declaration",
    "type_spec",
    "struct_type",
    "interface_type",
    "func_literal",
    // Python
    "class_definition",
    "lambda",
    // JavaScript and TypeScript
    "generator_function_declaration",
    "function_expression",
    "generator_function",
    "method_definition",
    "abstract_method_signature",
    "method_signature",
    "class_declaration",
    "abstract_class_declaration",
    "class",
    "interface_declaration",
    "type_alias_declaration",
    "enum_declaration",
    "namespace_declaration",
    "module_declaration",
    "internal_module",
    "ambient_declaration",
    "arrow_function",
    // HTML and CSS
    "element",
    "script_element",
    "style_element",
    "rule_set",
    "media_statement",
    "supports_statement",
    "keyframes_statement",
    "keyframe_block",
];

pub const FUNCTION_KINDS: &[&str] = &[
    "function_item",
    "function_definition",
    "function_declaration",
    "generator_function_declaration",
    "function_expression",
    "generator_function",
    "method_declaration",
    "method_definition",
    "abstract_method_signature",
    "method_signature",
    "func_literal",
    "closure_expression",
    "arrow_function",
];

pub const CLASS_KINDS: &[&str] = &[
    "class_definition",
    "class_declaration",
    "abstract_class_declaration",
    "class",
    "interface_declaration",
    "struct_item",
    "enum_item",
    "union_item",
    "trait_item",
    "impl_item",
    "struct_specifier",
    "union_specifier",
    "enum_specifier",
    "type_declaration",
    "struct_type",
    "interface_type",
];

pub const ARGUMENT_CONTAINER_KINDS: &[&str] = &[
    "arguments",
    "argument_list",
    "parameters",
    "parameter_list",
    "formal_parameters",
    "type_arguments",
    "type_parameters",
];

/// Control-flow and container nodes useful for structural block navigation.
pub const BLOCK_KINDS: &[&str] = &[
    // Generic roots and blocks
    "source_file",
    "program",
    "module",
    "document",
    "stylesheet",
    "translation_unit",
    "block",
    "statement_block",
    "compound_statement",
    "declaration_list",
    "field_declaration_list",
    "class_body",
    // Rust
    "async_block",
    "unsafe_block",
    "match_expression",
    "match_arm",
    "if_expression",
    "while_expression",
    "loop_expression",
    "for_expression",
    "token_tree",
    "enum_variant_list",
    // Bash
    "compound_statement",
    "subshell",
    "if_statement",
    "elif_clause",
    "else_clause",
    "for_statement",
    "c_style_for_statement",
    "while_statement",
    "until_statement",
    "case_statement",
    "case_item",
    "pipeline",
    "command_substitution",
    "process_substitution",
    // C and C-like control flow
    "if_statement",
    "switch_statement",
    "case_statement",
    "for_statement",
    "for_in_statement",
    "for_of_statement",
    "while_statement",
    "do_statement",
    "labeled_statement",
    // Go
    "expression_switch_statement",
    "type_switch_statement",
    "select_statement",
    "communication_case",
    "expression_case",
    "default_case",
    "composite_literal",
    "literal_value",
    // Python
    "elif_clause",
    "else_clause",
    "try_statement",
    "except_clause",
    "finally_clause",
    "with_statement",
    "match_statement",
    "case_clause",
    // JavaScript and TypeScript
    "switch_body",
    "switch_case",
    "switch_default",
    "try_statement",
    "catch_clause",
    "finally_clause",
    "object",
    "object_pattern",
    "object_type",
    "interface_body",
    "enum_body",
    "namespace_body",
    "module",
    "export_clause",
    "named_imports",
    "named_exports",
    // HTML and CSS
    "element",
    "script_element",
    "style_element",
    "rule_set",
    "media_statement",
    "supports_statement",
    "keyframes_statement",
    "keyframe_block",
    // JSON and expression containers
    "object",
    "array",
    "pair",
    "list",
    "dictionary",
    "set",
    "tuple",
    "array_expression",
    "array_pattern",
    "tuple_expression",
    "tuple_type",
    "parenthesized_expression",
    "parenthesized_type",
    "arguments",
    "argument_list",
    "parameters",
    "parameter_list",
    "formal_parameters",
    "jsx_element",
    "jsx_fragment",
];

/// Anonymous delimiter tokens recognized as structural boundaries.
pub const OPEN_DELIMITERS: &[&str] = &[
    "{",
    "(",
    "[",
    "<",
    "\"",
    "'",
    "`",
    "start_tag",
    "jsx_opening_element",
];
pub const CLOSE_DELIMITERS: &[&str] = &[
    "}",
    ")",
    "]",
    ">",
    "\"",
    "'",
    "`",
    "end_tag",
    "jsx_closing_element",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxNode {
    pub kind: String,
    pub named: bool,
    pub byte_range: Range<usize>,
    pub start_position: Point,
    pub end_position: Point,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryCapture {
    pub name: String,
    pub node: SyntaxNode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeInfo {
    pub kind: String,
    pub name: Option<String>,
    pub byte_range: Range<usize>,
}

#[derive(Clone)]
pub struct SyntaxTree {
    grammar: Grammar,
    tree: Tree,
    scope_cache: Arc<Mutex<HashMap<usize, Vec<SyntaxNode>>>>,
    block_cache: Arc<OnceLock<Vec<SyntaxNode>>>,
    node_cache: Arc<OnceLock<Vec<SyntaxNode>>>,
    argument_cache: Arc<OnceLock<Vec<SyntaxNode>>>,
}

impl SyntaxTree {
    pub fn grammar(&self) -> Grammar {
        self.grammar
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    pub fn root_kind(&self) -> &str {
        self.tree.root_node().kind()
    }

    pub fn node_at_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.descendant_at_byte(byte, false).map(Self::node_info)
    }

    pub fn named_node_at_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.descendant_at_byte(byte, true).map(Self::node_info)
    }

    pub fn parent_at_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.descendant_at_byte(byte, true)
            .and_then(|node| node.parent())
            .map(Self::node_info)
    }

    pub fn first_named_child_at_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.descendant_at_byte(byte, true)
            .and_then(|node| node.named_child(0))
            .map(Self::node_info)
    }

    pub fn next_named_sibling_at_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.descendant_at_byte(byte, true)
            .and_then(|node| node.next_named_sibling())
            .map(Self::node_info)
    }

    pub fn previous_named_sibling_at_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.descendant_at_byte(byte, true)
            .and_then(|node| node.prev_named_sibling())
            .map(Self::node_info)
    }

    pub fn scope_path_at_byte(&self, byte: usize) -> Vec<SyntaxNode> {
        if let Some(cached) = self.scope_cache.lock().unwrap().get(&byte).cloned() {
            return cached;
        }

        let Some(mut node) = self.descendant_at_byte(byte, true) else {
            return Vec::new();
        };
        let mut path = Vec::new();
        loop {
            path.push(Self::node_info(node));
            let Some(parent) = node.parent() else {
                break;
            };
            node = parent;
        }
        path.reverse();
        self.scope_cache.lock().unwrap().insert(byte, path.clone());
        path
    }

    pub fn block_path_at_byte(&self, byte: usize) -> Vec<SyntaxNode> {
        let Some(mut node) = self.descendant_at_byte(byte, true) else {
            return Vec::new();
        };
        let mut path = Vec::new();
        loop {
            if Self::is_block_node(node) {
                path.push(Self::node_info(node));
            }
            let Some(parent) = node.parent() else {
                break;
            };
            node = parent;
        }
        path.reverse();
        path
    }

    pub fn enclosing_block_at_byte(&self, byte: usize) -> Option<SyntaxNode> {
        let mut node = self.descendant_at_byte(byte, true)?;
        loop {
            if Self::is_block_node(node) {
                return Some(Self::node_info(node));
            }
            node = node.parent()?;
        }
    }

    pub fn next_block_after_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.blocks()
            .iter()
            .find(|node| node.byte_range.start > byte)
            .cloned()
    }

    pub fn previous_block_before_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.blocks()
            .iter()
            .rev()
            .find(|node| node.byte_range.end <= byte)
            .cloned()
    }

    pub fn block_start_at_byte(&self, byte: usize) -> Option<SyntaxNode> {
        let mut node = self.descendant_at_byte(byte, true)?;
        loop {
            if Self::is_block_node(node) {
                let info = Self::node_info(node);
                if info.byte_range.start < byte {
                    return Some(info);
                }
            }
            node = node.parent()?;
        }
    }

    pub fn block_end_at_byte(&self, byte: usize) -> Option<SyntaxNode> {
        let mut node = self.descendant_at_byte(byte, true)?;
        loop {
            if Self::is_block_node(node) {
                let info = Self::node_info(node);
                if info.byte_range.end > byte + 1 {
                    return Some(info);
                }
            }
            node = node.parent()?;
        }
    }

    pub fn next_function_after_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.next_node_by_kinds(byte, FUNCTION_KINDS)
    }

    pub fn previous_function_before_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.previous_node_by_kinds(byte, FUNCTION_KINDS)
    }

    pub fn next_class_after_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.next_node_by_kinds(byte, CLASS_KINDS)
    }

    pub fn previous_class_before_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.previous_node_by_kinds(byte, CLASS_KINDS)
    }

    pub fn next_argument_after_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.arguments()
            .iter()
            .find(|node| node.byte_range.start > byte)
            .cloned()
    }

    pub fn previous_argument_before_byte(&self, byte: usize) -> Option<SyntaxNode> {
        self.arguments()
            .iter()
            .rev()
            .find(|node| node.byte_range.end <= byte)
            .cloned()
    }

    pub fn delimiter_boundaries_at_byte(&self, byte: usize) -> Option<(SyntaxNode, SyntaxNode)> {
        let mut node = self.descendant_at_byte(byte, false)?;
        loop {
            if let Some(boundaries) = Self::delimiter_boundaries(node) {
                return Some((Self::node_info(boundaries.0), Self::node_info(boundaries.1)));
            }
            node = node.parent()?;
        }
    }

    pub fn blocks(&self) -> &[SyntaxNode] {
        self.block_cache.get_or_init(|| {
            let mut blocks = Vec::new();
            let mut cursor = self.tree.walk();

            loop {
                let node = cursor.node();
                if node.is_named() && Self::is_block_node(node) {
                    blocks.push(Self::node_info(node));
                }

                if cursor.goto_first_child() {
                    continue;
                }

                loop {
                    if cursor.goto_next_sibling() {
                        break;
                    }
                    if !cursor.goto_parent() {
                        blocks.sort_by_key(|node| (node.byte_range.start, node.byte_range.end));
                        return blocks;
                    }
                }
            }
        })
    }

    pub fn named_nodes(&self) -> &[SyntaxNode] {
        self.node_cache
            .get_or_init(|| self.collect_named_nodes(|_| true))
    }

    pub fn arguments(&self) -> &[SyntaxNode] {
        self.argument_cache.get_or_init(|| {
            self.collect_named_nodes(|node| {
                node.parent()
                    .is_some_and(|parent| ARGUMENT_CONTAINER_KINDS.contains(&parent.kind()))
            })
        })
    }

    pub fn current_scope(&self, source: &BufferSnapshot, byte: usize) -> Option<ScopeInfo> {
        let mut node = self.descendant_at_byte(byte, true)?;
        loop {
            if SCOPE_KINDS.contains(&node.kind()) {
                let name = node
                    .child_by_field_name("name")
                    .map(|name| Self::text_for_node(source, name));
                return Some(ScopeInfo {
                    kind: node.kind().to_string(),
                    name,
                    byte_range: node.byte_range(),
                });
            }
            node = node.parent()?;
        }
    }

    pub fn query(
        &self,
        source: &BufferSnapshot,
        query_source: &str,
    ) -> Result<Vec<QueryCapture>, QueryError> {
        let query = Query::new(&self.grammar.language(), query_source)?;
        let source_text: String = source
            .as_rope()
            .chunks_in_range(0..source.as_rope().len())
            .collect();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, self.tree.root_node(), source_text.as_bytes());
        let capture_names = query.capture_names();
        let mut captures = Vec::new();

        while let Some(query_match) = matches.next() {
            for capture in query_match.captures {
                captures.push(QueryCapture {
                    name: capture_names[capture.index as usize].to_string(),
                    node: Self::node_info(capture.node),
                });
            }
        }

        Ok(captures)
    }

    fn next_node_by_kinds(&self, byte: usize, kinds: &[&str]) -> Option<SyntaxNode> {
        self.named_nodes()
            .iter()
            .find(|node| node.byte_range.start > byte && kinds.contains(&node.kind.as_str()))
            .cloned()
    }

    fn previous_node_by_kinds(&self, byte: usize, kinds: &[&str]) -> Option<SyntaxNode> {
        self.named_nodes()
            .iter()
            .rev()
            .find(|node| node.byte_range.end <= byte && kinds.contains(&node.kind.as_str()))
            .cloned()
    }

    fn collect_named_nodes(&self, predicate: impl Fn(Node<'_>) -> bool) -> Vec<SyntaxNode> {
        let mut nodes = Vec::new();
        let mut cursor = self.tree.walk();

        loop {
            let node = cursor.node();
            if node.is_named() && predicate(node) {
                nodes.push(Self::node_info(node));
            }
            if cursor.goto_first_child() {
                continue;
            }
            loop {
                if cursor.goto_next_sibling() {
                    break;
                }
                if !cursor.goto_parent() {
                    nodes.sort_by_key(|node| (node.byte_range.start, node.byte_range.end));
                    return nodes;
                }
            }
        }
    }

    fn descendant_at_byte(&self, byte: usize, named: bool) -> Option<Node<'_>> {
        let root = self.tree.root_node();
        if root.end_byte() == 0 {
            return Some(root);
        }
        let start = byte.min(root.end_byte().saturating_sub(1));
        let end = start.saturating_add(1).min(root.end_byte());
        if named {
            root.named_descendant_for_byte_range(start, end)
        } else {
            root.descendant_for_byte_range(start, end)
        }
    }

    pub fn is_scope_kind(kind: &str) -> bool {
        SCOPE_KINDS.contains(&kind)
    }

    pub fn is_block_kind(kind: &str) -> bool {
        SCOPE_KINDS.contains(&kind) || BLOCK_KINDS.contains(&kind)
    }

    fn is_block_node(node: Node<'_>) -> bool {
        Self::is_block_kind(node.kind()) || Self::delimiter_boundaries(node).is_some()
    }

    fn delimiter_boundaries(node: Node<'_>) -> Option<(Node<'_>, Node<'_>)> {
        let count = node.child_count();
        if count < 2 {
            return None;
        }
        let first = node.child(0)?;
        let last_idx = u32::try_from(count.checked_sub(1)?).ok()?;
        let last = node.child(last_idx)?;

        let fk = first.kind();
        let lk = last.kind();

        // 1. Standard braces, parentheses, brackets, angle brackets
        if (fk == "{" && lk == "}")
            || (fk == "(" && lk == ")")
            || (fk == "[" && lk == "]")
            || (fk == "<" && lk == ">")
        {
            return Some((first, last));
        }

        // 2. HTML/JSX Elements & Tags
        if (fk == "start_tag" && lk == "end_tag")
            || (fk == "jsx_opening_element" && lk == "jsx_closing_element")
            || (node.kind() == "element"
                || node.kind() == "jsx_element"
                || node.kind() == "jsx_fragment")
        {
            return Some((first, last));
        }

        // 3. String quotes
        if (node.kind().contains("string") || node.kind().contains("literal"))
            && ((fk == "\"" && lk == "\"") || (fk == "'" && lk == "'") || (fk == "`" && lk == "`"))
        {
            return Some((first, last));
        }

        None
    }

    fn node_info(node: Node<'_>) -> SyntaxNode {
        SyntaxNode {
            kind: node.kind().to_string(),
            named: node.is_named(),
            byte_range: node.byte_range(),
            start_position: node.start_position(),
            end_position: node.end_position(),
        }
    }

    fn text_for_node(source: &BufferSnapshot, node: Node<'_>) -> String {
        source
            .as_rope()
            .chunks_in_range(node.byte_range())
            .collect()
    }
}

impl fmt::Debug for SyntaxTree {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SyntaxTree")
            .field("grammar", &self.grammar)
            .field("root_kind", &self.root_kind())
            .field("has_error", &self.tree.root_node().has_error())
            .finish()
    }
}

#[derive(Debug)]
pub enum ParseError {
    IncompatibleLanguage(LanguageError),
    Cancelled,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IncompatibleLanguage(error) => write!(formatter, "incompatible grammar: {error}"),
            Self::Cancelled => formatter.write_str("tree-sitter parsing was cancelled"),
        }
    }
}

impl std::error::Error for ParseError {}

pub struct TreeSitterParser {
    parser: Parser,
    grammar: Grammar,
}

impl TreeSitterParser {
    pub fn new(grammar: Grammar) -> Result<Self, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&grammar.language())
            .map_err(ParseError::IncompatibleLanguage)?;
        Ok(Self { parser, grammar })
    }

    pub fn parse(
        &mut self,
        snapshot: &BufferSnapshot,
        old_tree: Option<&SyntaxTree>,
    ) -> Result<SyntaxTree, ParseError> {
        let rope = snapshot.as_rope();
        let mut chunks = rope.chunks_in_range(0..rope.len());
        let old_tree = old_tree
            .filter(|tree| tree.grammar == self.grammar)
            .map(|tree| tree.tree());

        let tree = self
            .parser
            .parse_with_options(
                &mut move |offset, _| {
                    chunks.seek(offset);
                    chunks.next().unwrap_or("").as_bytes()
                },
                old_tree,
                None,
            )
            .ok_or(ParseError::Cancelled)?;

        Ok(SyntaxTree {
            grammar: self.grammar,
            tree,
            scope_cache: Arc::new(Mutex::new(HashMap::new())),
            block_cache: Arc::new(OnceLock::new()),
            node_cache: Arc::new(OnceLock::new()),
            argument_cache: Arc::new(OnceLock::new()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clock::ReplicaId;
    use text::{Buffer, BufferId};

    #[test]
    fn parses_a_buffer_snapshot_without_flattening_it() {
        let source = "fn main() { let value = 42; }";
        let buffer = Buffer::new(ReplicaId::LOCAL, BufferId::new(1).unwrap(), source);
        let mut parser = TreeSitterParser::new(Grammar::Rust).unwrap();
        let syntax = parser.parse(&buffer.snapshot(), None).unwrap();

        assert_eq!(syntax.root_kind(), "source_file");
        assert!(!syntax.tree().root_node().has_error());

        let value_offset = source.find("value").unwrap();
        assert_eq!(
            syntax.named_node_at_byte(value_offset).unwrap().kind,
            "identifier"
        );
        assert!(syntax.parent_at_byte(value_offset).is_some());

        let scope = syntax
            .current_scope(buffer.snapshot(), value_offset)
            .expect("cursor should be inside a function scope");
        assert_eq!(scope.kind, "function_item");
        assert_eq!(scope.name.as_deref(), Some("main"));

        let first_path = syntax.scope_path_at_byte(value_offset);
        let cached_path = syntax.scope_path_at_byte(value_offset);
        assert_eq!(first_path, cached_path);
        assert_eq!(first_path.last().unwrap().kind, "identifier");

        let captures = syntax
            .query(
                buffer.snapshot(),
                "(function_item name: (identifier) @function.name)",
            )
            .unwrap();
        assert_eq!(captures.len(), 1);
        assert_eq!(captures[0].name, "function.name");
        assert_eq!(captures[0].node.kind, "identifier");

        let enclosing_block = syntax
            .enclosing_block_at_byte(value_offset)
            .expect("value should be inside a block");
        assert_eq!(enclosing_block.kind, "block");
        assert!(enclosing_block.byte_range.contains(&value_offset));

        let block_path = syntax.block_path_at_byte(value_offset);
        assert_eq!(block_path.last().unwrap().kind, "block");
        assert!(!syntax.blocks().is_empty());
        assert!(syntax.next_block_after_byte(0).is_some());

        let boundaries = syntax
            .delimiter_boundaries_at_byte(value_offset)
            .expect("function body should have brace boundaries");
        assert_eq!(boundaries.0.kind, "{");
        assert_eq!(boundaries.1.kind, "}");
    }
}
