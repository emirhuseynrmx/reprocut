//! Grammar-validated source transformations for ReproCut.

use std::{path::Path, str};

use reprocut_core::{ByteRange, Operation, ProjectPath, TransformationError};
use thiserror::Error;
use tree_sitter::{Language, Node, Parser, Tree};

/// Bundled concrete-syntax grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxLanguage {
    /// Rust source.
    Rust,
    /// Python source or stub.
    Python,
    /// JavaScript or JSX source.
    JavaScript,
    /// TypeScript source.
    TypeScript,
    /// TypeScript JSX source.
    Tsx,
}

impl SyntaxLanguage {
    /// Selects a bundled grammar from a source path.
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "rs" => Some(Self::Rust),
            "py" | "pyi" => Some(Self::Python),
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            _ => None,
        }
    }

    fn grammar(self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        }
    }
}

/// Grammar-safe transformation strategy.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SyntaxStrategy {
    /// Delete a complete allowlisted named node.
    DeleteNode,
    /// Replace a wrapper node with one of its named children.
    HoistChild,
}

/// One byte-exact transform that has already reparsed successfully in isolation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxTransform {
    kind: String,
    range: ByteRange,
    replacement: Vec<u8>,
    strategy: SyntaxStrategy,
}

impl SyntaxTransform {
    /// Returns the Tree-sitter named-node kind.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns the source byte interval replaced by this transform.
    pub const fn range(&self) -> ByteRange {
        self.range
    }

    /// Returns deletion or child-hoisting strategy.
    pub const fn strategy(&self) -> SyntaxStrategy {
        self.strategy
    }

    /// Returns replacement bytes; deletions use an empty slice.
    pub fn replacement(&self) -> &[u8] {
        &self.replacement
    }

    /// Converts this validated edit into the canonical project operation model.
    pub fn operation(&self, path: ProjectPath) -> Operation {
        Operation::replace(path, self.range, self.replacement.clone())
    }

    /// Materializes this single edit with exactly one output allocation.
    pub fn candidate_bytes(&self, source: &[u8]) -> Result<Vec<u8>, SyntaxError> {
        apply_edit(source, self.range, &self.replacement)
    }
}

/// Parses source and rejects ERROR, MISSING, invalid UTF-8, or parser failure.
pub fn parse_valid(language: SyntaxLanguage, source: &[u8]) -> Result<(), SyntaxError> {
    let tree = parse(language, source)?;
    if contains_invalid_node(&tree) {
        return Err(SyntaxError::InvalidSyntax);
    }
    Ok(())
}

/// Enumerates allowlisted named-node deletions that remain grammar-valid.
pub fn deletion_transforms(
    language: SyntaxLanguage,
    source: &[u8],
) -> Result<Vec<SyntaxTransform>, SyntaxError> {
    let mut parser = configured_parser(language)?;
    let tree = parse_with(&mut parser, source)?;
    if contains_invalid_node(&tree) {
        return Err(SyntaxError::InvalidSyntax);
    }
    let mut transforms = Vec::new();
    walk_named(tree.root_node(), |node| {
        if is_deletable(language, node.kind()) {
            maybe_push_transform(
                source,
                node,
                Vec::new(),
                SyntaxStrategy::DeleteNode,
                &mut parser,
                &mut transforms,
            );
        }
    });
    canonicalize(&mut transforms);
    Ok(transforms)
}

/// Enumerates wrapper-to-child replacements that remain grammar-valid.
pub fn hoist_transforms(
    language: SyntaxLanguage,
    source: &[u8],
) -> Result<Vec<SyntaxTransform>, SyntaxError> {
    let mut parser = configured_parser(language)?;
    let tree = parse_with(&mut parser, source)?;
    if contains_invalid_node(&tree) {
        return Err(SyntaxError::InvalidSyntax);
    }
    let mut transforms = Vec::new();
    walk_named(tree.root_node(), |node| {
        if !is_hoistable(node.kind()) {
            return;
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            let replacement = source[child.byte_range()].to_vec();
            maybe_push_transform(
                source,
                node,
                replacement,
                SyntaxStrategy::HoistChild,
                &mut parser,
                &mut transforms,
            );
        }
    });
    canonicalize(&mut transforms);
    Ok(transforms)
}

/// Invalid grammar, range, parser, or canonical-operation conversion.
#[derive(Debug, Error)]
pub enum SyntaxError {
    /// Syntax reducers require valid UTF-8 source.
    #[error("syntax source is not valid UTF-8")]
    InvalidUtf8,
    /// Tree-sitter returned no tree.
    #[error("Tree-sitter parser returned no tree")]
    ParserReturnedNoTree,
    /// Grammar ABI could not be installed in the parser.
    #[error("Tree-sitter language mismatch: {0}")]
    Language(#[from] tree_sitter::LanguageError),
    /// Source contains an ERROR or MISSING node.
    #[error("source or transformed candidate is not grammar-valid")]
    InvalidSyntax,
    /// A Tree-sitter byte interval could not fit the canonical range model.
    #[error("syntax byte range is invalid")]
    InvalidRange,
    /// Candidate edit exceeded source bytes.
    #[error("syntax edit exceeded source bytes")]
    OutOfBounds,
    /// Canonical operation construction rejected the edit.
    #[error(transparent)]
    Transformation(#[from] TransformationError),
}

fn parse(language: SyntaxLanguage, source: &[u8]) -> Result<Tree, SyntaxError> {
    let mut parser = configured_parser(language)?;
    parse_with(&mut parser, source)
}

fn configured_parser(language: SyntaxLanguage) -> Result<Parser, SyntaxError> {
    let mut parser = Parser::new();
    parser.set_language(&language.grammar())?;
    Ok(parser)
}

fn parse_with(parser: &mut Parser, source: &[u8]) -> Result<Tree, SyntaxError> {
    str::from_utf8(source).map_err(|_| SyntaxError::InvalidUtf8)?;
    parser
        .parse(source, None)
        .ok_or(SyntaxError::ParserReturnedNoTree)
}

fn contains_invalid_node(tree: &Tree) -> bool {
    if tree.root_node().has_error() {
        return true;
    }
    let mut invalid = false;
    walk_named(tree.root_node(), |node| {
        invalid |= node.is_error() || node.is_missing();
    });
    invalid
}

fn walk_named<'tree>(root: Node<'tree>, mut visit: impl FnMut(Node<'tree>)) {
    let mut cursor = root.walk();
    loop {
        let node = cursor.node();
        if node.is_named() {
            visit(node);
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

fn maybe_push_transform(
    source: &[u8],
    node: Node<'_>,
    replacement: Vec<u8>,
    strategy: SyntaxStrategy,
    parser: &mut Parser,
    transforms: &mut Vec<SyntaxTransform>,
) {
    let Ok(start) = u64::try_from(node.start_byte()) else {
        return;
    };
    let Ok(end) = u64::try_from(node.end_byte()) else {
        return;
    };
    let Ok(range) = ByteRange::new(start, end) else {
        return;
    };
    let Ok(candidate) = apply_edit(source, range, &replacement) else {
        return;
    };
    if parse_with(parser, &candidate).is_ok_and(|tree| !contains_invalid_node(&tree)) {
        transforms.push(SyntaxTransform {
            kind: node.kind().to_owned(),
            range,
            replacement,
            strategy,
        });
    }
}

fn apply_edit(source: &[u8], range: ByteRange, replacement: &[u8]) -> Result<Vec<u8>, SyntaxError> {
    let start = usize::try_from(range.start()).map_err(|_| SyntaxError::OutOfBounds)?;
    let end = usize::try_from(range.end()).map_err(|_| SyntaxError::OutOfBounds)?;
    if end > source.len() {
        return Err(SyntaxError::OutOfBounds);
    }
    let final_length = source
        .len()
        .checked_sub(end - start)
        .and_then(|length| length.checked_add(replacement.len()))
        .ok_or(SyntaxError::OutOfBounds)?;
    let mut candidate = Vec::with_capacity(final_length);
    candidate.extend_from_slice(&source[..start]);
    candidate.extend_from_slice(replacement);
    candidate.extend_from_slice(&source[end..]);
    Ok(candidate)
}

fn canonicalize(transforms: &mut Vec<SyntaxTransform>) {
    transforms.sort_unstable_by(|left, right| {
        left.range
            .cmp(&right.range)
            .then_with(|| left.strategy.cmp(&right.strategy))
            .then_with(|| left.replacement.cmp(&right.replacement))
            .then_with(|| left.kind.cmp(&right.kind))
    });
    transforms.dedup_by(|left, right| {
        left.range == right.range
            && left.strategy == right.strategy
            && left.replacement == right.replacement
    });
}

fn is_deletable(language: SyntaxLanguage, kind: &str) -> bool {
    match language {
        SyntaxLanguage::Rust => matches!(
            kind,
            "function_item"
                | "struct_item"
                | "enum_item"
                | "impl_item"
                | "trait_item"
                | "type_item"
                | "const_item"
                | "static_item"
                | "use_declaration"
                | "mod_item"
                | "let_declaration"
                | "expression_statement"
                | "match_arm"
                | "attribute_item"
        ),
        SyntaxLanguage::Python => matches!(
            kind,
            "function_definition"
                | "class_definition"
                | "import_statement"
                | "import_from_statement"
                | "expression_statement"
                | "return_statement"
                | "assert_statement"
                | "decorated_definition"
        ),
        SyntaxLanguage::JavaScript | SyntaxLanguage::TypeScript | SyntaxLanguage::Tsx => matches!(
            kind,
            "function_declaration"
                | "class_declaration"
                | "lexical_declaration"
                | "variable_declaration"
                | "import_statement"
                | "export_statement"
                | "expression_statement"
                | "return_statement"
                | "interface_declaration"
                | "type_alias_declaration"
                | "enum_declaration"
                | "method_definition"
                | "pair"
        ),
    }
}

fn is_hoistable(kind: &str) -> bool {
    matches!(
        kind,
        "parenthesized_expression"
            | "expression_statement"
            | "await_expression"
            | "unary_expression"
            | "reference_expression"
    )
}
