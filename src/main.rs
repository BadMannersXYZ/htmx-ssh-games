use std::path::PathBuf;

use anyhow::Result;

use clap::{Parser, Subcommand, ValueEnum};
use htmx_axum_russh_games::{
    entrypoint::{local_server_entrypoint, ssh_entrypoint},
    http::{checkbox, multipaint_by_numbers, ROUTER},
};
use tracing::trace;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Debug, Clone, Subcommand)]
enum OperationMode {
    /// Run a conventional HTTP server locally.
    LocalServer {
        /// Hostname to listen to.
        #[arg(short = 'H', long, default_value_t = String::from("localhost"))]
        hostname: String,

        /// Local port to expose our site.
        #[arg(short, long, default_value_t = 5023)]
        port: u16,
    },

    /// Expose the HTTP server through SSH remote port forwarding.
    Ssh {
        /// SSH hostname.
        hostname: String,

        /// SSH port.
        #[arg(short, long, default_value_t = 22)]
        port: u16,

        /// Identity file containing private key.
        #[arg(short, long, default_value_t = String::from(""))]
        login_name: String,

        /// Identity file containing private key.
        #[arg(short, long, value_name = "FILE")]
        identity_file: PathBuf,

        /// Remote hostname to bind to.
        #[arg(short = 'R', long, default_value_t = String::from(""))]
        remote_host: String,

        /// Remote port to bind to.
        #[arg(short = 'P', long, default_value_t = 80)]
        remote_port: u16,

        /// Request a pseudo-terminal to be allocated with the given command.
        #[arg(long)]
        request_pty: Option<String>,
    },
}

#[derive(Debug, Copy, Clone, ValueEnum)]
enum ActivityRouter {
    /// 400 Checkboxes - A barebones clone of One Million Checkboxes.
    Checkboxes,
    /// Multipaint by Numbers - A multiplayer nonogram/picross.
    Multipaint,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct MainEntrypointArgs {
    /// Which activity router to serve.
    #[arg(value_enum, default_value_t = ActivityRouter::Checkboxes)]
    router: ActivityRouter,

    /// Which mode to run this application as.
    #[command(subcommand)]
    mode: OperationMode,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();
    trace!("Tracing is up!");
    let args = MainEntrypointArgs::parse();
    match args.router {
        ActivityRouter::Checkboxes => ROUTER.set(checkbox::get_router()).unwrap(),
        ActivityRouter::Multipaint => ROUTER
            .set(multipaint_by_numbers::get_router().await)
            .unwrap(),
    }
    match args.mode {
        OperationMode::LocalServer { hostname, port } => {
            local_server_entrypoint(hostname.as_str(), port).await
        }
        OperationMode::Ssh {
            hostname,
            port,
            login_name,
            identity_file,
            remote_host,
            remote_port,
            request_pty,
        } => {
            ssh_entrypoint(
                hostname.as_str(),
                port,
                login_name.as_str(),
                identity_file,
                remote_host.as_str(),
                remote_port,
                request_pty,
            )
            .await
        }
    }
}
