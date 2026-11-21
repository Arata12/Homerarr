//! Allowlist sanitizer for Custom Widget template output. Templates are admin-authored but the
//! rendered result (including data from upstream APIs) is shown to every viewer, so this is the
//! security boundary; the save-time checks in `homarr-services` are only early feedback.

use std::collections::{HashMap, HashSet};

use ammonia::{Builder, UrlRelative};

const TAGS: &[&str] = &[
    "div",
    "span",
    "p",
    "br",
    "hr",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "strong",
    "b",
    "em",
    "i",
    "u",
    "s",
    "small",
    "sub",
    "sup",
    "code",
    "pre",
    "blockquote",
    "ul",
    "ol",
    "li",
    "dl",
    "dt",
    "dd",
    "table",
    "thead",
    "tbody",
    "tfoot",
    "tr",
    "th",
    "td",
    "caption",
    "colgroup",
    "col",
    "a",
    "img",
    "figure",
    "figcaption",
    "details",
    "summary",
    "progress",
    "meter",
    "time",
    "abbr",
    "mark",
];

const GENERIC_ATTRIBUTES: &[&str] = &["class", "style", "title", "id", "role", "lang", "dir"];

/// Sanitizes the final rendered HTML of a Custom Widget template.
pub fn sanitize_custom_widget_html(html: &str) -> String {
    let tag_attributes: HashMap<&str, HashSet<&str>> = HashMap::from([
        // `rel` is forced through `link_rel`, so it must not be an allowed attribute.
        ("a", HashSet::from(["href", "target"])),
        (
            "img",
            HashSet::from(["src", "alt", "width", "height", "loading"]),
        ),
        ("td", HashSet::from(["colspan", "rowspan", "scope"])),
        ("th", HashSet::from(["colspan", "rowspan", "scope"])),
        (
            "progress",
            HashSet::from(["value", "max", "min", "low", "high", "optimum"]),
        ),
        (
            "meter",
            HashSet::from(["value", "max", "min", "low", "high", "optimum"]),
        ),
        ("time", HashSet::from(["datetime"])),
        ("details", HashSet::from(["open"])),
        ("col", HashSet::from(["span"])),
        ("colgroup", HashSet::from(["span"])),
    ]);
    Builder::empty()
        .tags(TAGS.iter().copied().collect())
        .clean_content_tags(HashSet::from(["script", "style", "template"]))
        .generic_attributes(GENERIC_ATTRIBUTES.iter().copied().collect())
        .generic_attribute_prefixes(HashSet::from(["aria-", "data-"]))
        .tag_attributes(tag_attributes)
        // `data` is only meant for `<img src="data:image/…">`; the filter below drops it elsewhere.
        .url_schemes(HashSet::from(["http", "https", "mailto", "data"]))
        .url_relative(UrlRelative::PassThrough)
        .link_rel(Some("noopener noreferrer"))
        .attribute_filter(|element, attribute, value| {
            if value.trim_start().to_ascii_lowercase().starts_with("data:") {
                let allowed = element == "img"
                    && attribute == "src"
                    && value
                        .trim_start()
                        .to_ascii_lowercase()
                        .starts_with("data:image/");
                return allowed.then(|| value.into());
            }
            Some(value.into())
        })
        .clean(html)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean(html: &str) -> String {
        sanitize_custom_widget_html(html).to_ascii_lowercase()
    }

    #[test]
    fn event_handlers_are_removed() {
        for html in [
            r#"<img src="x" onerror = "alert(1)">"#,
            r#"<div onmouseover="x()">a</div>"#,
            r#"<img src=x ONERROR=alert(1)>"#,
            r#"<details open ontoggle="x()"><summary>s</summary></details>"#,
        ] {
            let out = clean(html);
            for handler in ["onerror", "onmouseover", "ontoggle"] {
                assert!(!out.contains(handler), "{html} -> {out}");
            }
        }
    }

    #[test]
    fn dangerous_elements_are_removed() {
        for html in [
            r#"<meta http-equiv="refresh" content="0;url=https://evil.example">"#,
            r#"<iframe srcdoc="<script>alert(1)</script>"></iframe>"#,
            r#"<form action="https://evil.example"><input name="p"><button>go</button></form>"#,
            r#"<base href="https://evil.example/">"#,
            r#"<link rel="stylesheet" href="https://evil.example/x.css">"#,
            r#"<script>alert(1)</script>"#,
            r#"<svg><script>alert(1)</script></svg>"#,
            r#"<object data="x"></object><embed src="x">"#,
        ] {
            let out = clean(html);
            for bad in [
                "<meta", "<iframe", "<form", "<base", "<link", "<script", "<svg", "<object",
                "<embed", "<input", "<button", "srcdoc", "alert(1)",
            ] {
                assert!(!out.contains(bad), "{html} -> {out}");
            }
        }
    }

    #[test]
    fn script_urls_are_removed_including_entity_encoded() {
        for html in [
            r#"<a href="javascript:alert(1)">x</a>"#,
            r#"<a href="&#106;avascript:alert(1)">x</a>"#,
            r#"<a href="jav&#x61;script:alert(1)">x</a>"#,
            r#"<a href=" java&#9;script:alert(1)">x</a>"#,
            r#"<a href="data:text/html,<script>alert(1)</script>">x</a>"#,
            r#"<img src="javascript:alert(1)">"#,
        ] {
            let out = clean(html);
            assert!(!out.contains("javascript"), "{html} -> {out}");
            assert!(!out.contains("data:"), "{html} -> {out}");
        }
    }

    #[test]
    fn normal_templates_survive() {
        let html = r#"<div class="box" style="color:red" data-x="1" aria-label="l"><table><thead><tr><th scope="col">A</th></tr></thead><tbody><tr><td colspan="2">1</td></tr></tbody></table><a href="https://example.com/x" target="_blank">link</a> <a href="/rel">rel</a><img src="/i.png" alt="i" width="10"><img src="data:image/png;base64,AAAA"><progress value="1" max="2"></progress></div>"#;
        let out = sanitize_custom_widget_html(html);
        for keep in [
            r#"class="box""#,
            r#"style="color:red""#,
            r#"data-x="1""#,
            r#"aria-label="l""#,
            "<table>",
            r#"scope="col""#,
            r#"colspan="2""#,
            r#"href="https://example.com/x""#,
            r#"target="_blank""#,
            r#"rel="noopener noreferrer""#,
            r#"href="/rel""#,
            r#"src="/i.png""#,
            r#"src="data:image/png;base64,AAAA""#,
            r#"<progress value="1" max="2">"#,
        ] {
            assert!(out.contains(keep), "missing {keep} in {out}");
        }
    }
}
