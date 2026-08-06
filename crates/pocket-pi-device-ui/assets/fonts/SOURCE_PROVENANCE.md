# PocketJS font atlases

These `.pfa` files were generated with PocketJS's official
`framework/compiler/bake-font.ts` at the same pinned revision used by the
firmware (`4c5dc9ef1dd26e6f49b036c210931d399f2b52b2`). They contain the ASCII glyph
set plus the common smart punctuation `‘’“”–—…`, baked from PocketJS's bundled
Inter Regular and Inter Bold fonts. Model replies commonly use these
typographic characters even when the prompt contains plain ASCII.

- slot 3: Inter Regular, 18 px;
- slot 6: Inter Regular, 36 px;
- slot 10: Inter Bold, 18 px;
- slot 12: Inter Bold, 24 px.

The atlases use PocketJS font-atlas format v3 with one coverage sample per
logical pixel. Inter is distributed under the SIL Open Font License; the full
license is retained as `INTER-LICENSE.txt`.
