//! Upstream: `src/probes/tracing.ts`

use indexmap::IndexSet;
use serde_json::Value;

use crate::probe::ProbeCtx;
use crate::variable_tracer::{TraceOptions, TracedIdentifierReport, VariableTracer};

/// Upstream `TraceAllOptions`: `SourceTraced` without `moduleName`, where
/// `followConsecutiveAssignment` defaults to `true`.
#[derive(Debug, Clone, Copy)]
pub struct TraceAllOptions {
    pub follow_consecutive_assignment: bool,
    pub follow_return_value_assignement: bool,
}

impl Default for TraceAllOptions {
    fn default() -> Self {
        Self {
            follow_consecutive_assignment: true,
            follow_return_value_assignement: false,
        }
    }
}

impl TraceAllOptions {
    fn with_module(self, module_name: Option<&str>) -> TraceOptions {
        TraceOptions {
            follow_consecutive_assignment: self.follow_consecutive_assignment,
            follow_return_value_assignement: self.follow_return_value_assignement,
            module_name: module_name.map(str::to_owned),
            name: None,
        }
    }
}

/// For global identifiers like `Math.random` which are never behind an
/// imported module.
pub fn trace_all<'a>(
    tracer: &mut VariableTracer,
    identifiers: impl IntoIterator<Item = &'a str>,
    options: TraceAllOptions,
) {
    for identifier_or_member_expr in identifiers {
        tracer.trace(identifier_or_member_expr, options.with_module(None));
    }
}

/// For identifiers gated behind the module they start with:
/// `"fs.readFileSync"`. Returns the module names.
pub fn trace_all_from_module<'a>(
    tracer: &mut VariableTracer,
    identifiers: impl IntoIterator<Item = &'a str>,
    options: TraceAllOptions,
) -> IndexSet<String> {
    let mut modules = IndexSet::new();
    for identifier_or_member_expr in identifiers {
        let module_name = identifier_or_member_expr
            .split('.')
            .next()
            .unwrap_or_default();
        modules.insert(module_name.to_owned());

        tracer.trace(
            identifier_or_member_expr,
            options.with_module(Some(module_name)),
        );
    }

    modules
}

pub fn has_imported_modules(ctx: &ProbeCtx<'_>, module_names: &[&str]) -> bool {
    let imported_modules = &ctx.source_file.tracer.imported_modules;

    module_names
        .iter()
        .all(|module_name| imported_modules.contains(*module_name))
}

/// Report of the CallExpression being walked, injected by `ProbeRunner`.
pub fn get_traced_call<'a>(ctx: &ProbeCtx<'a>) -> Option<&'a TracedIdentifierReport> {
    ctx.traced_data
}

/// Match the CallExpression being walked against a set of traced names,
/// shaped as a `validate_node` result carrying the matched name.
pub fn match_traced_call(ctx: &ProbeCtx<'_>, names: &[&str]) -> Option<Value> {
    let identifier_or_member_expr = &get_traced_call(ctx)?.identifier_or_member_expr;

    names
        .contains(&identifier_or_member_expr.as_str())
        .then(|| Value::String(identifier_or_member_expr.clone()))
}
