//! The least-authority tool contract. The model is told exactly the tools
//! enabled for THIS call, derived from the harness's actual offering -
//! never from a global list of what REX can do in general. A capability the
//! call does not have must read as nonexistent, not merely discouraged.

use crate::roles::ToolPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolSpec {
    pub name: String,
    pub summary: String,
    pub requires_approval: bool,
    pub mutates: bool,
}

impl ToolSpec {
    pub fn new(name: &str, summary: &str, requires_approval: bool, mutates: bool) -> Self {
        Self {
            name: name.to_string(),
            summary: summary.to_string(),
            requires_approval,
            mutates,
        }
    }
}

/// Tool names a read-only role keeps. Anything mutating is removed even if
/// it appears here; the `mutates` flag is the final word.
pub const READ_ONLY_TOOLS: &[&str] = &[
    "update_plan",
    "read_file",
    "search_files",
    "glob_files",
    "web_search",
    "web_fetch",
    "explore",
    "complete_task",
];

/// Apply a role's tool policy to the call's actual offering.
pub fn filter_for_policy(specs: &[ToolSpec], policy: ToolPolicy) -> Vec<ToolSpec> {
    match policy {
        ToolPolicy::All => specs.to_vec(),
        ToolPolicy::None => Vec::new(),
        ToolPolicy::ReadOnly => specs
            .iter()
            .filter(|s| READ_ONLY_TOOLS.contains(&s.name.as_str()) && !s.mutates)
            .cloned()
            .collect(),
    }
}

/// Render the tool contract module body for the tools enabled at this call.
pub fn render_contract(specs: &[ToolSpec]) -> String {
    if specs.is_empty() {
        return "No tools are enabled for this call. Answer from the supplied context only. \
Do not claim to have read, written, run or searched anything beyond what is in front of you."
            .to_string();
    }
    let mut out = String::from(
        "You may use exactly the tools listed here. Tools not listed do not exist in this run: \
do not call them and do not claim their results. Tool outcomes come back from the harness as \
observations; a call you did not make did not happen.\n",
    );
    for spec in specs {
        let mut tags = Vec::new();
        tags.push(if spec.mutates {
            "mutating"
        } else {
            "read-only"
        });
        if spec.requires_approval {
            tags.push("requires trusted approval");
        }
        out.push_str(&format!(
            "- {} ({}) - {}\n",
            spec.name,
            tags.join(", "),
            spec.summary
        ));
    }
    out.push_str(ENVIRONMENT_RULE);
    if specs.iter().any(|s| s.name == "run_command") {
        out.push_str(COMMAND_RULE);
    }
    out
}

/// Tells the model what the state's `environment` block is for. The block's
/// facts vary per machine, so they live in the per-turn state; this rule is
/// the same everywhere, so it can sit in the pinned system prompt.
const ENVIRONMENT_RULE: &str = "\nThe state's `environment` block describes the machine and workspace this run is on: \
operating system, installed programs, project files, limits. Work to it. Use its OS and path conventions, \
do not assume a program it lists as not found, and do not claim an environment fact it does not state.";

/// Command guidance, present only when the call can run commands (a
/// read-only role's contract never mentions them).
const COMMAND_RULE: &str = "\nCommands: pass an argv list, one command per call; there is no shell, so no pipes, \
redirects, `&&` or globs, and the working directory is set with cwd. Commands have no network and start with a \
clean environment. Shells, interpreters, launchers and destructive commands are blocked: use the project's own \
tooling (cargo, npm, make, pytest ...) and the file tools instead. Pick a command that exists in \
`programs_on_path` and fits `workspace_files`, and stay inside `command_limits`. A blocked or failed command is an observation: change the \
approach, do not retry the same call through another blocked program.";

#[cfg(test)]
mod tests {
    use super::*;

    fn offering() -> Vec<ToolSpec> {
        vec![
            ToolSpec::new("update_plan", "plan", false, false),
            ToolSpec::new("read_file", "read", false, false),
            ToolSpec::new("create_file", "create", true, true),
            ToolSpec::new("edit_file", "edit", true, true),
            ToolSpec::new("search_files", "search", false, false),
            ToolSpec::new("run_command", "run", true, true),
            ToolSpec::new("web_search", "web", false, false),
            ToolSpec::new("complete_task", "done", false, false),
        ]
    }

    #[test]
    fn read_only_policy_strips_every_mutating_tool() {
        let scoped = filter_for_policy(&offering(), ToolPolicy::ReadOnly);
        let names: Vec<&str> = scoped.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "update_plan",
                "read_file",
                "search_files",
                "web_search",
                "complete_task"
            ]
        );
        let contract = render_contract(&scoped);
        // tool overclaiming defence: mutating tools must not exist in the contract
        for gone in ["create_file", "edit_file", "run_command"] {
            assert!(!contract.contains(gone), "contract leaked {gone}");
        }
        assert!(contract.contains("do not exist in this run"));
    }

    #[test]
    fn none_policy_renders_an_explicit_no_tools_contract() {
        let scoped = filter_for_policy(&offering(), ToolPolicy::None);
        assert!(scoped.is_empty());
        let contract = render_contract(&scoped);
        assert!(contract.contains("No tools are enabled"));
        assert!(!contract.contains("read_file ("));
    }

    #[test]
    fn the_environment_rule_is_present_and_the_command_rule_only_with_commands() {
        let with = render_contract(&filter_for_policy(&offering(), ToolPolicy::All));
        assert!(with.contains("`environment` block"));
        assert!(with.contains("no shell"));
        assert!(with.contains("programs_on_path"));
        let read_only = render_contract(&filter_for_policy(&offering(), ToolPolicy::ReadOnly));
        assert!(read_only.contains("`environment` block"));
        assert!(
            !read_only.contains("no shell"),
            "read-only roles get no command rule"
        );
        let none = render_contract(&[]);
        assert!(!none.contains("environment"));
    }

    #[test]
    fn all_policy_keeps_the_full_offering_with_tags() {
        let scoped = filter_for_policy(&offering(), ToolPolicy::All);
        assert_eq!(scoped.len(), 8);
        let contract = render_contract(&scoped);
        assert!(contract.contains("create_file (mutating, requires trusted approval)"));
        assert!(contract.contains("read_file (read-only)"));
    }
}
