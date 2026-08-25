//! Upstream: `src/VariableTracer.ts`
//!
//! Faithful port. Upstream is an `EventEmitter` and emits
//! `AssignmentEvent` / `ImportEvent` / `ReturnValueEvent` synchronously;
//! here events are pushed into `self.events` in the exact same order and the
//! analyser drains them (via [`VariableTracer::drain_events`]) right after
//! each `walk` call and dispatches them to probes.
//!
//! Upstream keeps a `Map<string, Traced>` where several keys may point at the
//! *same* JS object (consecutive assignment aliases share the traced record,
//! and `assignmentMemory` mutations through one key must be observable
//! through the others). That reference semantic is reproduced with
//! `Rc<RefCell<Traced>>`.

use std::cell::RefCell;
use std::rc::Rc;

use indexmap::{IndexMap, IndexSet};
use serde_json::Value;

use crate::estree::{
    Node, SourceLocation, call_expression_identifier, extract_logical_expression,
    get_call_expression_arguments, get_member_expression_identifier,
    get_variable_declaration_identifiers, identifier_name, is_call_expression, is_identifier,
    is_string_literal, is_type, js_string, node_type, to_literal,
};
use crate::utils::{
    get_sub_member_expression_segments, is_evil_identifier_path, is_neutral_callable,
    make_prefix_remover, strip_node_prefix,
};

// CONSTANTS (upstream: kGlobalIdentifiersToTrace, kRequirePatterns, kUnsafeGlobalCallExpression)
const K_GLOBAL_IDENTIFIERS_TO_TRACE: [&str; 5] =
    ["globalThis", "global", "root", "GLOBAL", "window"];
const K_REQUIRE_PATTERNS: [&str; 5] = [
    "require",
    "require.resolve",
    "require.main",
    "process.mainModule.require",
    "process.getBuiltinModule",
];
const K_UNSAFE_GLOBAL_CALL_EXPRESSION: [&str; 2] = ["eval", "Function"];

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

/// Upstream: internal `Traced` interface (`Required<SourceTraced>` +
/// identifierOrMemberExpr + assignmentMemory).
#[derive(Debug)]
struct Traced {
    name: String,
    identifier_or_member_expr: String,
    follow_consecutive_assignment: bool,
    #[allow(dead_code)]
    follow_return_value_assignement: bool,
    module_name: Option<String>,
    assignment_memory: Vec<AssignmentMemory>,
}

type SharedTraced = Rc<RefCell<Traced>>;

#[derive(Debug, Clone)]
pub struct LiteralIdentifier {
    pub value: String,
    /// "Literal" | "TemplateLiteral"
    pub r#type: &'static str,
}

/// Queued tracer events. Upstream: `AssignmentEvent`, `ImportEvent`,
/// `ReturnValueEvent` symbols on the EventEmitter. Upstream also re-emits the
/// assignment payload under the traced `identifierOrMemberExpr` event name;
/// consumers here filter on the payload field instead.
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
    // PUBLIC PROPERTIES
    pub literal_identifiers: IndexMap<String, LiteralIdentifier>,
    /// Resolves an identifier assigned an object literal back to its
    /// ObjectExpression node.
    pub object_identifiers: IndexMap<String, Node>,
    pub imported_modules: IndexSet<String>,
    /// Queued events (Rust replacement for the EventEmitter).
    pub events: Vec<TracerEvent>,

    // PRIVATE PROPERTIES
    traced: IndexMap<String, SharedTraced>,
    variables_ref_to_global: IndexSet<String>,
    neutral_callable: IndexSet<String>,
    assigned_return_value_to_traced: IndexMap<String, String>,
}

impl VariableTracer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Upstream `enableDefaultTracing`.
    pub fn enable_default_tracing(mut self) -> Self {
        for pattern in K_REQUIRE_PATTERNS {
            self.trace(
                pattern,
                TraceOptions {
                    follow_consecutive_assignment: true,
                    name: Some("require".to_owned()),
                    ..Default::default()
                },
            );
        }

        self.trace("eval", TraceOptions::default());
        self.trace("Function", TraceOptions::default());
        self.trace(
            "atob",
            TraceOptions {
                follow_consecutive_assignment: true,
                ..Default::default()
            },
        );
        self
    }

    /// Upstream `trace`.
    pub fn trace(&mut self, identifier_or_member_expr: &str, options: TraceOptions) -> &mut Self {
        let TraceOptions {
            follow_consecutive_assignment,
            follow_return_value_assignement,
            module_name,
            name,
        } = options;
        let name = name.unwrap_or_else(|| identifier_or_member_expr.to_owned());

        self.traced.insert(
            identifier_or_member_expr.to_owned(),
            Rc::new(RefCell::new(Traced {
                name: name.clone(),
                identifier_or_member_expr: identifier_or_member_expr.to_owned(),
                follow_consecutive_assignment,
                follow_return_value_assignement,
                assignment_memory: Vec::new(),
                module_name: module_name.clone(),
            })),
        );

        if identifier_or_member_expr.contains('.') {
            for expr in get_sub_member_expression_segments(identifier_or_member_expr) {
                if !self.traced.contains_key(&expr) {
                    self.trace(
                        &expr,
                        TraceOptions {
                            follow_consecutive_assignment: true,
                            follow_return_value_assignement: false,
                            module_name: module_name.clone(),
                            name: Some(name.clone()),
                        },
                    );
                }
            }
        }

        self
    }

    /// Upstream `getDataFromIdentifier` with `DataIdentifierOptions`.
    pub fn get_data_from_identifier(
        &self,
        identifier_or_member_expr: &str,
        remove_global_identifier: bool,
    ) -> Option<TracedIdentifierReport> {
        let identifier_or_member_expr = if remove_global_identifier {
            let remover = make_prefix_remover(
                K_GLOBAL_IDENTIFIERS_TO_TRACE
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
            );
            remover(identifier_or_member_expr)
        } else {
            identifier_or_member_expr.to_owned()
        };

        let is_member_expr = identifier_or_member_expr.contains('.');
        let is_tracing_identifier = self.traced.contains_key(&identifier_or_member_expr);

        let mut final_identifier = identifier_or_member_expr.clone();
        if is_member_expr && !is_tracing_identifier {
            let segment = identifier_or_member_expr.split('.').next().unwrap_or("");
            if let Some(traced_identifier) = self.traced.get(segment) {
                final_identifier = format!(
                    "{}{}",
                    traced_identifier.borrow().identifier_or_member_expr,
                    &identifier_or_member_expr[segment.len()..]
                );
            }

            if !self.traced.contains_key(&final_identifier) {
                return None;
            }
        } else if !is_tracing_identifier {
            return None;
        }

        let traced_identifier = self.traced.get(&final_identifier)?.borrow();
        if !self.is_traced_identifier_imported_as_module(&traced_identifier) {
            return None;
        }

        let assignment_memory = self
            .traced
            .get(&traced_identifier.name)
            .map(|entry| entry.borrow().assignment_memory.clone())
            .unwrap_or_default();

        Some(TracedIdentifierReport {
            name: traced_identifier.name.clone(),
            identifier_or_member_expr: traced_identifier.identifier_or_member_expr.clone(),
            assignment_memory,
        })
    }

    /// Lookup for `externalIdentifierLookup` closures.
    pub fn literal_identifier_lookup(&self, name: &str) -> Option<String> {
        self.literal_identifiers
            .get(name)
            .map(|id| id.value.clone())
    }

    /// Upstream `#getTracedName`.
    fn get_traced_name(&self, identifier_or_member_expr: &str) -> Option<String> {
        self.traced
            .get(identifier_or_member_expr)
            .map(|traced| traced.borrow().name.clone())
    }

    /// Upstream `#isTracedIdentifierImportedAsModule`.
    fn is_traced_identifier_imported_as_module(&self, id: &Traced) -> bool {
        match &id.module_name {
            None => true,
            Some(module_name) => self.imported_modules.contains(module_name),
        }
    }

    /// Upstream `#declareNewAssignment`.
    fn declare_new_assignment(&mut self, identifier_or_member_expr: &str, id: &Value) {
        let Some(traced_variant) = self.traced.get(identifier_or_member_expr).cloned() else {
            // We return if required module has not been imported.
            // It means the assignment has no relation with the required tracing.
            return;
        };
        if !self.is_traced_identifier_imported_as_module(&traced_variant.borrow()) {
            return;
        }

        let new_identifier_name = identifier_name(id).unwrap_or("").to_owned();

        let (name, identifier_or_member_expr_value, follow_consecutive_assignment) = {
            let traced = traced_variant.borrow();
            (
                traced.name.clone(),
                traced.identifier_or_member_expr.clone(),
                traced.follow_consecutive_assignment,
            )
        };

        self.events.push(TracerEvent::Assignment {
            name: name.clone(),
            identifier_or_member_expr: identifier_or_member_expr_value,
            id: new_identifier_name.clone(),
            location: SourceLocation::from_node(id),
        });

        if follow_consecutive_assignment && !self.traced.contains_key(&new_identifier_name) {
            if let Some(named_entry) = self.traced.get(&name).cloned() {
                named_entry
                    .borrow_mut()
                    .assignment_memory
                    .push(AssignmentMemory {
                        r#type: AssignmentKind::AliasBinding,
                        name: new_identifier_name.clone(),
                    });
            }
            self.traced.insert(new_identifier_name, traced_variant);
        }
    }

    /// Upstream `#isGlobalVariableIdentifier`.
    fn is_global_variable_identifier(&self, identifier_name: &str) -> bool {
        K_GLOBAL_IDENTIFIERS_TO_TRACE.contains(&identifier_name)
            || self.variables_ref_to_global.contains(identifier_name)
    }

    /// Upstream `#searchForMemberExprAlternative`.
    ///
    /// Search alternative for the given MemberExpression parts.
    fn search_for_member_expr_alternative(&self, parts: &[String]) -> Vec<String> {
        parts
            .iter()
            .flat_map(|identifier_name| {
                if let Some(traced) = self.traced.get(identifier_name) {
                    return vec![traced.borrow().identifier_or_member_expr.clone()];
                }

                // If identifier is global then we can eliminate the value from
                // MemberExpr: globalThis.process === process;
                if self.is_global_variable_identifier(identifier_name) {
                    return vec![];
                }

                vec![identifier_name.clone()]
            })
            .collect()
    }

    /// Upstream `#autoTraceId`.
    fn auto_trace_id(&mut self, id: &Value, prefix: Option<&str>) {
        for (name, assignment_id) in get_variable_declaration_identifiers(id, None) {
            let identifier_or_member_expr = match prefix {
                Some(prefix) => format!("{prefix}.{name}"),
                None => name,
            };

            if self.traced.contains_key(&identifier_or_member_expr) {
                self.declare_new_assignment(&identifier_or_member_expr, &assignment_id);
            }
        }
    }

    /// Upstream `#reverseAtob`.
    fn reverse_atob(&mut self, node: &Value, id: &Value) {
        let call_expr_arguments = {
            let literal_identifiers = &self.literal_identifiers;
            let lookup = move |name: &str| literal_identifiers.get(name).map(|id| id.value.clone());
            get_call_expression_arguments(node, &lookup)
        };
        let Some(call_expr_arguments) = call_expr_arguments else {
            return;
        };

        if let Some(call_expr_argument_node) = call_expr_arguments.first() {
            self.literal_identifiers.insert(
                identifier_name(id).unwrap_or("").to_owned(),
                LiteralIdentifier {
                    value: base64_decode_to_string(call_expr_argument_node),
                    r#type: "Literal",
                },
            );
        }
    }

    /// Upstream `#walkImportDeclaration`.
    fn walk_import_declaration(&mut self, node: &Value) {
        let Some(source_value) = node.pointer("/source/value").and_then(Value::as_str) else {
            return;
        };
        let stripped = strip_node_prefix(source_value);
        let module_name = stripped
            .strip_suffix("/promises")
            .unwrap_or(stripped)
            .to_owned();
        if !self.traced.contains_key(&module_name) {
            return;
        }

        self.imported_modules.insert(module_name.clone());

        self.events.push(TracerEvent::Import {
            module_name: module_name.clone(),
            value: source_value.to_owned(),
            location: SourceLocation::from_node(node),
        });

        let empty = Vec::new();
        let specifiers = node
            .get("specifiers")
            .and_then(Value::as_array)
            .unwrap_or(&empty);

        if let Some(first_specifier) = specifiers.first() {
            // import * as boo from "crypto";
            if is_type(first_specifier, "ImportNamespaceSpecifier") {
                if let Some(local) = first_specifier.get("local") {
                    self.declare_new_assignment(&module_name, local);
                }

                return;
            }

            // import boo from "crypto";
            if is_type(first_specifier, "ImportDefaultSpecifier") {
                if let Some(local) = first_specifier.get("local") {
                    self.declare_new_assignment(&module_name, local);
                }

                return;
            }
        }

        // import { createHash } from "crypto";
        for specifier in specifiers
            .iter()
            .filter(|specifier_node| is_type(specifier_node, "ImportSpecifier"))
        {
            let Some(imported) = specifier.get("imported") else {
                continue;
            };
            if !is_identifier(imported) {
                continue;
            }
            let full_imported_name =
                format!("{module_name}.{}", identifier_name(imported).unwrap_or(""));

            if self.traced.contains_key(&full_imported_name) {
                self.declare_new_assignment(&full_imported_name, imported);
            }
        }
    }

    /// Upstream `#walkRequireCallExpression`.
    fn walk_require_call_expression(&mut self, node: &Value, id: &Value) {
        let module_name_literal = node
            .get("arguments")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|argument_node| {
                is_string_literal(argument_node)
                    && argument_node
                        .get("value")
                        .and_then(Value::as_str)
                        .is_some_and(|value| self.traced.contains_key(strip_node_prefix(value)))
            });
        let Some(module_name_literal) = module_name_literal else {
            return;
        };
        let literal_value = module_name_literal
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("");
        let module_name = strip_node_prefix(literal_value).to_owned();
        self.imported_modules.insert(module_name.clone());

        self.events.push(TracerEvent::Import {
            module_name: module_name.clone(),
            value: literal_value.to_owned(),
            location: SourceLocation::from_node(module_name_literal),
        });

        match node_type(id) {
            Some("Identifier") => {
                self.declare_new_assignment(&module_name, id);
            }
            Some("ObjectPattern") => {
                self.auto_trace_id(id, Some(&module_name));
            }
            _ => {}
        }
    }

    /// Upstream `#walkVariableDeclaratorInitialization`.
    fn walk_variable_declarator_initialization(
        &mut self,
        variable_declarator_node: &Value,
        child_node: &Value,
    ) {
        if child_node.is_null() {
            return;
        }
        let Some(id) = variable_declarator_node.get("id") else {
            return;
        };
        if !is_identifier(id) {
            return;
        }
        let id_name = identifier_name(id).unwrap_or("").to_owned();

        match node_type(child_node) {
            // let foo = "10"; <-- "foo" is the key and "10" the value
            Some("Literal") => {
                let value = match child_node.get("value") {
                    Some(value) => js_string(value),
                    // String(undefined)
                    None => "undefined".to_owned(),
                };
                self.literal_identifiers.insert(
                    id_name,
                    LiteralIdentifier {
                        value,
                        r#type: "Literal",
                    },
                );
            }
            // const x = `hello ${name}`; "x" is the key and "hello ${0}" the value
            Some("TemplateLiteral") => {
                self.literal_identifiers.insert(
                    id_name,
                    LiteralIdentifier {
                        value: to_literal(child_node),
                        r#type: "TemplateLiteral",
                    },
                );
            }

            /*
             * import os from "node:os";
             *
             * const foo = {
             *    host: os.hostname(), <-- Property
             *    ...{ bar: "hello world"} <-- SpreadElement
             * };
             * ^ ObjectExpression
             */
            Some("ObjectExpression") => {
                // Only record top-level assignments (`const x = {...}`) so
                // consumers can resolve "x" back to its object shape.
                let is_top_level_init = variable_declarator_node
                    .get("init")
                    .is_some_and(|init| std::ptr::eq(init, child_node));
                if is_top_level_init {
                    self.object_identifiers.insert(id_name, child_node.clone());
                }

                let properties: Vec<&Value> = child_node
                    .get("properties")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .collect();
                for property in properties {
                    let node = match node_type(property) {
                        Some("Property") => property.get("value"),
                        Some("SpreadElement") => property.get("argument"),
                        _ => None,
                    };

                    if let Some(node) = node
                        && !node.is_null()
                    {
                        self.walk_variable_declarator_initialization(
                            variable_declarator_node,
                            node,
                        );
                    }
                }
            }

            Some("ArrayExpression") => {
                let elements: Vec<&Value> = child_node
                    .get("elements")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .collect();
                for element in elements {
                    self.walk_variable_declarator_initialization(variable_declarator_node, element);
                }
            }

            Some("SpreadElement") => {
                if let Some(argument) = child_node.get("argument") {
                    self.walk_variable_declarator_initialization(
                        variable_declarator_node,
                        argument,
                    );
                }
            }

            /*
             * const g = eval("this");
             * const g = Function("return this")();
             */
            Some("NewExpression") | Some("CallExpression") => {
                let Some(full_identifier_path) = call_expression_identifier(child_node) else {
                    return;
                };

                let traced_full_identifier_name = self
                    .get_traced_name(&full_identifier_path)
                    .unwrap_or_else(|| full_identifier_path.clone());
                let identifier_name_segment = full_identifier_path
                    .split('.')
                    .next()
                    .unwrap_or("")
                    .to_owned();

                let traced_variant: Option<SharedTraced> =
                    if self.traced.contains_key(&traced_full_identifier_name) {
                        self.traced.get(&traced_full_identifier_name).cloned()
                    } else {
                        let parts: Vec<String> = traced_full_identifier_name
                            .split('.')
                            .map(str::to_owned)
                            .collect();
                        let alternative_member_expr_fullname =
                            self.search_for_member_expr_alternative(&parts).join(".");
                        self.traced.get(&alternative_member_expr_fullname).cloned()
                    };

                if let Some(traced_variant) = &traced_variant
                    && traced_variant.borrow().follow_return_value_assignement
                {
                    traced_variant
                        .borrow_mut()
                        .assignment_memory
                        .push(AssignmentMemory {
                            r#type: AssignmentKind::ReturnValueAssignment,
                            name: id_name.clone(),
                        });
                    let (name, identifier_or_member_expr, follow_consecutive_assignment) = {
                        let traced = traced_variant.borrow();
                        (
                            traced.name.clone(),
                            traced.identifier_or_member_expr.clone(),
                            traced.follow_consecutive_assignment,
                        )
                    };
                    self.events.push(TracerEvent::ReturnValue {
                        name,
                        identifier_or_member_expr,
                        id: id_name.clone(),
                        location: SourceLocation::from_node(id),
                        arguments: child_node
                            .get("arguments")
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default(),
                    });
                    if follow_consecutive_assignment {
                        self.assigned_return_value_to_traced
                            .insert(id_name.clone(), traced_full_identifier_name.clone());
                    }
                }

                if node_type(child_node) != Some("CallExpression") {
                    return;
                }
                // const id = Function.prototype.call.call(require, require, "http");
                if self.neutral_callable.contains(&identifier_name_segment)
                    || is_evil_identifier_path(&full_identifier_path)
                {
                    self.walk_require_call_expression(child_node, id);
                } else if K_UNSAFE_GLOBAL_CALL_EXPRESSION
                    .contains(&identifier_name_segment.as_str())
                {
                    self.variables_ref_to_global.insert(id_name);
                }
                // const foo = require("crypto");
                // const bar = require.call(null, "crypto");
                else if K_REQUIRE_PATTERNS.contains(&identifier_name_segment.as_str()) {
                    self.walk_require_call_expression(child_node, id);
                } else if traced_full_identifier_name == "atob" {
                    self.reverse_atob(child_node, id);
                }
            }

            // const r = require
            Some("Identifier") => {
                let identifier_name_str = identifier_name(child_node).unwrap_or("").to_owned();
                if self.traced.contains_key(&identifier_name_str) {
                    self.declare_new_assignment(&identifier_name_str, id);
                } else if self.is_global_variable_identifier(&identifier_name_str) {
                    self.variables_ref_to_global.insert(id_name.clone());
                }

                if let Some(traced_full_identifier_name) = self
                    .assigned_return_value_to_traced
                    .get(&identifier_name_str)
                    .cloned()
                    && let Some(traced_variant) =
                        self.traced.get(&traced_full_identifier_name).cloned()
                {
                    traced_variant
                        .borrow_mut()
                        .assignment_memory
                        .push(AssignmentMemory {
                            r#type: AssignmentKind::ReturnValueAssignment,
                            name: id_name.clone(),
                        });
                    self.assigned_return_value_to_traced
                        .insert(id_name, traced_full_identifier_name);
                }
            }

            // process.mainModule and require.resolve
            Some("MemberExpression") => {
                // Example: ["process", "mainModule"]
                let member_expr_parts = {
                    let literal_identifiers = &self.literal_identifiers;
                    let lookup =
                        move |name: &str| literal_identifiers.get(name).map(|id| id.value.clone());
                    get_member_expression_identifier(child_node, &lookup)
                };
                let member_expr_fullname = member_expr_parts.join(".");

                // Function.prototype.call
                if is_neutral_callable(&member_expr_fullname) {
                    self.neutral_callable.insert(id_name);
                } else if self.traced.contains_key(&member_expr_fullname) {
                    self.declare_new_assignment(&member_expr_fullname, id);
                } else {
                    let alternative_member_expr_fullname = self
                        .search_for_member_expr_alternative(&member_expr_parts)
                        .join(".");

                    if self.traced.contains_key(&alternative_member_expr_fullname) {
                        self.declare_new_assignment(&alternative_member_expr_fullname, id);
                    }
                }

                if let Some(object) = child_node.get("object")
                    && is_call_expression(object)
                {
                    self.walk_variable_declarator_initialization(variable_declarator_node, object);
                }
            }

            _ => {}
        }
    }

    /// Upstream `#walkVariableDeclarationWithAnythingElse`.
    fn walk_variable_declaration_with_anything_else(&mut self, variable_declarator_node: &Value) {
        let Some(init) = variable_declarator_node.get("init") else {
            return;
        };
        if init.is_null() {
            return;
        }
        let Some(id) = variable_declarator_node.get("id") else {
            return;
        };

        match node_type(init) {
            // const { process } = eval("this");
            Some("CallExpression") => {
                let Some(full_identifier_path) = call_expression_identifier(init) else {
                    return;
                };
                let identifier_name_segment = full_identifier_path.split('.').next().unwrap_or("");
                // const {} = Function.prototype.call.call(require, require, "http");
                if is_evil_identifier_path(&full_identifier_path) {
                    self.walk_require_call_expression(init, id);
                } else if K_UNSAFE_GLOBAL_CALL_EXPRESSION.contains(&identifier_name_segment) {
                    self.auto_trace_id(id, None);
                }
                // const { createHash } = require("crypto");
                else if K_REQUIRE_PATTERNS.contains(&identifier_name_segment) {
                    self.walk_require_call_expression(init, id);
                }
            }

            // const { process } = globalThis;
            Some("Identifier") => {
                let identifier_name_str = identifier_name(init).unwrap_or("");
                if self.is_global_variable_identifier(identifier_name_str) {
                    self.auto_trace_id(id, None);
                }
            }

            _ => {}
        }
    }

    /// Upstream `#walkVariableDeclarator`.
    fn walk_variable_declarator(&mut self, node: &Value) {
        // var foo; <-- no initialization here.
        let Some(init) = node.get("init") else {
            return;
        };
        if init.is_null() {
            return;
        }

        /*
         * const { foo } = {};
         *       ^     ^ ObjectPattern (example)
         */
        if !node.get("id").is_some_and(is_identifier) {
            self.walk_variable_declaration_with_anything_else(node);

            return;
        }

        // var root = freeGlobal || freeSelf || Function('return this')();
        if is_type(init, "LogicalExpression") {
            for (_operator, extracted_node) in extract_logical_expression(init) {
                self.walk_variable_declarator_initialization(node, &extracted_node);
            }
        }
        // const foo = "bar";
        else {
            self.walk_variable_declarator_initialization(node, init);
        }
    }

    /// Upstream `walk`.
    pub fn walk(&mut self, node: &Node) {
        match node_type(node) {
            Some("ImportDeclaration") => {
                self.walk_import_declaration(node);
            }
            Some("VariableDeclaration") => {
                let declarations: Vec<&Value> = node
                    .get("declarations")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .collect();
                for declaration in declarations {
                    self.walk_variable_declarator(declaration);
                }
            }
            _ => {}
        }
    }

    /// Drain queued events (Rust replacement for EventEmitter dispatch).
    pub fn drain_events(&mut self) -> Vec<TracerEvent> {
        std::mem::take(&mut self.events)
    }
}

/// Node.js `Buffer.from(input, "base64").toString()` emulation: a lenient
/// base64 decoder (accepts both the standard and url-safe alphabets, skips
/// any other invalid character) followed by lossy UTF-8 decoding.
///
/// Node's decoder treats `=` not as a skippable character but as a hard
/// terminator: decoding stops the instant a `=` is seen, discarding
/// anything after it (even further valid base64 data), rather than
/// resuming past it. E.g. `Buffer.from("QQ==QQ==", "base64")` decodes to
/// just `"A"`, not the four-`Q` stream you'd get by merely skipping `=`.
fn base64_decode_to_string(input: &str) -> String {
    let mut bytes: Vec<u8> = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;

    for ch in input.bytes() {
        if ch == b'=' {
            break;
        }
        let sextet = match ch {
            b'A'..=b'Z' => (ch - b'A') as u32,
            b'a'..=b'z' => (ch - b'a' + 26) as u32,
            b'0'..=b'9' => (ch - b'0' + 52) as u32,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        buffer = (buffer << 6) | sextet;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
        }
    }

    String::from_utf8_lossy(&bytes).into_owned()
}
