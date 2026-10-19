//! Reading request documents: size limit, no DTDs, namespace-agnostic element names.

use std::str::FromStr;

use roxmltree::{Document, Node, ParsingOptions};

use crate::{DialectError, MAX_DOCUMENT_BYTES};

/// Parses a request document. DTDs are refused, so entity tricks (billion laughs, external
/// entities) cannot reach the emulator.
pub(crate) fn parse(text: &str) -> Result<Document<'_>, DialectError> {
    if text.len() > MAX_DOCUMENT_BYTES {
        return Err(DialectError::new(format!(
            "the document is larger than {MAX_DOCUMENT_BYTES} bytes"
        )));
    }
    let options = ParsingOptions {
        allow_dtd: false,
        nodes_limit: 1_000_000,
        ..ParsingOptions::default()
    };
    Document::parse_with_options(text, options)
        .map_err(|e| DialectError::new(format!("not well-formed XML: {e}")))
}

/// Element children of `node`.
pub(crate) fn elements<'a, 'i>(node: Node<'a, 'i>) -> impl Iterator<Item = Node<'a, 'i>> {
    node.children().filter(Node::is_element)
}

/// The first element child named `name`, in any namespace.
pub(crate) fn child<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    elements(node).find(|n| n.tag_name().name() == name)
}

/// An attribute, in any namespace.
pub(crate) fn attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|a| a.name() == name)
        .map(|a| a.value())
}

/// A boolean attribute: `true`/`1` or `false`/`0`.
pub(crate) fn flag(node: Node<'_, '_>, name: &str) -> Result<Option<bool>, DialectError> {
    match attr(node, name) {
        None => Ok(None),
        Some("true" | "1") => Ok(Some(true)),
        Some("false" | "0") => Ok(Some(false)),
        Some(other) => Err(bad(node, name, other)),
    }
}

/// A numeric attribute.
pub(crate) fn number<T: FromStr>(
    node: Node<'_, '_>,
    name: &str,
) -> Result<Option<T>, DialectError> {
    match attr(node, name) {
        None => Ok(None),
        Some(value) => value
            .trim()
            .parse()
            .map(Some)
            .map_err(|_| bad(node, name, value)),
    }
}

/// The text content directly inside `node`.
pub(crate) fn text(node: Node<'_, '_>) -> String {
    node.children()
        .filter(Node::is_text)
        .filter_map(|n| n.text())
        .collect()
}

/// An attribute value the dialect does not allow.
pub(crate) fn bad(node: Node<'_, '_>, name: &str, value: &str) -> DialectError {
    DialectError::new(format!(
        "<{}> has an invalid {name}=\"{value}\"",
        node.tag_name().name()
    ))
}

/// Escapes text for an XML attribute or element.
pub(crate) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dtds_are_refused() {
        let doc = r#"<!DOCTYPE x [<!ENTITY a "aaaa">]><x>&a;</x>"#;
        assert!(parse(doc).is_err());
    }

    #[test]
    fn oversized_documents_are_refused() {
        let doc = format!("<x>{}</x>", "a".repeat(MAX_DOCUMENT_BYTES));
        assert!(parse(&doc).is_err());
    }
}
