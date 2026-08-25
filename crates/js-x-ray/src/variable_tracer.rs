//! Upstream: `src/VariableTracer.ts`
//!
//! PORT-TODO(stub): faithful port pending. The public surface below is the
//! contract consumed by SourceFile, ProbeRunner and the probes. Events are
//! queued instead of emitted synchronously; consumers drain them right after
//! each `walk` call, matching upstream's synchronous EventEmitter dispatch
//! ordering closely enough for analysis purposes.

use indexmap::IndexMap;
use serde_json::Value;

use crate::estree::{Node, SourceLocation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignmentKind {
    AliasBinding,
    ReturnValueAssignment,
}

#[derive(Debug, Clone)]
pub struct AssignmentMemory {
    pub r#type: AssignmentKind,
    pub name: String,
}

/// Upstream: `TracedIdentifierReport`.
#[derive(Debug, Clone)]
pub struct TracedIdentifierReport {
    pub name: String,
    pub identifier_or_member_expr: String,
    pub assignment_memory: Vec<AssignmentMemory>,
}

/// Upstream: `SourceTraced` options for `trace()`.
#[derive(Debug, Default, Clone)]
pub struct TraceOptions {
    pub follow_consecutive_assignment: bool,
    pub follow_return_value_assignement: bool,
    pub module_name: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LiteralIdentifier {
    pub value: String,
    /// "Literal" | "TemplateLiteral"
    pub r#type: &'static str,
}

/// Queued tracer events. Upstream: `AssignmentEvent`, `ImportEvent`,
/// `ReturnValueEvent` symbols on the EventEmitter.
#[derive(Debug, Clone)]
pub enum TracerEvent {
    Assignment {
        name: String,
        identifier_or_member_expr: String,
        id: String,
        location: Option<SourceLocation>,
    },
    ReturnValue {
        name: String,
        identifier_or_member_expr: String,
        id: String,
        location: Option<SourceLocation>,
        arguments: Vec<Value>,
    },
    Import {
        module_name: String,
        value: String,
        location: Option<SourceLocation>,
    },
}

#[derive(Debug, Default)]
pub struct VariableTracer {
    pub literal_identifiers: IndexMap<String, LiteralIdentifier>,
    pub object_identifiers: IndexMap<String, Node>,
    pub imported_modules: indexmap::IndexSet<String>,
    pub events: Vec<TracerEvent>,
    traced: IndexMap<String, TracedIdentifierReport>,
}

impl VariableTracer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `enableDefaultTracing`.
    pub fn enable_default_tracing(mut self) -> Self {
        // PORT-TODO(stub): trace require/eval/Function/atob defaults.
        let _ = &mut self;
        self
    }

    /// Upstream `trace`.
    pub fn trace(&mut self, identifier_or_member_expr: &str, options: TraceOptions) -> &mut Self {
        // PORT-TODO(stub)
        let _ = (identifier_or_member_expr, options);
        self
    }

    /// Upstream `getDataFromIdentifier`.
    pub fn get_data_from_identifier(
        &self,
        identifier_or_member_expr: &str,
        remove_global_identifier: bool,
    ) -> Option<TracedIdentifierReport> {
        // PORT-TODO(stub)
        let _ = remove_global_identifier;
        self.traced.get(identifier_or_member_expr).cloned()
    }

    /// Lookup for `externalIdentifierLookup` closures.
    pub fn literal_identifier_lookup(&self, name: &str) -> Option<String> {
        self.literal_identifiers.get(name).map(|id| id.value.clone())
    }

    /// Upstream `walk`.
    pub fn walk(&mut self, node: &Node) {
        // PORT-TODO(stub)
        let _ = node;
    }

    /// Drain queued events (Rust replacement for EventEmitter dispatch).
    pub fn drain_events(&mut self) -> Vec<TracerEvent> {
        std::mem::take(&mut self.events)
    }
}
