use shuttle_secrets::{SecretStore, Secrets};
use std::time::Duration;
use tmi::ChannelRef;

#[shuttle_runtime::main]
async fn shuttle_main(
    #[Secrets] secret_store: SecretStore,
) -> Result<Dhayib, shuttle_runtime::Error> {
    Ok(Dhayib {
        credentials: tmi::Credentials {
            nick: secret_store.must("NICK")?,
            pass: secret_store.must("PASS")?,
        },
    })
}

struct Dhayib {
    credentials: tmi::Credentials,
}

#[shuttle_runtime::async_trait]
impl shuttle_runtime::Service for Dhayib {
    async fn bind(self, _addr: std::net::SocketAddr) -> Result<(), shuttle_runtime::Error> {
        let mut client = tmi::Client::builder()
            .credentials(self.credentials)
            .connect_with_timeout(Duration::from_secs(1))
            .await
            .map_err(into_shuttle)?;

        client
            .join(ChannelRef::parse("#SadMadLadSalman").unwrap())
            .await
            .map_err(into_shuttle)?;

        loop {
            // `tokio::select` either `ctrl-c` or `client.recv()`
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    break;
                }
                msg = client.recv() => {
                    let msg = msg.map_err(into_shuttle)?;
                    let msg = msg.as_typed().map_err(into_shuttle)?;
                    handle_message(&mut client, msg).await?;
                }
            }
        }

        // Start your service and bind to the socket address
        Ok(())
    }
}

async fn handle_message(
    client: &mut tmi::Client,
    msg: tmi::Message<'_>,
) -> Result<(), shuttle_runtime::Error> {
    if let tmi::Message::Privmsg(msg) = msg {
        if msg.text().contains("6yb") || msg.text().contains("ok") {
            client
                .privmsg(msg.channel(), "6yb")
                .send()
                .await
                .map_err(into_shuttle)?;
        }
    }

    Ok(())
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
