use std::{net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use axum::extract::Request;
use hyper::{body::Incoming, service::service_fn};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
};
use russh::{
    client::{self, Config, Handle, Msg, Session},
    keys::key::{self, KeyPair},
    Channel, ChannelId, ChannelMsg, Disconnect,
};
use tokio::{
    io::{stderr, stdout, AsyncWriteExt},
    time::sleep,
};
use tower::Service;
use tracing::{debug, debug_span, info, trace};

use crate::{http::ROUTER, unwrap_infallible};

/* Russh session and client */

/// User-implemented session type as a helper for interfacing with the SSH protocol.
pub struct TcpForwardSession(Handle<Client>);

/// User-implemented session type as a helper for interfacing with the SSH protocol.
impl TcpForwardSession {
    /// Attempts to connect to the SSH server. If authentication fails, it returns an error value immediately.
    ///
    /// Our reconnection strategy comes from an iterator which yields `Duration`s. Each one tells us how long to delay
    /// our next reconnection attempt. The function will stop attempting to reconnect once the iterator
    /// stops yielding values.
    pub async fn connect(
        host: &str,
        port: u16,
        login_name: &str,
        config: Arc<Config>,
        secret_key: Arc<KeyPair>,
        mut timer_iterator: impl Iterator<Item = Duration>,
    ) -> Result<Self> {
        let span = debug_span!("TcpForwardSession.connect");
        let _enter = span;
        debug!("TcpForwardSession connecting...");
        let mut attempts = 0u32;
        let session = loop {
            attempts += 1;
            debug!("Connection retry #{}", attempts);
            match client::connect(Arc::clone(&config), (host, port), Client {}).await {
                Ok(mut session) => {
                    if session
                        .authenticate_publickey(login_name, Arc::clone(&secret_key))
                        .await
                        .with_context(|| "Error while authenticating with public key.")?
                    {
                        debug!(attempts = attempts, "Public key authentication succeeded!");
                        break session;
                    } else {
                        return Err(anyhow!("Public key authentication failed."));
                    }
                }
                Err(err) => {
                    debug!(err = ?err, "Unable to connect to remote host.");
                    let Some(duration) = timer_iterator.next() else {
                        debug!(attempts = attempts, "Failed to recconect.");
                        return Err(anyhow!("Gave up graceful reconnection."));
                    };
                    sleep(duration).await;
                }
            }
        };
        Ok(Self(session))
    }

    /// Sends a port forwarding request and opens a session to receive miscellaneous data.
    /// The function yields when the session is broken (for example, if the connection was lost).
    pub async fn start_forwarding(
        &mut self,
        remote_host: &str,
        remote_port: u16,
        request_pty: Option<&str>,
    ) -> Result<u32> {
        let span = debug_span!("TcpForwardSession.start");
        let _enter = span;
        let session = &mut self.0;
        session
            .tcpip_forward(remote_host, remote_port.into())
            .await
            .with_context(|| "tcpip_forward error.")?;
        debug!("Requested tcpip_forward session.");
        let mut channel = session
            .channel_open_session()
            .await
            .with_context(|| "channel_open_session error.")?;
        debug!("Created open session channel.");
        // let mut stdin = stdin();
        let mut stdout = stdout();
        let mut stderr = stderr();
        if let Some(cmd) = request_pty {
            let size = termsize::get().unwrap();
            channel
                .request_pty(
                    false,
                    &std::env::var("TERM").unwrap_or("xterm".into()),
                    size.cols.into(),
                    size.rows.into(),
                    0,
                    0,
                    &[],
                )
                .await
                .with_context(|| "Unable to request pseudo-terminal.")?;
            debug!("Requested pseudo-terminal.");
            channel
                .exec(true, cmd)
                .await
                .with_context(|| "Unable to execute command for pseudo-terminal.")?;
        };
        let code = loop {
            let Some(msg) = channel.wait().await else {
                return Err(anyhow!("Unexpected end of channel."));
            };
            trace!("Got a message through initial session!");
            match msg {
                ChannelMsg::Data { ref data } => {
                    stdout.write_all(data).await?;
                    stdout.flush().await?;
                }
                ChannelMsg::ExtendedData { ref data, ext: 1 } => {
                    stderr.write_all(data).await?;
                    stderr.flush().await?;
                }
                ChannelMsg::Success => (),
                ChannelMsg::Close => break 0,
                ChannelMsg::ExitStatus { exit_status } => {
                    debug!("Exited with code {exit_status}");
                    channel
                        .eof()
                        .await
                        .with_context(|| "Unable to close connection.")?;
                    break exit_status;
                }
                msg => return Err(anyhow!("Unknown message type {:?}.", msg)),
            }
        };
        Ok(code)
    }

    pub async fn close(&mut self) -> Result<()> {
        self.0
            .disconnect(Disconnect::ByApplication, "", "English")
            .await?;
        Ok(())
    }
}

/// Our SSH client implementing the `Handler` callbacks for the functions we need to use.
struct Client {}

#[async_trait]
impl client::Handler for Client {
    type Error = anyhow::Error;

    /// Always accept the SSH server's pubkey. Don't do this in production.
    #[allow(unused_variables)]
    async fn check_server_key(
        &mut self,
        server_public_key: &key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    /// Handle a new forwarded connection, represented by a specific `Channel`. We will create a clone of our router,
    /// and forward any messages from this channel with its streaming API.
    ///
    /// To make Axum behave with streaming, we must turn it into a Tower service first.
    /// And to handle the SSH channel as a stream, we will use a utility method from Tokio that turns our
    /// AsyncRead/Write stream into a `hyper` IO object.
    ///
    /// See also: [axum/examples/serve-with-hyper](https://github.com/tokio-rs/axum/blob/main/examples/serve-with-hyper/src/main.rs)
    #[allow(unused_variables)]
    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<Msg>,
        connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let span = debug_span!("server_channel_open_forwarded_tcpip");
        let _enter = span.enter();
        debug!(
            sshid = %String::from_utf8_lossy(session.remote_sshid()),
            connected_address = connected_address,
            connected_port = connected_port,
            originator_address = originator_address,
            originator_port = originator_port,
            "New connection!"
        );
        let address = SocketAddr::new(
            originator_address.parse().unwrap(),
            originator_port.try_into().unwrap(),
        );
        let mut router = ROUTER
            .get()
            .with_context(|| "Router hasn't been initialized.")?
            .clone()
            .into_make_service_with_connect_info::<SocketAddr>();
        // See https://github.com/tokio-rs/axum/blob/6efcb75d99a437fa80c81e2308ec8234b023e1a7/examples/unix-domain-socket/src/main.rs#L66
        let tower_service = unwrap_infallible(router.call(address).await);
        let hyper_service =
            service_fn(move |req: Request<Incoming>| tower_service.clone().call(req));
        // tokio::spawn is required to let us reply over the data channel.
        tokio::spawn(async move {
            Builder::new(TokioExecutor::new())
                .serve_connection_with_upgrades(TokioIo::new(channel.into_stream()), hyper_service)
                .await
                .expect("Invalid request");
        });
        Ok(())
    }

    #[allow(unused_variables)]
    async fn auth_banner(
        &mut self,
        banner: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!("Received auth banner.");
        let mut stdout = stdout();
        stdout.write_all(banner.as_bytes()).await?;
        stdout.flush().await?;
        Ok(())
    }

    #[allow(unused_variables)]
    async fn exit_status(
        &mut self,
        channel: ChannelId,
        exit_status: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!(channel = ?channel, "exit_status");
        if exit_status == 0 {
            info!("Remote exited with status {}.", exit_status);
        } else {
            info!("Remote exited with status {}.", exit_status);
        }
        Ok(())
    }

    #[allow(unused_variables)]
    async fn channel_open_confirmation(
        &mut self,
        channel: ChannelId,
        max_packet_size: u32,
        window_size: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!(channel = ?channel, max_packet_size, window_size, "channel_open_confirmation");
        Ok(())
    }

    #[allow(unused_variables)]
    async fn channel_success(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        debug!(channel = ?channel, "channel_success");
        Ok(())
    }
}
