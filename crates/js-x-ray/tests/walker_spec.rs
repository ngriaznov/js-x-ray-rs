//! Upstream: `test/walker.spec.ts`

use js_x_ray::estree::is_identifier;
use js_x_ray::walker::{WalkerContext, walk, walk_enter};
use serde_json::{Value, json};

#[test]
fn walks_a_malformed_node() {
    let expected_answer = json!({ "type": "Answer", "value": 42 });
    let mut ast = json!({
        "type": "Test",
        "block": [
            { "type": "Foo", "answer": Value::Null },
            { "type": "Foo", "answer": expected_answer }
        ]
    });
    let expected_answer = ast.pointer("/block/1/answer").unwrap().clone();

    let mut answer: Option<Value> = None;
    walk_enter(&mut ast, |_ctx, node| {
        if node.get("type").and_then(Value::as_str) == Some("Answer") {
            answer = Some(node.clone());
        }
    });

    assert_eq!(answer, Some(expected_answer));
}

#[test]
fn walks_an_ast() {
    let mut ast = json!({
        "type": "Program",
        "body": [
            {
                "type": "VariableDeclaration",
                "declarations": [
                    {
                        "type": "VariableDeclarator",
                        "id": { "type": "Identifier", "name": "a" },
                        "init": { "type": "Literal", "value": 1, "raw": "1" }
                    },
                    {
                        "type": "VariableDeclarator",
                        "id": { "type": "Identifier", "name": "b" },
                        "init": { "type": "Literal", "value": 2, "raw": "2" }
                    }
                ],
                "kind": "var"
            }
        ],
        "sourceType": "module"
    });

    // Snapshot the sub-nodes up front: nothing in this test mutates the
    // tree, so these clones double as the expected pre-order/post-order
    // sequences without relying on JS-style object identity (`Value` only
    // supports structural equality).
    let root = ast.clone();
    let body0 = ast.pointer("/body/0").unwrap().clone();
    let decl0 = ast.pointer("/body/0/declarations/0").unwrap().clone();
    let decl0_id = ast.pointer("/body/0/declarations/0/id").unwrap().clone();
    let decl0_init = ast.pointer("/body/0/declarations/0/init").unwrap().clone();
    let decl1 = ast.pointer("/body/0/declarations/1").unwrap().clone();
    let decl1_id = ast.pointer("/body/0/declarations/1/id").unwrap().clone();
    let decl1_init = ast.pointer("/body/0/declarations/1/init").unwrap().clone();

    let mut entered: Vec<Value> = Vec::new();
    let mut left: Vec<Value> = Vec::new();

    walk(
        &mut ast,
        Some(&mut |_ctx: &mut WalkerContext, node: &mut Value| entered.push(node.clone())),
        Some(&mut |_ctx: &mut WalkerContext, node: &mut Value| left.push(node.clone())),
    );

    assert_eq!(
        entered,
        vec![
            root.clone(),
            body0.clone(),
            decl0.clone(),
            decl0_id.clone(),
            decl0_init.clone(),
            decl1.clone(),
            decl1_id.clone(),
            decl1_init.clone(),
        ]
    );
    assert_eq!(
        left,
        vec![
            decl0_id, decl0_init, decl0, decl1_id, decl1_init, decl1, body0, root
        ]
    );
}

#[test]
fn handles_null_literals() {
    let mut ast = json!({
        "type": "Program",
        "body": [
            {
                "type": "ExpressionStatement",
                "expression": { "type": "Literal", "value": Value::Null, "raw": "null" }
            },
            {
                "type": "ExpressionStatement",
                "expression": { "type": "Literal", "value": 1, "raw": "1" }
            }
        ],
        "sourceType": "module"
    });

    // Only asserts that walking a `Literal` whose `value` is JSON `null`
    // does not panic (a null value is not itself a node, so it must not be
    // mistaken for a removed child).
    walk(
        &mut ast,
        Some(&mut |_ctx, _node| {}),
        Some(&mut |_ctx, _node| {}),
    );
}

#[test]
fn allows_walk_to_be_invoked_within_a_walk_without_context_corruption() {
    let mut ast = json!({
        "type": "Program",
        "body": [
            {
                "type": "ExpressionStatement",
                "expression": {
                    "type": "BinaryExpression",
                    "left": { "type": "Identifier", "name": "a" },
                    "operator": "+",
                    "right": { "type": "Identifier", "name": "b" }
                }
            }
        ],
        "sourceType": "module"
    });

    let mut identifiers: Vec<String> = Vec::new();
    walk_enter(&mut ast, |_ctx, node| {
        if node.get("type").and_then(Value::as_str) == Some("ExpressionStatement") {
            walk_enter(node, |inner_ctx, _inner_node| inner_ctx.skip());
        }

        if is_identifier(node)
            && let Some(name) = node.get("name").and_then(Value::as_str)
        {
            identifiers.push(name.to_owned());
        }
    });

    assert_eq!(identifiers, vec!["a".to_owned(), "b".to_owned()]);
}

#[test]
fn replaces_a_node() {
    let forty_two = json!({ "type": "Literal", "value": 42, "raw": "42" });

    for phase_is_enter in [true, false] {
        let mut ast = json!({
            "type": "Program",
            "body": [
                {
                    "type": "ExpressionStatement",
                    "expression": {
                        "type": "BinaryExpression",
                        "left": { "type": "Identifier", "name": "a" },
                        "operator": "+",
                        "right": { "type": "Identifier", "name": "b" }
                    }
                }
            ],
            "sourceType": "module"
        });

        let mut handler = |ctx: &mut WalkerContext, node: &mut Value| {
            if is_identifier(node) && node.get("name").and_then(Value::as_str) == Some("b") {
                ctx.replace(forty_two.clone());
            }
        };

        if phase_is_enter {
            walk(&mut ast, Some(&mut handler), None);
        } else {
            walk(&mut ast, None, Some(&mut handler));
        }

        assert_eq!(ast.pointer("/body/0/expression/right").unwrap(), &forty_two);
    }
}

#[test]
fn replaces_a_top_level_node() {
    let mut ast = json!({ "type": "Identifier", "name": "answer" });
    let forty_two = json!({ "type": "Literal", "value": 42, "raw": "42" });

    walk_enter(&mut ast, |ctx, node| {
        if is_identifier(node) && node.get("name").and_then(Value::as_str) == Some("answer") {
            ctx.replace(forty_two.clone());
        }
    });

    // Upstream's `walk()` returns the (possibly replaced) root; the Rust
    // walker mutates the root `Value` in place through `&mut` instead.
    assert_eq!(ast, forty_two);
}

#[test]
fn removes_a_node_property() {
    for phase_is_enter in [true, false] {
        let mut ast = json!({
            "type": "Program",
            "body": [
                {
                    "type": "ExpressionStatement",
                    "expression": {
                        "type": "BinaryExpression",
                        "left": { "type": "Identifier", "name": "a" },
                        "operator": "+",
                        "right": { "type": "Identifier", "name": "b" }
                    }
                }
            ],
            "sourceType": "module"
        });

        let mut handler = |ctx: &mut WalkerContext, node: &mut Value| {
            if is_identifier(node) && node.get("name").and_then(Value::as_str) == Some("b") {
                ctx.remove();
            }
        };

        if phase_is_enter {
            walk(&mut ast, Some(&mut handler), None);
        } else {
            walk(&mut ast, None, Some(&mut handler));
        }

        // Adaptation: JS deletes the property, leaving it `undefined`; the
        // Rust walker has no "absent key" concept for object fields, so a
        // removed child becomes JSON `null` (see `walker::visit_children`).
        assert_eq!(
            ast.pointer("/body/0/expression/right").unwrap(),
            &Value::Null
        );
    }
}

#[test]
fn removes_a_node_from_array() {
    for phase_is_enter in [true, false] {
        let mut ast = json!({
            "type": "Program",
            "body": [
                {
                    "type": "VariableDeclaration",
                    "declarations": [
                        { "type": "VariableDeclarator", "id": { "type": "Identifier", "name": "a" }, "init": Value::Null },
                        { "type": "VariableDeclarator", "id": { "type": "Identifier", "name": "b" }, "init": Value::Null },
                        { "type": "VariableDeclarator", "id": { "type": "Identifier", "name": "c" }, "init": Value::Null }
                    ],
                    "kind": "let"
                }
            ],
            "sourceType": "module"
        });

        // Adaptation: upstream also records `ctx.index` (the declarator's
        // live array index at visit time) via the walker's third callback
        // argument; the Rust `SyncHandler` exposes no such per-visit index,
        // so this only re-derives the portable half of the upstream
        // assertion — every original node is still visited exactly once,
        // in place, despite earlier siblings being spliced out mid-walk.
        let mut visited_count = 0usize;
        let mut handler = |ctx: &mut WalkerContext, node: &mut Value| {
            if node.get("type").and_then(Value::as_str) == Some("VariableDeclarator") {
                visited_count += 1;
                let name = node.pointer("/id/name").and_then(Value::as_str);
                if name == Some("a") || name == Some("b") {
                    ctx.remove();
                }
            }
        };

        if phase_is_enter {
            walk(&mut ast, Some(&mut handler), None);
        } else {
            walk(&mut ast, None, Some(&mut handler));
        }

        let declarations = ast
            .pointer("/body/0/declarations")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(declarations.len(), 1);
        assert_eq!(declarations[0].pointer("/id/name").unwrap(), "c");
        assert_eq!(visited_count, 3);
    }
}
