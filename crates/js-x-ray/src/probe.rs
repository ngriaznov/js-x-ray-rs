//! Upstream: `src/ProbeRunner.ts`
//!
//! Probe model notes for porters:
//! - Upstream `validateNode` may be an array of validators; here a probe
//!   implements `validate_node` returning the FIRST matching validator's data
//!   (`None` = no validator matched, `Some(data)` = matched; `data` may be
//!   `Value::Null`, mirroring `[true]` without payload).
//! - Upstream `main` returns `void | null | symbol`; `ProbeReturn` keeps the
//!   `void` vs `null` distinction because `breakOnMatch` only fires when the
//!   signal is NOT `Continue` (JS `null`).
//! - Upstream probe `context` objects are plain struct fields on the Rust
//!   probe. `CALL_EXPRESSION_IDENTIFIER` / `CALL_EXPRESSION_DATA` (contants.ts)
//!   are passed through [`ProbeCtx`] instead of being injected into contexts.
//! - `setEntryPoint`/named main handlers: keep an `entry_point` field on the
//!   probe struct and dispatch inside `main`.

use serde_json::Value;

use crate::estree::{
    GetCallExpressionIdentifierOptions, Node, get_call_expression_identifier, is_call_expression,
};
use crate::source_file::SourceFile;
use crate::variable_tracer::{TracedIdentifierReport, TracerEvent};

/// What a probe's `main` returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeReturn {
    /// JS `undefined` — the probe matched and ran to completion.
    Matched,
    /// JS `null` (`Signals.Continue`) — keep evaluating other probes.
    Continue,
    /// `Signals.Skip` — skip this node's children entirely.
    Skip,
    /// `Signals.Break` — stop evaluating probes (or the probe's break group).
    Break,
}

/// Per-node context handed to probes.
pub struct ProbeCtx<'a> {
    pub source_file: &'a mut SourceFile,
    /// `CALL_EXPRESSION_IDENTIFIER`: set when the current node is a
    /// CallExpression with a resolvable identifier that is being traced.
    pub traced_identifier: Option<&'a str>,
    /// `CALL_EXPRESSION_DATA`: the traced identifier's report.
    pub traced_data: Option<&'a TracedIdentifierReport>,
}

pub trait Probe {
    fn name(&self) -> &'static str;

    /// ESTree node types this probe handles; `None` = catch-all.
    fn node_types(&self) -> Option<&'static [&'static str]> {
        None
    }

    fn initialize(&mut self, _source_file: &mut SourceFile) {}

    /// `None` = no validator matched. `Some(data)` = matched with data
    /// (`Value::Null` when the validator carried no payload).
    fn validate_node(&mut self, node: &Node, ctx: &mut ProbeCtx<'_>) -> Option<Value>;

    fn main(&mut self, node: &Node, data: &Value, ctx: &mut ProbeCtx<'_>) -> ProbeReturn;

    fn finalize(&mut self, _source_file: &mut SourceFile) {}

    fn teardown(&mut self, _source_file: &mut SourceFile) {}

    fn break_on_match(&self) -> bool {
        false
    }

    fn break_group(&self) -> Option<&'static str> {
        None
    }

    /// Rust replacement for subscribing to `VariableTracer` events in
    /// `initialize`. Called for every event drained after tracer walks.
    fn on_tracer_event(&mut self, _event: &TracerEvent, _source_file: &mut SourceFile) {}
}

/// The action the walker must take after probing a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalkAction {
    None,
    Skip,
}

/// Upstream: `ProbeRunner`.
pub struct ProbeRunner {
    pub probes: Vec<Box<dyn Probe>>,
    break_groups: indexmap::IndexSet<&'static str>,
}

impl ProbeRunner {
    pub fn new(source_file: &mut SourceFile, mut probes: Vec<Box<dyn Probe>>) -> Self {
        for probe in &mut probes {
            probe.initialize(source_file);
        }
        Self {
            probes,
            break_groups: indexmap::IndexSet::new(),
        }
    }

    /// Dispatch drained tracer events to every probe.
    pub fn dispatch_events(&mut self, events: &[TracerEvent], source_file: &mut SourceFile) {
        for event in events {
            for probe in &mut self.probes {
                probe.on_tracer_event(event, source_file);
            }
        }
    }

    /// Upstream `walk`: run every applicable probe against a node.
    pub fn walk(&mut self, node: &Node, source_file: &mut SourceFile) -> WalkAction {
        self.break_groups.clear();

        let mut traced_identifier: Option<String> = None;
        let mut traced_report: Option<TracedIdentifierReport> = None;

        if is_call_expression(node) {
            let tracer = &source_file.tracer;
            let lookup = |name: &str| tracer.literal_identifier_lookup(name);
            let id = get_call_expression_identifier(
                node,
                &GetCallExpressionIdentifierOptions {
                    external_identifier_lookup: &lookup,
                    resolve_call_expression: true,
                },
            );
            if let Some(id) = id {
                traced_report = tracer.get_data_from_identifier(&id, false);
                traced_identifier = Some(id);
            }
        }

        let node_ty = node.get("type").and_then(Value::as_str).unwrap_or("");

        for probe in &mut self.probes {
            if let Some(types) = probe.node_types()
                && !types.is_empty()
                && !types.contains(&node_ty)
            {
                continue;
            }
            if let Some(break_group) = probe.break_group()
                && self.break_groups.contains(break_group)
            {
                continue;
            }

            let signal = {
                let mut ctx = ProbeCtx {
                    source_file,
                    traced_identifier: traced_identifier.as_deref(),
                    traced_data: traced_report.as_ref(),
                };
                run_probe(probe.as_mut(), node, &mut ctx)
            };
            probe.teardown(source_file);

            match signal {
                ProbeReturn::Continue => continue,
                ProbeReturn::Skip => return WalkAction::Skip,
                ProbeReturn::Break => match probe.break_group() {
                    None => break,
                    Some(group) => {
                        self.break_groups.insert(group);
                    }
                },
                ProbeReturn::Matched => {
                    if probe.break_on_match() {
                        match probe.break_group() {
                            None => break,
                            Some(group) => {
                                self.break_groups.insert(group);
                            }
                        }
                    }
                }
            }
        }

        WalkAction::None
    }

    /// Upstream `finalize`.
    pub fn finalize(&mut self, source_file: &mut SourceFile) {
        for probe in &mut self.probes {
            probe.finalize(source_file);
        }
    }
}

/// Upstream `#runProbe`: validators then main.
fn run_probe(probe: &mut dyn Probe, node: &Node, ctx: &mut ProbeCtx<'_>) -> ProbeReturn {
    match probe.validate_node(node, ctx) {
        Some(data) => probe.main(node, &data, ctx),
        // No validator matched → JS `null` (Continue).
        None => ProbeReturn::Continue,
    }
}
