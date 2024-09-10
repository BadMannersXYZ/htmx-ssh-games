use std::{iter, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use axum::Router;
use russh::{client, keys::decode_secret_key};
use tokio::{fs, net::TcpListener};
use tracing::{debug, error, info};

use crate::{http::ROUTER, ssh::TcpForwardSession};

/* Local server entrypoint */

/// Spins up a local Axum server for development.
pub async fn local_server_entrypoint(hostname: &str, port: u16) -> Result<()> {
    let listener = TcpListener::bind((hostname, port))
        .await
        .with_context(|| "Failed to bind TCP listener")?;
    println!("Listening on http://{}:{}", hostname, port);
    axum::serve(
        listener,
        Router::clone(
            ROUTER
                .get()
                .with_context(|| "Router hasn't been initialized.")?,
        ),
    )
    .await
    .with_context(|| "Server has closed.")
}

/* SSH entrypoint */

/// Begins remote port forwarding (reverse tunneling) with Russh to serve an Axum application.
pub async fn ssh_entrypoint(
    host: &str,
    port: u16,
    login_name: &str,
    identity_file: PathBuf,
    remote_host: &str,
    remote_port: u16,
    request_pty: Option<String>,
) -> Result<()> {
    let secret_key = fs::read_to_string(identity_file)
        .await
        .with_context(|| "Failed to open secret key")?;
    let secret_key =
        Arc::new(decode_secret_key(&secret_key, None).with_context(|| "Invalid secret key")?);
    let config = Arc::new(client::Config {
        ..Default::default()
    });
    loop {
        let mut reconnect_attempt = 0;
        let mut session = TcpForwardSession::connect(
            host,
            port,
            login_name,
            Arc::clone(&config),
            Arc::clone(&secret_key),
            iter::from_fn(move || {
                reconnect_attempt += 1;
                if reconnect_attempt <= 5 {
                    Some(Duration::from_secs(2 * reconnect_attempt))
                } else {
                    None
                }
            }),
        )
        .await
        .with_context(|| "Connection failed.")?;
        match session
            .start_forwarding(remote_host, remote_port, request_pty.as_deref())
            .await
        {
            Err(e) => error!(error = ?e, "TCP forward session failed."),
            _ => info!("Connection closed."),
        }
        debug!("Attempting graceful disconnect.");
        if let Err(e) = session.close().await {
            debug!(error = ?e, "Graceful disconnect failed.")
        }
        debug!("Restarting connection.");
    }
}
