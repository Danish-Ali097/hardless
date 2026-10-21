//! Reference docs served as MCP resources (FR-46). Each comes from the same source as the rest
//! of the documentation: the fault catalogue in `hardless-core`, and the README sections of
//! `hardless-escpos` and `hardless-http-dialects`.

use hardless_core::{catalogue, OptionDefault};

/// The section of a Markdown document that starts with the `## heading` line, up to the next
/// `## ` heading.
pub fn section<'a>(markdown: &'a str, heading: &str) -> &'a str {
    let marker = format!("## {heading}");
    let Some(start) = markdown.find(&marker) else {
        return "";
    };
    let rest = &markdown[start..];
    let end = rest[marker.len()..]
        .find("\n## ")
        .map_or(rest.len(), |i| i + marker.len() + 1);
    rest[..end].trim_end()
}

/// `hardless://docs/escpos-commands`: the ESC/POS commands the printer understands, with their
/// byte sequences.
pub fn escpos_commands() -> String {
    format!(
        "# Supported ESC/POS commands\n\nUnknown commands become `unknown` nodes with their raw bytes and a warning; the parser never fails on input.\n\n{}\n",
        section(hardless_escpos::REFERENCE, "Commands")
            .trim_start_matches("## Commands")
            .trim()
    )
}

/// `hardless://docs/http-dialects`: the ePOS-Print-compatible and WebPRNT-compatible request
/// and response reference.
pub fn http_dialects() -> String {
    let reference = hardless_http_dialects::REFERENCE;
    format!(
        "# HTTP printer dialects\n\nEndpoints: `POST /cgi-bin/epos/service.cgi?devid=<id>&timeout=<ms>` (ePOS-Print-compatible) and `POST /StarWebPRNT/SendMessage` or `/p/<deviceId>/StarWebPRNT/SendMessage` (WebPRNT-compatible), on ports 8080 and 8443.\n\n{}\n\n{}\n",
        section(reference, "ePOS-Print-compatible (`epos`)"),
        section(reference, "WebPRNT-compatible (`webprnt`)")
    )
}

/// `hardless://docs/faults`: the fault catalogue (PRD section 10).
pub fn faults() -> String {
    let mut out = String::from(
        "# Fault catalogue\n\nTurn a fault on with `set_fault` (or `PUT /api/v1/devices/<id>/faults/<faultId>`); options are a JSON object.\n\n| Fault ID | Applies to | Effect | Options |\n| --- | --- | --- | --- |\n",
    );
    for spec in catalogue() {
        let options = if spec.options.is_empty() {
            "—".to_owned()
        } else {
            spec.options
                .iter()
                .map(|o| {
                    let default = match o.default {
                        OptionDefault::Integer(n) => n.to_string(),
                        OptionDefault::Number(x) => x.to_string(),
                    };
                    format!("`{}` (default {default}): {}", o.name, o.description)
                })
                .collect::<Vec<_>>()
                .join("; ")
        };
        out.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            spec.id, spec.applies_to, spec.effect, options
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fr_46_reference_docs_come_from_their_sources() {
        assert!(escpos_commands().contains("| Initialize"));
        assert!(escpos_commands().contains("`DLE EOT n`"));
        let dialects = http_dialects();
        assert!(dialects.contains("## ePOS-Print-compatible"));
        assert!(dialects.contains("## WebPRNT-compatible"));
        assert!(!dialects.contains("## License"));
        let faults = faults();
        assert_eq!(faults.matches("\n| `").count(), catalogue().len());
        assert!(faults.contains("`afterBytes` (default 64)"));
    }
}
