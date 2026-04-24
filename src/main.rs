use anyhow::Result;
use rmcp::ServiceExt;
use rmcp::transport::stdio;
use tracing_subscriber::EnvFilter;

use cosmic_grackle::contact_store::ContactStoreHandle;
use cosmic_grackle::server::ContactsServer;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    tracing::info!("Starting macOS Contacts MCP server");

    let store = ContactStoreHandle::new().map_err(|e| anyhow::anyhow!("{}", e))?;
    let server = ContactsServer::new(store);

    let service = server
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("serving error: {:?}", e))?;

    service.waiting().await?;
    Ok(())
}
