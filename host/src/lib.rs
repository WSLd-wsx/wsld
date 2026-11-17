//! Shared CLI entry for separately built WSX editions.
mod cli;

/// Execute exactly the same CLI implementation used by the existing host.
pub fn execute_cli(args: &[String]) -> Result<(), String> {
    cli::execute(args)
}
