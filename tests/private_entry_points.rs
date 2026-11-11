//! The doc-hidden entry points the WebAssembly binding calls in place of
//! public ones (`markdown_syntax::__private`), held to the public calls they
//! stand for over every fixture input and its CRLF variant.

#[path = "support/fixtures.rs"]
#[allow(dead_code)]
mod fixtures;

#[path = "support/leaves.rs"]
mod leaves;

use markdown_syntax::__private::{
    parse_with_leaf_contents, to_markdown_against, to_markdown_against_with, LeafSource,
};
use markdown_syntax::prelude::*;

use fixtures::every_fixture_input;
use std::ops::Range;

/// Each fixture input, with where it is, and its CRLF variant.
fn corpus() -> Vec<(String, String)> {
    every_fixture_input()
        .into_iter()
        .flat_map(|(at, input)| {
            let crlf = input.replace('\n', "\r\n");
            [(format!("{at} (CRLF)"), crlf), (at, input)]
        })
        .collect()
}

/// A leaf's path, text, and runs, as `LeafContents::each` lends them.
type Leaf = (Vec<usize>, String, Vec<(Range<usize>, Range<usize>)>);

fn leaf(path: &[usize], source: LeafSource<'_>) -> Leaf {
    (
        path.to_vec(),
        source.text.to_string(),
        source.runs().collect(),
    )
}

#[test]
fn leaf_sources_read_with_the_parse_are_the_documents() {
    for (at, input) in corpus() {
        let parsed = parse(&input);
        let expected: Vec<Leaf> = parsed
            .document
            .leaf_sources()
            .iter()
            .map(|(path, source)| leaf(&path.0, LeafSource::of(source)))
            .collect();
        let (output, leaves) = parse_with_leaf_contents(&input, |_, contents| {
            let mut leaves = Vec::new();
            contents.each(|path, source| leaves.push(leaf(path, source)));
            leaves
        });
        assert_eq!(format!("{output:?}"), format!("{parsed:?}"), "{at}");
        assert_eq!(leaves, expected, "{at}");
        let (output, (leaves, _)) = parse_with_leaf_contents(&input, |_, contents| {
            let mut leaves = Vec::new();
            let kept = contents.each_keeping(|path, source| leaves.push(leaf(path, source)));
            (leaves, kept)
        });
        assert_eq!(
            format!("{output:?}"),
            format!("{parsed:?}"),
            "{at}: keeping"
        );
        assert_eq!(leaves, expected, "{at}: keeping");
    }
}

/// `parsed` edited in a few ways an editor edits a document, each named.
fn edits(parsed: &Document) -> Vec<(String, Document)> {
    let definitions = Definitions::of(parsed);
    let mut edits = vec![
        ("unchanged".to_string(), parsed.clone()),
        (
            "every leaf built from its text".to_string(),
            leaves::rebuilt_from_leaf_sources(parsed),
        ),
    ];
    let sources = parsed.leaf_sources();
    for (what, leaf) in [
        ("the first leaf", sources.first()),
        ("the last leaf", sources.last()),
    ] {
        let Some((path, source)) = leaf else { continue };
        for (how, text) in [
            ("built from its text", source.text.clone()),
            ("built from edited text", format!("{} edited", source.text)),
        ] {
            let mut tree = parsed.clone();
            leaves::rebuild_at(&mut tree.children, &path.0, &text, &definitions);
            edits.push((format!("{what} {how}"), tree));
        }
    }
    if !parsed.children.is_empty() {
        let mut tree = parsed.clone();
        tree.children.remove(parsed.children.len() / 2);
        edits.push(("the middle block removed".to_string(), tree));
    }
    let mut tree = parsed.clone();
    let list = tree.children.iter_mut().find_map(|block| match block {
        Block::List(list) if list.children.len() > 1 => Some(list),
        _ => None,
    });
    if let Some(list) = list {
        list.children.pop();
        edits.push(("a list's last item removed".to_string(), tree));
    }
    edits
}

/// Writing against the parse writes as the document does, with the leaf
/// sources read again from the source and with the ones kept from the parse;
/// debug builds also check that the kept ones are the ones read again.
#[test]
fn writing_against_the_parse_writes_as_the_document_does() {
    let mut changed = SerializeOptions::default();
    changed.line_ending = LineEnding::CrLf;
    changed.bullet = Some(BulletMarker::Asterisk);
    let options = [SerializeOptions::default(), changed];
    for (at, input) in corpus() {
        let (output, kept) =
            parse_with_leaf_contents(&input, |_, contents| contents.each_keeping(|_, _| {}));
        let parsed = output.document;
        let mut edits = edits(&parsed);
        edits.push((
            "without its source".to_string(),
            parsed.clone().without_source(),
        ));
        for (edit, tree) in edits {
            let against = if tree.source().is_some() {
                parsed.clone()
            } else {
                parsed.clone().without_source()
            };
            for options in &options {
                let written = tree.to_markdown_with(options);
                assert_eq!(
                    to_markdown_against(&tree, &against, options),
                    written,
                    "{at}: {edit}"
                );
                if against.source().is_some() {
                    assert_eq!(
                        to_markdown_against_with(&tree, &against, &kept, options),
                        written,
                        "{at}: {edit}, with the kept sources"
                    );
                }
            }
        }
    }
}

/// The binding's write does not read a kept text again: a leaf built from its
/// text with other definitions and read again with the tree's
/// (`reread_kept_sources`), as the binding decodes it, is written as the
/// public write writes it, as its text.
#[test]
fn a_leaf_read_again_with_the_trees_definitions_is_written_as_its_text() {
    let input = "[a]  *b*\n\n[a]: /u\n";
    let (output, kept) =
        parse_with_leaf_contents(input, |_, contents| contents.each_keeping(|_, _| {}));
    let parsed = output.document;
    let mut tree = parsed.clone();
    leaves::rebuild_at(
        &mut tree.children,
        &[0],
        "[a]  *b*",
        &Definitions::default(),
    );
    tree.reread_kept_sources();
    let options = SerializeOptions::default();
    assert_eq!(tree.to_markdown_with(&options).as_deref(), Ok(input));
    assert_eq!(
        to_markdown_against_with(&tree, &parsed, &kept, &options).as_deref(),
        Ok(input)
    );
}

/// A leaf whose inlines changed after it was built from its text is written
/// from its inlines by the public write, which reads the text again; the
/// binding's write takes the text as reading as the inlines, and debug
/// builds panic on it, naming the leaf.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "the leaf at [0] keeps a text that does not read as its inlines")]
fn a_leaf_changed_after_its_text_panics_the_bindings_write_in_debug_builds() {
    let input = "a  *b*\n";
    let (output, kept) =
        parse_with_leaf_contents(input, |_, contents| contents.each_keeping(|_, _| {}));
    let parsed = output.document;
    let mut tree = parsed.clone();
    leaves::rebuild_at(
        &mut tree.children,
        &[0],
        "a  *b*",
        &Definitions::of(&parsed),
    );
    let Block::Paragraph(paragraph) = &mut tree.children[0] else {
        unreachable!()
    };
    paragraph.children.push(Text::from("!").into());
    assert_eq!(tree.to_markdown().as_deref(), Ok("a  *b*!\n"));
    let _ = to_markdown_against_with(&tree, &parsed, &kept, &SerializeOptions::default());
}
