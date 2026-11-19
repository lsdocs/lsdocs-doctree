# lsdocs-doctree

Part of [lsdocs](https://github.com/lsdocs), a static-site generator whose
parsers, transformers and renderers are plugins.

The document tree every lsdocs plugin exchanges: `Document`, `Node` (the
core node kinds plus `Custom`), byte-offset `Span`s and front-matter
metadata, with serde only. The serialized shape is part of the wire
protocol (see `spec/wire.md` in
[lsdocs-protocol](https://github.com/lsdocs/lsdocs-protocol)), and the
crate's documentation describes the shape every parser produces (nested
sections, explicit ids only, link targets).

## Known limitations

- Tables have no cell spans (`morecols`, `morerows`) and no stub columns,
  so parsers drop them.
- Images have no `width`, `height`, `scale` or `align`; figures no
  `figwidth` or `align`.
- No inline kinds for `subscript`, `superscript`, `abbreviation`,
  `acronym` or classed `inline` spans; parsers fall back to their text (or
  to an inline `custom` node, `inline: true`, which renderers draw as an
  inline element).
- Topic and sidebar titles have no node of their own (parsers write bold
  paragraphs).

## TODO

- [ ] Cell spans and stub columns for tables.
- [ ] Image and figure sizing and alignment.
- [ ] Inline kinds for sub/superscript and abbreviations (or settle on
      the inline `custom` convention for them).
