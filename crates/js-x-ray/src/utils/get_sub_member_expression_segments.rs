//! Upstream: `src/utils/getSubMemberExpressionSegments.ts`

/// For `"a.b.c"` yields `["a", "a.b"]`.
pub fn get_sub_member_expression_segments(member_expression_fullpath: &str) -> Vec<String> {
    let parts: Vec<&str> = member_expression_fullpath.split('.').collect();
    (1..parts.len()).map(|len| parts[..len].join(".")).collect()
}
