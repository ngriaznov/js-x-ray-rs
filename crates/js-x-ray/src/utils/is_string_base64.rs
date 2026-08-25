//! Upstream: `src/utils/isStringBase64.ts`

use std::sync::LazyLock;

use regex::Regex;

#[derive(Debug, Default, Clone, Copy)]
pub struct Base64Options {
    pub allow_mime: Option<bool>,
    pub mime_required: Option<bool>,
    pub padding_required: Option<bool>,
    pub allow_empty: Option<bool>,
}

macro_rules! lazy_regex {
    ($name:ident, $pattern:expr) => {
        static $name: LazyLock<Regex> =
            LazyLock::new(|| Regex::new($pattern).expect("valid regex"));
    };
}

lazy_regex!(
    DEFAULT_BASE64,
    r"(?i)^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$"
);
lazy_regex!(
    BASE64_NO_PADDING,
    r"(?i)^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}(?:==)?|[A-Za-z0-9+/]{3}=?)?$"
);
lazy_regex!(
    BASE64_ALLOW_MIME,
    r"(?i)^(?:data:\w+/[a-zA-Z+\-.]+;base64,)?(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$"
);
lazy_regex!(
    BASE64_ALLOW_MIME_NO_PADDING,
    r"(?i)^(?:data:\w+/[a-zA-Z+\-.]+;base64,)?(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}(?:==)?|[A-Za-z0-9+/]{3}=?)?$"
);
lazy_regex!(
    BASE64_REQUIRE_MIME,
    r"(?i)^(?:data:\w+/[a-zA-Z+\-.]+;base64,)(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$"
);
lazy_regex!(
    BASE64_REQUIRE_MIME_NO_PADDING,
    r"(?i)^(?:data:\w+/[a-zA-Z+\-.]+;base64,)(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}(?:==)?|[A-Za-z0-9+/]{3}=?)?$"
);

pub fn is_string_base64(v: &str, opts: Base64Options) -> bool {
    if opts.allow_empty == Some(false) && v.is_empty() {
        return false;
    }

    if opts.mime_required == Some(true) {
        return if opts.padding_required == Some(false) {
            BASE64_REQUIRE_MIME_NO_PADDING.is_match(v)
        } else {
            BASE64_REQUIRE_MIME.is_match(v)
        };
    }

    if opts.allow_mime == Some(true) {
        return if opts.padding_required == Some(false) {
            BASE64_ALLOW_MIME_NO_PADDING.is_match(v)
        } else {
            BASE64_ALLOW_MIME.is_match(v)
        };
    }

    if opts.padding_required == Some(false) {
        return BASE64_NO_PADDING.is_match(v);
    }

    DEFAULT_BASE64.is_match(v)
}
