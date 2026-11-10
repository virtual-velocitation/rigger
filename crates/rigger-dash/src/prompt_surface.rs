//! The dash's prompt texts (spec 121, criterion 16): every tool description and `inputSchema`
//! property description the MCP server's `tool_list` advertises on its three surfaces, each one
//! `pub const` whose doc comment opens with its class, CONTRACT, since each is the advertised
//! meaning of its tool or argument. THE NAMING RULE names them: a text two or more surfaces
//! advertise word for word is one shared const, `<TOOL>` for a tool's description and
//! `<TOOL>_<ARG>` for a property's; any other text is `<SURFACE>_<TOOL>` or
//! `<SURFACE>_<TOOL>_<ARG>`, its surface prefix `SPAWN_BOUND`, `LOOKUP` or `WORKFLOW`. No text
//! carries a slot. The registry types are `rigger_domain::prompt_surface`'s. The module is pure
//! consts and compiles in every lane.
