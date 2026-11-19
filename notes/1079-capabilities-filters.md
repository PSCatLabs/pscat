## Filters for `--capabilities` and `describe_art_capabilities` (issue #1079, 2026-11-18)

A cloud session called `describe_art_capabilities` first, as the skill says, and
got 267,000 characters, which the client cut at 100,000, in the middle of the
parameters it came for.

**Built.** `--capabilities` takes `--name NAME` (repeatable), `--kind KIND`,
`--source FILE` and `--brief` right after it; the MCP tool takes `names`, `kind`,
`source` and `brief` and passes them as those flags, so the two answer alike.
`capabilities::CapabilityFilter` and `payload_json_filtered` do the work;
`CapabilityKind::ALL` is the one list of kinds (CLI error, MCP schema `enum`, a
test that it matches the kinds the catalog carries).

**Sizes** (this checkout's catalog, 487 entries of which 157 are fonts):

| call | bytes |
|---|---|
| no arguments (unchanged) | 319,327 |
| `brief` | 76,868 |
| `brief` + `kind: procedure` | 44,996 |
| `brief` + `source: paintkit` | 7,091 |
| `brief` + `kind: palette` | 3,824 |
| `names: ["pkwash"]` | 2,672 |

`brief` keeps the first sentence of the description, cut at 110 characters. Plain
`brief` was first tried with the whole description and was 137 KB, which still
truncates, so it is the first sentence only.

**Decisions.**
- The default, with no arguments, is byte for byte what it was. The hosted server
  and any client that parses the payload keep working; the fix for the first call
  is the tool description and the skills telling an agent to ask for `brief` or
  `names`.
- A filtered payload adds `matched`, `total` and `unmatched_names`; an unfiltered one
  has none of them. The `catalog_signature` is the whole catalog's.
- `--source` matches by exact path, file name or stem, ignoring case. A substring
  match would make `paint` select `paintkit`, `paintbrush` and whatever comes next.
- The filter flags are read only straight after `--capabilities`; anywhere else
  `--name` is still an unknown option.
- Wildcards and `--search TEXT` were not added: an agent that does not know a
  name uses `kind` + `brief` and reads the list.

**Not done.** The ChatGPT cloud skills (`plugin-chatgpt/cloud/`) are not told about
the new arguments: they talk to the hosted pscat-mcp server, which does not have
them yet. The new paragraphs in `plugin/skills/` are dropped from that build by the
overlay.
