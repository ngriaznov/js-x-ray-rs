//! Upstream: `src/utils/isOneLineExpressionExport.ts`

use serde_json::Value;

use crate::estree::{
    call_expression_identifier, is_call_expression, is_member_expression, is_type,
};

pub fn is_one_line_expression_export(body: &[Value]) -> bool {
    let [first_node] = body else {
        return false;
    };
    if !is_type(first_node, "ExpressionStatement") {
        return false;
    }
    let Some(expression) = first_node.get("expression") else {
        return false;
    };

    match expression.get("type").and_then(Value::as_str) {
        // module.exports = require('...');
        Some("AssignmentExpression") => expression
            .get("right")
            .is_some_and(export_assignment_has_require_leave),
        // require('...');
        Some("CallExpression") => export_assignment_has_require_leave(expression),
        _ => false,
    }
}

fn export_assignment_has_require_leave(expr: &Value) -> bool {
    if is_type(expr, "LogicalExpression") {
        return at_least_one_branch_has_require_leave(expr.get("left"), expr.get("right"));
    }
    if is_type(expr, "ConditionalExpression") {
        return at_least_one_branch_has_require_leave(
            expr.get("consequent"),
            expr.get("alternate"),
        );
    }
    if is_call_expression(expr) {
        return call_expression_identifier(expr).as_deref() == Some("require");
    }
    if is_member_expression(expr) {
        let mut root_member = expr.get("object").unwrap_or(&Value::Null);
        while is_member_expression(root_member) {
            root_member = root_member.get("object").unwrap_or(&Value::Null);
        }
        if !is_call_expression(root_member) {
            return false;
        }
        return call_expression_identifier(root_member).as_deref() == Some("require");
    }
    false
}

fn at_least_one_branch_has_require_leave(left: Option<&Value>, right: Option<&Value>) -> bool {
    [left, right]
        .into_iter()
        .flatten()
        .any(export_assignment_has_require_leave)
}
