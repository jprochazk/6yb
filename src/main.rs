use shuttle_secrets::{SecretStore, Secrets};
use tmi::client::write::SameMessageBypass;

#[shuttle_runtime::main]
async fn shuttle_main(
    #[Secrets] secret_store: SecretStore,
) -> Result<Tayb, shuttle_runtime::Error> {
    let channels = Channels::parse(secret_store.must("CHANNELS")?)?;
    let client = tmi::Client::builder()
        .credentials(tmi::Credentials {
            nick: secret_store.must("NICK")?,
            pass: secret_store.must("PASS")?,
        })
        .connect()
        .await
        .map_err(into_shuttle)?;
    let smb = SameMessageBypass::default();
    Ok(Tayb {
        channels,
        client,
        smb,
    })
}

struct Channels(Vec<tmi::Channel>);

impl Channels {
    fn parse(channels_csv: String) -> Result<Self, shuttle_runtime::Error> {
        Ok(Self(
            channels_csv
                .split(',')
                .map(|v| v.to_owned())
                .map(tmi::Channel::parse)
                .collect::<Result<Vec<_>, _>>()
                .map_err(into_shuttle)?,
        ))
    }
}

impl std::ops::Deref for Channels {
    type Target = Vec<tmi::Channel>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct Tayb {
    channels: Channels,
    client: tmi::Client,
    smb: SameMessageBypass,
}

#[shuttle_runtime::async_trait]
impl shuttle_runtime::Service for Tayb {
    async fn bind(mut self, _addr: std::net::SocketAddr) -> Result<(), shuttle_runtime::Error> {
        self.on_connect().await?;

        loop {
            // `tokio::select` either `ctrl-c` or `client.recv()`
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    break;
                }
                msg = self.client.recv() => {
                    let msg = msg.map_err(into_shuttle)?;
                    let msg = msg.as_typed().map_err(into_shuttle)?;
                    self.handle_message(msg).await?;
                }
            }
        }

        // Start your service and bind to the socket address
        Ok(())
    }
}

impl Tayb {
    async fn on_connect(&mut self) -> Result<(), shuttle_runtime::Error> {
        self.client
            .join_all(&*self.channels)
            .await
            .map_err(into_shuttle)?;
        Ok(())
    }

    async fn handle_message(
        &mut self,
        msg: tmi::Message<'_>,
    ) -> Result<(), shuttle_runtime::Error> {
        match msg {
            tmi::Message::Privmsg(msg) => {
                const OK: &[&str] = &["6yb", "ok"];
                let tayb = msg.text().split_ascii_whitespace().any(|v| OK.contains(&v));
                if tayb {
                    self.client
                        .privmsg(msg.channel(), &format!("6yb{}", self.smb.get()))
                        .send()
                        .await
                        .map_err(into_shuttle)?;
                }
            }
            tmi::Message::Ping(ping) => {
                self.client.pong(&ping).await.map_err(into_shuttle)?;
            }
            tmi::Message::Reconnect => {
                self.client.reconnect().await.map_err(into_shuttle)?;
                self.on_connect().await?;
            }
            _ => {}
        }

        Ok(())
    }
}

trait SecretStoreExt {
    fn must(&self, key: &str) -> Result<String, shuttle_runtime::Error>;
}

impl SecretStoreExt for SecretStore {
    fn must(&self, key: &str) -> Result<String, shuttle_runtime::Error> {
        Ok(self
            .get(key)
            .ok_or_else(|| shuttle_runtime::CustomError::msg(format!("{key} not set")))?)
    }
}

fn into_shuttle<E>(err: E) -> shuttle_runtime::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    shuttle_runtime::CustomError::new(err).into()
}
