//! A file's facts: what the extractors read off one file's path and bytes, with every name as
//! written. Nothing here is resolved across files - no target, tier or placeholder - so a change
//! to one file never rewrites another file's facts. The code store holds these per file, stamped
//! with the tree they were read from; the resolver turns them into nodes and edges at read.

/// Which extractor a file's facts come from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Half {
    /// The per-language symbol extractors: definitions, mentions and module declarations.
    #[default]
    Code,
    /// The design-document extractor: concepts and links.
    Design,
    /// The workflow-definition extractor: nodes and relations.
    Workflow,
}

/// Whether a file is product or test code by its path, the extractor registry's path rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    #[default]
    Product,
    Test,
}

/// The tree a source names: the commit checked out, or a directory's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tip {
    /// The ref the operator checkout has checked out, for display.
    pub head: String,
    /// The commit; none when the tree is read from a directory.
    pub commit: Option<String>,
    /// The root tree id; for a directory source, a hash over the sorted (path, content hash) pairs.
    pub tree: String,
}

/// The tree a snapshot describes, and the extractor and sequence number that produced it. Only
/// `advance` builds one, from a [`Tip`] and what only it knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub tip: Tip,
    /// The extractor fingerprint: changes whenever any extractor's output can change.
    pub fingerprint: u64,
    /// The snapshot's sequence number.
    pub seq: u64,
}

/// What a name-reference is, as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MentionKind {
    /// An untagged reference: a call, or any reference the extractor tags with no kind.
    Call,
    /// A reference tagged with its kind (a type reference, a use, a trait impl, a const read).
    Tagged(String),
}

/// One definition in a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Def {
    /// The path within the file, the parent chain joined by `::` ("Outer::inner").
    pub local: String,
    /// The definition's own name ("inner").
    pub leaf: String,
    /// The definition's kind ("fn", "struct", "field", ...).
    pub kind: String,
    pub line: u32,
    pub end_line: u32,
    /// The index in the file's `defs` of the enclosing definition; none at file level.
    pub parent: Option<u32>,
    /// Whether the language marks this definition test-only (a test function, a test module).
    pub test: bool,
}

/// One name-reference in a file: the name as written, never a target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mention {
    /// The index in the file's `defs` of the enclosing definition; none at file level.
    pub enclosing: Option<u32>,
    /// The name as written ("bar", "Foo::bar", "crate::x::bar").
    pub name: String,
    /// The final identifier of `name` ("bar"), the key the resolver finds candidates by. Every
    /// extractor names a reference by its identifier alone, so it equals `name` today.
    pub leaf: String,
    pub kind: MentionKind,
    pub line: u32,
    /// Whether the mention sits inside a test-only definition.
    pub test: bool,
}

/// One module declared in a file (Rust's `mod foo;`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleDecl {
    pub name: String,
    /// The index in the file's definitions of the module definition that declares it.
    pub def: u32,
    /// Whether the declaration is test-only.
    pub test: bool,
    /// The paths the module's file can have, from the declaring file's path and any explicit path
    /// attribute. They depend on the declaring file alone; the view picks the first one that is
    /// a file in the tree.
    pub candidates: Vec<String>,
}

/// One design-intent concept a document yields, as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocConcept {
    /// The node kind the concept folds into.
    pub kind: String,
    pub id: String,
    pub title: String,
}

/// One design-intent link a document yields, with both ends as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocLink {
    pub from: String,
    /// The edge relation the link folds into.
    pub rel: String,
    pub to: String,
}

/// One node of the workflow definition (a stage, a gate, an agent role).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WfNode {
    /// The node kind.
    pub kind: String,
    pub id: String,
    pub title: String,
}

/// One relation of the workflow definition, with both ends as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WfRelation {
    pub from: String,
    /// The edge relation.
    pub rel: String,
    pub to: String,
}

/// Everything read off one file. The lists of the file's [`Half`] are filled; a code-half file
/// also carries the design facts the design extractor finds in source (its `WHY:` and `NOTE:`
/// rationale concepts and their links).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileFacts {
    /// The repo-relative, `/`-separated path.
    pub path: String,
    /// The git blob id, or an FNV content hash when there is no git.
    pub blob: String,
    /// The language name from the extractor registry.
    pub lang: String,
    pub half: Half,
    pub scope: Scope,
    /// Whether the parse of the file held an error, so its defs and mentions cover only as far
    /// as the parse reached.
    pub partial: bool,
    /// In the extractor's order.
    pub defs: Vec<Def>,
    /// In the extractor's order.
    pub mentions: Vec<Mention>,
    pub modules: Vec<ModuleDecl>,
    pub doc_concepts: Vec<DocConcept>,
    pub doc_links: Vec<DocLink>,
    pub wf_nodes: Vec<WfNode>,
    pub wf_relations: Vec<WfRelation>,
}

impl FileFacts {
    /// Whether the file holds design or workflow facts: concepts, links, nodes or relations.
    pub fn holds_intent(&self) -> bool {
        !(self.doc_concepts.is_empty()
            && self.doc_links.is_empty()
            && self.wf_nodes.is_empty()
            && self.wf_relations.is_empty())
    }
}
