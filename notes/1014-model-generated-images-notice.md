# 1014: notice for the AI-generated images in examples/hybrid

`THIRD_PARTY_NOTICES.md` mapped `examples/` to "original creative work, CC0".
The `examples/hybrid/` JPEGs (#995) are image-model output, which that row
did not say. A row under "Exceptions" now covers:

- every AI-generated image in `examples/hybrid/` as its README lists them
  under "Where the images came from" (today the `*_bg.jpg` and `*_model.jpg`
  files; worded so images added by later tickets are covered without
  editing the entry);
- the images that contain them: `docs/guides/hybrid/*_compare.png` and
  `gallery/renders/hybrid_*.png`.

Terms stated: CC0-1.0 to the extent any rights exist, no warranty; OpenAI's
terms assign its rights in the output to the user; provenance and prompts are
in `examples/hybrid/README.md`. The wording is a draft for the owner.

Not changed: `licenses/assets.json` still maps `examples/**` to CC0-1.0 with
a note calling it "owner-owned original creative expression"; the owner may
want that note to say "except AI-generated images" too.
