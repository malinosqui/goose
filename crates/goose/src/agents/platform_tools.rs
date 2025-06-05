use indoc::indoc;
use mcp_core::tool::{Tool, ToolAnnotations};
use serde_json::json;

pub const PLATFORM_READ_RESOURCE_TOOL_NAME: &str = "platform__read_resource";
pub const PLATFORM_LIST_RESOURCES_TOOL_NAME: &str = "platform__list_resources";
pub const PLATFORM_SEARCH_AVAILABLE_EXTENSIONS_TOOL_NAME: &str =
    "platform__search_available_extensions";
pub const PLATFORM_MANAGE_EXTENSIONS_TOOL_NAME: &str = "platform__manage_extensions";
pub const PLATFORM_SPAWN_INTERACTIVE_SUBAGENT_TOOL_NAME: &str = "platform__spawn_interactive_subagent";
pub const PLATFORM_LIST_SUBAGENTS_TOOL_NAME: &str = "platform__list_subagents";
pub const PLATFORM_GET_SUBAGENT_STATUS_TOOL_NAME: &str = "platform__get_subagent_status";
pub const PLATFORM_TERMINATE_SUBAGENT_TOOL_NAME: &str = "platform__terminate_subagent";

pub fn read_resource_tool() -> Tool {
    Tool::new(
        PLATFORM_READ_RESOURCE_TOOL_NAME.to_string(),
        indoc! {r#"
            Read a resource from an extension.

            Resources allow extensions to share data that provide context to LLMs, such as
            files, database schemas, or application-specific information. This tool searches for the
            resource URI in the provided extension, and reads in the resource content. If no extension
            is provided, the tool will search all extensions for the resource.
        "#}.to_string(),
        json!({
            "type": "object",
            "required": ["uri"],
            "properties": {
                "uri": {"type": "string", "description": "Resource URI"},
                "extension_name": {"type": "string", "description": "Optional extension name"}
            }
        }),
        Some(ToolAnnotations {
            title: Some("Read a resource".to_string()),
            read_only_hint: true,
            destructive_hint: false,
            idempotent_hint: false,
            open_world_hint: false,
        }),
    )
}

pub fn list_resources_tool() -> Tool {
    Tool::new(
        PLATFORM_LIST_RESOURCES_TOOL_NAME.to_string(),
        indoc! {r#"
            List resources from an extension(s).

            Resources allow extensions to share data that provide context to LLMs, such as
            files, database schemas, or application-specific information. This tool lists resources
            in the provided extension, and returns a list for the user to browse. If no extension
            is provided, the tool will search all extensions for the resource.
        "#}
        .to_string(),
        json!({
            "type": "object",
            "properties": {
                "extension_name": {"type": "string", "description": "Optional extension name"}
            }
        }),
        Some(ToolAnnotations {
            title: Some("List resources".to_string()),
            read_only_hint: true,
            destructive_hint: false,
            idempotent_hint: false,
            open_world_hint: false,
        }),
    )
}

pub fn search_available_extensions_tool() -> Tool {
    Tool::new(
        PLATFORM_SEARCH_AVAILABLE_EXTENSIONS_TOOL_NAME.to_string(),
        "Searches for additional extensions available to help complete tasks.
        Use this tool when you're unable to find a specific feature or functionality you need to complete your task, or when standard approaches aren't working.
        These extensions might provide the exact tools needed to solve your problem.
        If you find a relevant one, consider using your tools to enable it.".to_string(),
        json!({
            "type": "object",
            "required": [],
            "properties": {}
        }),
        Some(ToolAnnotations {
            title: Some("Discover extensions".to_string()),
            read_only_hint: true,
            destructive_hint: false,
            idempotent_hint: false,
            open_world_hint: false,
        }),
    )
}

pub fn manage_extensions_tool() -> Tool {
    Tool::new(
        PLATFORM_MANAGE_EXTENSIONS_TOOL_NAME.to_string(),
        "Tool to manage extensions and tools in goose context.
            Enable or disable extensions to help complete tasks.
            Enable or disable an extension by providing the extension name.
            "
        .to_string(),
        json!({
            "type": "object",
            "required": ["action", "extension_name"],
            "properties": {
                "action": {"type": "string", "description": "The action to perform", "enum": ["enable", "disable"]},
                "extension_name": {"type": "string", "description": "The name of the extension to enable"}
            }
        }),
        Some(ToolAnnotations {
            title: Some("Enable or disable an extension".to_string()),
            read_only_hint: false,
            destructive_hint: false,
            idempotent_hint: false,
            open_world_hint: false,
        }),
    )
}

pub fn spawn_interactive_subagent_tool() -> Tool {
    Tool::new(
        PLATFORM_SPAWN_INTERACTIVE_SUBAGENT_TOOL_NAME.to_string(),
        indoc! {r#"
            Spawn a specialized subagent to handle specific tasks independently.
            
            Subagents are configured using recipes that define their instructions, extensions, and behavior.
            Each subagent maintains its own conversation history and can be used for specialized tasks
            like research, code review, or interactive assistance.
            
            The subagent will process the initial message and be ready for further interaction.
            Use other subagent tools to manage, communicate with, or terminate the subagent.
        "#}.to_string(),
        json!({
            "type": "object",
            "required": ["recipe_name", "message"],
            "properties": {
                "recipe_name": {
                    "type": "string", 
                    "description": "Name of the recipe file to configure the subagent (e.g., 'research_assistant_recipe.yaml')"
                },
                "message": {
                    "type": "string", 
                    "description": "Initial message to send to the subagent"
                },
                "max_turns": {
                    "type": "integer", 
                    "description": "Optional maximum number of conversation turns (default: unlimited)",
                    "minimum": 1
                },
                "timeout_seconds": {
                    "type": "integer", 
                    "description": "Optional timeout for the subagent in seconds",
                    "minimum": 1
                }
            }
        }),
        Some(ToolAnnotations {
            title: Some("Spawn interactive subagent".to_string()),
            read_only_hint: false,
            destructive_hint: false,
            idempotent_hint: false,
            open_world_hint: false,
        }),
    )
}

pub fn list_subagents_tool() -> Tool {
    Tool::new(
        PLATFORM_LIST_SUBAGENTS_TOOL_NAME.to_string(),
        "List all active subagents and their basic information.
        Returns a list of subagent IDs and their current status."
        .to_string(),
        json!({
            "type": "object",
            "required": [],
            "properties": {}
        }),
        Some(ToolAnnotations {
            title: Some("List active subagents".to_string()),
            read_only_hint: true,
            destructive_hint: false,
            idempotent_hint: true,
            open_world_hint: false,
        }),
    )
}

pub fn get_subagent_status_tool() -> Tool {
    Tool::new(
        PLATFORM_GET_SUBAGENT_STATUS_TOOL_NAME.to_string(),
        indoc! {r#"
            Get detailed status and progress information for subagents.
            
            If subagent_id is provided, returns detailed information for that specific subagent.
            If no subagent_id is provided, returns status for all active subagents.
            
            Status information includes current state, progress, turn count, and conversation history.
        "#}.to_string(),
        json!({
            "type": "object",
            "properties": {
                "subagent_id": {
                    "type": "string", 
                    "description": "Optional ID of specific subagent to get status for"
                },
                "include_conversation": {
                    "type": "boolean", 
                    "description": "Whether to include full conversation history (default: false)",
                    "default": false
                }
            }
        }),
        Some(ToolAnnotations {
            title: Some("Get subagent status".to_string()),
            read_only_hint: true,
            destructive_hint: false,
            idempotent_hint: true,
            open_world_hint: false,
        }),
    )
}

pub fn terminate_subagent_tool() -> Tool {
    Tool::new(
        PLATFORM_TERMINATE_SUBAGENT_TOOL_NAME.to_string(),
        indoc! {r#"
            Terminate one or more subagents.
            
            If subagent_id is provided, terminates that specific subagent.
            If 'all' is provided as subagent_id, terminates all active subagents.
            
            Terminated subagents cannot be restarted - you would need to spawn a new one.
        "#}.to_string(),
        json!({
            "type": "object",
            "required": ["subagent_id"],
            "properties": {
                "subagent_id": {
                    "type": "string", 
                    "description": "ID of subagent to terminate, or 'all' to terminate all subagents"
                }
            }
        }),
        Some(ToolAnnotations {
            title: Some("Terminate subagent".to_string()),
            read_only_hint: false,
            destructive_hint: true,
            idempotent_hint: false,
            open_world_hint: false,
        }),
    )
}
