//! Standalone SVG extraction for chart reports.

/// Extracts the first inline SVG from report HTML and carries over the
/// document's style rules.
pub fn standalone(html: &str) -> Result<String, String> {
    let start = html
        .find("<svg")
        .ok_or_else(|| "the report does not contain an SVG chart".to_string())?;
    let close = html[start..]
        .find("</svg>")
        .ok_or_else(|| "the report's SVG chart is not closed".to_string())?;
    let end = start + close + "</svg>".len();
    let svg = &html[start..end];
    let open_end = svg
        .find('>')
        .ok_or_else(|| "the report's SVG chart has no opening tag".to_string())?
        + 1;
    let (open, body) = svg.split_at(open_end);

    let mut styles = String::new();
    let mut rest = html;
    while let Some(style_start) = rest.find("<style") {
        let after = &rest[style_start..];
        let Some(open_end) = after.find('>') else {
            break;
        };
        let content_start = open_end + 1;
        let Some(close) = after[content_start..].find("</style>") else {
            break;
        };
        styles.push_str(&after[content_start..content_start + close]);
        styles.push('\n');
        rest = &after[content_start + close + "</style>".len()..];
    }

    let mut out = String::with_capacity(svg.len() + styles.len() + 64);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n");
    out.push_str(open);
    if !styles.is_empty() {
        out.push_str("<style type=\"text/css\">");
        out.push_str(&styles);
        out.push_str("</style>");
    }
    out.push_str(body);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> String {
        "<html><head><style>.card rect { fill: white; } @page { margin: 10mm; }</style></head>\
         <body><h1>Chart</h1><svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 10 10\">\
         <rect width=\"10\" height=\"10\" /></svg><p>Footer</p></body></html>"
            .to_string()
    }

    #[test]
    fn extracts_svg_with_declaration_and_styles() {
        let svg = standalone(&document()).expect("svg");
        assert!(svg.starts_with("<?xml version=\"1.0\""));
        assert!(svg.contains("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.contains("<style type=\"text/css\">"));
        assert!(svg.contains(".card rect { fill: white; }"));
        assert!(svg.contains("<rect width=\"10\" height=\"10\" />"));
        assert!(!svg.contains("<html"));
        assert!(!svg.contains("<h1>"));
        assert!(svg.ends_with("</svg>"));
    }

    #[test]
    fn missing_svg_is_an_error() {
        assert!(standalone("<html><body>No chart</body></html>").is_err());
        assert!(standalone("<svg xmlns=\"x\"><rect /></svg>").is_ok());
    }

    #[test]
    fn multiple_style_blocks_are_preserved() {
        let html = "<head><style>.a { color: red; }</style><style>.b { color: blue; }</style></head>\
                    <body><svg xmlns=\"x\"></svg></body>";
        let svg = standalone(html).expect("svg");
        assert!(svg.contains(".a { color: red; }"));
        assert!(svg.contains(".b { color: blue; }"));
    }
}
