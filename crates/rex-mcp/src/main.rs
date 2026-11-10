use rex_daemon::{DaemonPolicy, HarnessDaemon, LeaseKeeper, LeaseKeeperConfig};
use rex_mcp::McpServer;
use std::io::BufReader;
use std::path::PathBuf;

fn main() {
    if let Err(e) = run() {
        eprintln!("rex-mcp: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    // `REX_STATE_DIR`, else the per-user default (shared with the desktop app's
    // supervision of host tasks; see `rex_mcp::client::default_state_dir`).
    let state = rex_mcp::client::default_state_dir();
    let workspace = std::env::var_os("REX_WORKSPACE")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_dir()?);
    let approved = std::env::var("REX_APPROVE_TASK_MUTATIONS").as_deref() == Ok("1");
    let mut policy = DaemonPolicy::conservative(workspace);
    policy.approve_task_mutations = approved;
    let daemon = HarnessDaemon::open(state, policy)?;
    let enabled = std::env::var("REX_LEASE_KEEPER").as_deref() != Ok("0");
    let _lease_keeper: Option<LeaseKeeper> = if enabled {
        Some(daemon.spawn_lease_keeper(LeaseKeeperConfig::default())?)
    } else {
        None
    };
    let mut server = McpServer::new(daemon);
    server.serve(
        BufReader::new(std::io::stdin().lock()),
        std::io::stdout().lock(),
    )?;
    Ok(())
}
