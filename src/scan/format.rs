//! Document formats: what config and `--stdin-format` call each one, the
//! extensions it claims, and its scanner.

use std::path::Path;

use super::{Options, Scanned, asciidoc, html, markdown, mdx, org, text, typst};

/// What a document is written in, which decides how it's scanned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// CommonMark, with tables, footnotes and front matter.
    Markdown,
    /// MDX: Markdown with JSX, ESM and expressions.
    Mdx,
    /// Emacs Org mode.
    Org,
    /// Plain text: paragraphs separated by blank lines.
    Text,
    /// HTML.
    Html,
    /// AsciiDoc.
    Asciidoc,
    /// Typst.
    Typst,
}

/// A document format: what config and `--stdin-format` call it, the
/// extensions it claims, and its scanner.
#[derive(Debug)]
pub struct DocumentFormat {
    /// The format this row is for.
    pub format: Format,
    /// What config and `--stdin-format` call it.
    pub name: &'static str,
    /// Lowercase, without the dot.
    pub extensions: &'static [&'static str],
    scan: fn(&str, Options<'_>) -> Scanned,
}

/// Every document format, in `Format` order. A new format is a variant, a
/// row, and its scanner.
pub const DOCUMENTS: &[DocumentFormat] = &[
    DocumentFormat {
        format: Format::Markdown,
        name: "markdown",
        extensions: &["md", "markdown"],
        scan: markdown::scan,
    },
    DocumentFormat {
        format: Format::Mdx,
        name: "mdx",
        extensions: &["mdx"],
        scan: mdx::scan,
    },
    DocumentFormat {
        format: Format::Org,
        name: "org",
        extensions: &["org"],
        scan: org::scan,
    },
    DocumentFormat {
        format: Format::Text,
        name: "text",
        extensions: &["txt"],
        scan: text::scan,
    },
    DocumentFormat {
        format: Format::Html,
        name: "html",
        extensions: &["html", "htm", "xhtml"],
        scan: html::scan,
    },
    DocumentFormat {
        format: Format::Asciidoc,
        name: "asciidoc",
        extensions: &["adoc", "asciidoc"],
        scan: asciidoc::scan,
    },
    DocumentFormat {
        format: Format::Typst,
        name: "typst",
        extensions: &["typ"],
        scan: typst::scan,
    },
];

const _: () = {
    let mut i = 0;
    while i < DOCUMENTS.len() {
        assert!(
            DOCUMENTS[i].format.row() == i,
            "DOCUMENTS must be in `Format` order: `Format::document` indexes it by variant"
        );
        i += 1;
    }
};

impl Format {
    /// Every format, in table order.
    pub fn all() -> impl Iterator<Item = Format> {
        DOCUMENTS.iter().map(|d| d.format)
    }

    /// The format for a file name, or `None` if proserunner doesn't check
    /// that kind of file. Extensions are case-insensitive.
    pub fn of(path: &Path) -> Option<Format> {
        let ext = path.extension()?.to_str()?;
        DOCUMENTS
            .iter()
            .find(|d| d.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
            .map(|d| d.format)
    }

    /// The format called `name` in config and on the command line.
    pub fn named(name: &str) -> Option<Format> {
        DOCUMENTS.iter().find(|d| d.name == name).map(|d| d.format)
    }

    /// What config and `--stdin-format` call it.
    pub fn name(self) -> &'static str {
        self.document().map_or("", |d| d.name)
    }

    /// Every extension proserunner reads without config, in table order.
    pub fn extensions() -> impl Iterator<Item = &'static str> {
        DOCUMENTS.iter().flat_map(|d| d.extensions.iter().copied())
    }

    /// The format's row in `DOCUMENTS`.
    const fn row(self) -> usize {
        match self {
            Format::Markdown => 0,
            Format::Mdx => 1,
            Format::Org => 2,
            Format::Text => 3,
            Format::Html => 4,
            Format::Asciidoc => 5,
            Format::Typst => 6,
        }
    }

    fn document(self) -> Option<&'static DocumentFormat> {
        DOCUMENTS.get(self.row())
    }

    /// Scans `source`, which has no byte order mark, without masking.
    pub(super) fn scan(self, source: &str, options: Options<'_>) -> Scanned {
        self.document()
            .map_or_else(Scanned::default, |d| (d.scan)(source, options))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_by_extension() {
        assert_eq!(Format::of(Path::new("a/README.MD")), Some(Format::Markdown));
        assert_eq!(Format::of(Path::new("page.mdx")), Some(Format::Mdx));
        assert_eq!(Format::of(Path::new("notes.txt")), Some(Format::Text));
        assert_eq!(Format::of(Path::new("todo.org")), Some(Format::Org));
        assert_eq!(Format::of(Path::new("site/index.htm")), Some(Format::Html));
        assert_eq!(Format::of(Path::new("guide.adoc")), Some(Format::Asciidoc));
        assert_eq!(Format::of(Path::new("paper.typ")), Some(Format::Typst));
        assert_eq!(Format::of(Path::new("paper.tex")), None);
        assert_eq!(Format::of(Path::new("Makefile")), None);
    }

    #[test]
    fn formats_by_name() {
        for format in Format::all() {
            assert_eq!(Format::named(format.name()), Some(format));
        }
        assert_eq!(Format::named("markdown"), Some(Format::Markdown));
        assert_eq!(Format::named("md"), None);
        let extensions: Vec<_> = Format::extensions().collect();
        assert_eq!(
            extensions,
            [
                "md", "markdown", "mdx", "org", "txt", "html", "htm", "xhtml", "adoc", "asciidoc",
                "typ"
            ]
        );
    }
}
