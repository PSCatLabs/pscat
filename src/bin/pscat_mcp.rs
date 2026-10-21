//! pscat-mcp — pscat as MCP tools. The server is `src/mcp_server.rs`,
//! shared with `pscat mcp` (#569).

#[path = "../mcp_server.rs"]
mod mcp_server;

fn main() -> std::process::ExitCode {
    mcp_server::run(std::env::args().skip(1))
}
