//! Opt-in live check: uses authenticated local CLIs and prints no account data.
use dashy::dashboard::{
    models::ProviderError,
    process::SystemProcessRunner,
    providers::{
        claude::ClaudeProvider, codex::CodexProvider, cursor::CursorProvider,
        github::GitHubProvider, grok::GrokProvider, DataProvider,
    },
};
use std::{future::Future, time::Instant};

async fn report<T>(name: &str, fetch: impl Future<Output = Result<T, ProviderError>>) {
    let started = Instant::now();
    let outcome = match fetch.await {
        Ok(_) => "connected: parsed live data".to_owned(),
        Err(error) => error.to_string(),
    };
    println!("{name}: {outcome} ({}ms)", started.elapsed().as_millis());
}

#[tokio::main]
async fn main() {
    let codex = CodexProvider::new(SystemProcessRunner);
    let claude = ClaudeProvider::new(SystemProcessRunner);
    let github = GitHubProvider::new(SystemProcessRunner);
    let grok = GrokProvider::new(SystemProcessRunner);
    let cursor = CursorProvider::new(SystemProcessRunner);
    tokio::join!(
        report("codex", codex.fetch()),
        report("claude", claude.fetch()),
        report("github", github.fetch()),
        report("grok", grok.fetch()),
        report("cursor", cursor.fetch()),
    );
}
