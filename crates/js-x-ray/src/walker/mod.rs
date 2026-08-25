//! Synchronous ESTree walker over `serde_json::Value` trees.
//!
//! Upstream: `src/walker/` (index.ts, walker.base.ts, walker.sync.ts) — itself
//! a fork of `estree-walker`. Traversal order follows JSON object key order,
//! which `serde_json`'s `preserve_order` feature keeps stable.

use serde_json::Value;

use crate::estree::types::is_node;

/// Mutation requests a visitor can issue for the node it is visiting.
/// Upstream: `WalkerContext` (walker.base.ts).
#[derive(Debug, Default)]
pub struct WalkerContext {
    should_skip: bool,
    should_remove: bool,
    replacement: Option<Value>,
}

impl WalkerContext {
    /// Do not descend into the current node's children.
    pub fn skip(&mut self) {
        self.should_skip = true;
    }

    /// Remove the current node from its parent.
    pub fn remove(&mut self) {
        self.should_remove = true;
    }

    /// Replace the current node.
    pub fn replace(&mut self, node: Value) {
        self.replacement = Some(node);
    }

    /// Replace the current node and skip its children.
    pub fn replace_and_skip(&mut self, node: Value) {
        self.should_skip = true;
        self.replacement = Some(node);
    }

    fn take(&mut self) -> (bool, bool, Option<Value>) {
        (
            std::mem::take(&mut self.should_skip),
            std::mem::take(&mut self.should_remove),
            self.replacement.take(),
        )
    }
}

pub type SyncHandler<'a> = dyn FnMut(&mut WalkerContext, &mut Value) + 'a;

enum Outcome {
    Kept,
    Removed,
}

struct SyncWalker<'h, 'a> {
    enter: Option<&'h mut SyncHandler<'a>>,
    leave: Option<&'h mut SyncHandler<'a>>,
    context: WalkerContext,
}

impl SyncWalker<'_, '_> {
    /// Upstream: `SyncWalker.visit`. The parent/prop/index bookkeeping is
    /// unnecessary in Rust: replacement writes through the `&mut Value` and
    /// removal is signalled to the caller through [`Outcome`].
    fn visit(&mut self, node: &mut Value) -> Outcome {
        if node.is_null() {
            return Outcome::Removed;
        }
        // Deep ASTs (minified/obfuscated sources) exceed default thread
        // stacks; grow on demand like the deserializer does.
        stacker::maybe_grow(64 * 1024, 1024 * 1024, || self.visit_inner(node))
    }

    fn visit_inner(&mut self, node: &mut Value) -> Outcome {
        if self.enter.is_some() {
            let saved = self.context.take();
            if let Some(enter) = self.enter.as_deref_mut() {
                enter(&mut self.context, node);
            }
            let (skipped, removed, replacement) = self.context.take();
            (
                self.context.should_skip,
                self.context.should_remove,
                self.context.replacement,
            ) = saved;

            if let Some(replacement) = replacement {
                *node = replacement;
            }
            if removed {
                return Outcome::Removed;
            }
            if skipped {
                return Outcome::Kept;
            }
        }

        self.visit_children(node);

        if self.leave.is_some() {
            let saved = self.context.take();
            if let Some(leave) = self.leave.as_deref_mut() {
                leave(&mut self.context, node);
            }
            let (_, removed, replacement) = self.context.take();
            (
                self.context.should_skip,
                self.context.should_remove,
                self.context.replacement,
            ) = saved;

            if let Some(replacement) = replacement {
                *node = replacement;
            }
            if removed {
                return Outcome::Removed;
            }
        }

        Outcome::Kept
    }

    fn visit_children(&mut self, node: &mut Value) {
        match node {
            Value::Object(map) => {
                for (_key, value) in map.iter_mut() {
                    match value {
                        Value::Array(_) => self.visit_array(value),
                        _ if is_node(value) => {
                            if let Outcome::Removed = self.visit(value) {
                                *value = Value::Null;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Value::Array(_) => self.visit_array(node),
            _ => {}
        }
    }

    fn visit_array(&mut self, value: &mut Value) {
        let Value::Array(items) = value else { return };
        let mut i = 0;
        while i < items.len() {
            if is_node(&items[i])
                && let Outcome::Removed = self.visit(&mut items[i])
            {
                items.remove(i);
                continue;
            }
            i += 1;
        }
    }
}

/// Walk an AST (a node or a body array), invoking `enter` and/or `leave`.
/// Upstream: `walk` (walker/index.ts). Like upstream, the handlers are also
/// invoked for the root value itself — including a root body *array*, which
/// callers must tolerate (see `AstAnalyser.#walkEnter`).
pub fn walk<'a>(
    ast: &mut Value,
    enter: Option<&mut SyncHandler<'a>>,
    leave: Option<&mut SyncHandler<'a>>,
) {
    let mut walker = SyncWalker {
        enter,
        leave,
        context: WalkerContext::default(),
    };
    // Upstream visits the root through the same code path; a removed root is
    // left untouched (there is no parent to remove it from).
    if ast.is_array() {
        if let Some(enter) = walker.enter.as_deref_mut() {
            let mut ctx = WalkerContext::default();
            enter(&mut ctx, ast);
            let (skipped, _, replacement) = ctx.take();
            if let Some(replacement) = replacement {
                *ast = replacement;
            }
            if skipped {
                return;
            }
        }
        walker.visit_children(ast);
        if let Some(leave) = walker.leave.as_deref_mut() {
            let mut ctx = WalkerContext::default();
            leave(&mut ctx, ast);
        }
    } else {
        walker.visit(ast);
    }
}

/// Upstream: `walkEnter`.
pub fn walk_enter(ast: &mut Value, mut enter: impl FnMut(&mut WalkerContext, &mut Value)) {
    walk(ast, Some(&mut enter), None);
}
