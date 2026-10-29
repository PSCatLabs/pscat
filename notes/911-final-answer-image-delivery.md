# 911: final-answer image delivery

A Codex desktop trial of plugin 0.7.0-rc.1 rendered a spiral, got the PNG back
from `render_postscript`, and answered with a source link and the claim that it
had rendered. The user saw nothing ("yeah I don't see it though .. Can you show
me?"). Saving the PNG and putting its absolute path in the final answer fixed
it. The PNG was valid; this was a delivery failure by the agent, not a renderer
or MCP defect.

## What shipped

A short "Show the result to the user" section in `plugin/skills/pscat/SKILL.md`,
three lines each in `psart` and `psviz`, and a "Plugin desktop smoke check" in
`docs/dev/RELEASING.md` (the smoke prompt, the follow-up, separate render and
delivery rows).

## What the hosts' documentation says (checked 2026-10-29)

Little. Each claim is marked documented or observed.

- Codex desktop: the docs describe previews of documents, slides, spreadsheets
  and PDFs, a viewer for generated images, and file cards with an Open control
  (learn.chatgpt.com/docs/artifacts-viewer, /docs/image-generation). They do not
  say whether a markdown image with an absolute path renders in an answer, or
  whether an MCP tool's image block shows outside the trace. Observed in the
  #911 trial: `![](absolute path)` showed the image; the tool-call image did not.
- Claude desktop, Code tab: "To open a file Claude mentions, click its path in
  the chat" (code.claude.com/docs/en/desktop). Images are documented as user
  input only. Nothing says tool-result images or markdown images show inline.
- Cowork: local MCP servers load when the session runs on the user's computer
  (claude.com/docs/plugins/platform-support); Claude reads and writes local
  files (claude.com/docs/cowork/overview). How an image reaches the chat is not
  documented.
- Claude Code in a terminal: images are documented as input (paste), not output.

So the skill states the rule that the trial showed working for Codex, gives the
Claude hosts an absolute path as a link without claiming inline display, and says
where the file is in a terminal. Revisit when a host documents image output.

`out_path`'s reply is `wrote <absolute path>` (`wrote_message` in
`src/mcp_server.rs`), so the skill tells the agent to use that path.
