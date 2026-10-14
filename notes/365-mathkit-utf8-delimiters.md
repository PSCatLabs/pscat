## mathkit: UTF-8 delimiters after \left, \right and \big (issue #365, 2026-10-14)

Found while working #339. `\left⟨ x \right⟩` (the characters, not `\langle`)
reached `mkdelim`'s last branch, which took one ASCII byte, so the error was
`mk-unsupported-delimiter:<0xE2>`: a raw lead byte in an error name.

### What changed (lib/mathkit.ps)

- `mkutf8` split into `mkutf8str` (lead byte to the whole UTF-8 character,
  `mkpos` advanced, cut short at the end of the source) and `mkutf8name`
  (character to strokeglyphs' glyph name). `mkutf8` behaves as before.
- `mkdelim`: a byte of 128 or more goes to the new `mkdelimutf8`, which
  accepts a character whose glyph name is in `mkdelimglyphs`: the ten
  symbols that grow (the parens, brackets, braces, `bar`, `angleleft`,
  `angleright`, `dblbar`). So `⟨ ⟩ 〈 〉` give the angle brackets and `‖ ∥`
  the double bar. This is an explicit set, not `sggrownames`, which also
  holds `/summation /integral /radical ...`; `\left∑` stays refused.
- Anything else is `mk-unsupported-delimiter:<whole character>`, for example
  `mk-unsupported-delimiter:∮`. The `\` then non-letter branch (`\left\⟨`)
  names the whole character too: `mk-unsupported-delimiter:\⟨`.
- The header's supported-subset list mentions the typed forms.

### Not changed

- ASCII errors (`\left<` is `mk-unsupported-delimiter:<`, `\left x` is `:x`).
- Malformed UTF-8 (a lone continuation byte, a lead byte cut off by the end
  of the source) still puts its raw bytes in the name: there is no character
  to name. The sequence is taken as long as its lead byte says, as `mkutf8`
  does for ordinary text.
- The ASCII `*` and `^*` (#364) are a separate ticket.

### Tests

`%%SelfTest: delimiters-typed-as-unicode` (typed forms lay out the same as
their commands, for `\left`, `\right`, `\bigl`, `\bigr` and a `.` partner;
the refusals name the whole character), and a stack-unwinding case in
`errors-leave-the-stacks-as-they-were`.
