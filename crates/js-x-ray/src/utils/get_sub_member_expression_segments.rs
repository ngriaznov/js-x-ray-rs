//! Upstream: `src/utils/getSubMemberExpressionSegments.ts`

/// For `"a.b.c"` yields `["a", "a.b"]`.
pub fn get_sub_member_expression_segments(member_expression_fullpath: &str) -> Vec<String> {
    let identifiers: Vec<&str> = member_expression_fullpath.split('.').collect();
    let mut segments = Vec::new();
    let mut out = Vec::new();
    for identifier in identifiers.iter().take(identifiers.len().saturating_sub(1)) {
        segments.push(*identifier);
        out.push(segments.join("."));
    }
    out
}
