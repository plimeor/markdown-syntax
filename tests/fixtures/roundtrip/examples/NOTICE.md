# CommonMark Official Examples Notice

`official-inputs.cases` and `official-stable-inputs.cases` are derived from the
generated CommonMark suite copied at
`../oracles/upstream-tests/commonmark.rs`.

The generated suite originates in the CommonMark specification examples. The
CommonMark specification is authored by John MacFarlane and licensed under the
Creative Commons Attribution-ShareAlike 4.0 International license
(`CC-BY-SA-4.0`):

- https://spec.commonmark.org/
- https://creativecommons.org/licenses/by-sa/4.0/

Only Markdown inputs are used by this package. Expected HTML renderings from the
upstream generated suite are intentionally not used because `markdown-syntax`
does not provide an HTML renderer.
