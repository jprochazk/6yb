use governor::Quota;
use shuttle_runtime::{SecretStore, Secrets};
use std::num::NonZeroU32;
use std::time::Duration;
use tmi::client::write::SameMessageBypass;

#[shuttle_runtime::main]
async fn shuttle_main(
    #[Secrets] secret_store: SecretStore,
) -> Result<Tayb, shuttle_runtime::Error> {
    let channels = secret_store
        .must("CHANNELS")?
        .split(',')
        .map(String::from)
        .collect();
    let client = tmi::Client::builder()
        .credentials(tmi::Credentials::new(
            secret_store.must("NICK")?,
            secret_store.must("PASS")?,
        ))
        .connect()
        .await
        .map_err(into_shuttle)?;
    let smb = SameMessageBypass::default();
    Ok(Tayb {
        channels,
        client,
        smb,
        rate_limit: UserRateLimit::new(),
    })
}

struct UserRateLimit(governor::DefaultKeyedRateLimiter<String>);

impl UserRateLimit {
    fn new() -> Self {
        Self(governor::DefaultKeyedRateLimiter::hashmap(
            Quota::with_period(Duration::from_secs(3))
                .unwrap()
                .allow_burst(NonZeroU32::new(1).unwrap()),
        ))
    }

    fn can_reply_to(&mut self, user: String) -> bool {
        self.0.check_key(&user).is_ok()
    }
}

struct Tayb {
    channels: Vec<String>,
    client: tmi::Client,
    smb: SameMessageBypass,
    rate_limit: UserRateLimit,
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

struct Phrase<'a> {
    needle: &'a [&'a str],
    reply: &'a str,
}

const PHRASES: &[Phrase] = &[
    Phrase {
        needle: &["6yb", "ok", "طيب"],
        reply: "6yb",
    },
    Phrase {
        needle: &["yep", "yebb"],
        reply: "YEBB",
    },
];

impl<'a> Phrase<'a> {
    fn get_reply(text: &str) -> Option<&'static str> {
        let fragments = text.split_ascii_whitespace();
        for phrase in PHRASES {
            if fragments
                .clone()
                .any(|frag| phrase.needle.iter().any(|v| frag.eq_ignore_ascii_case(v)))
            {
                return Some(phrase.reply);
            }
        }

        None
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
                // !pyramid <n>
                if let Some(mut args) = msg.text().strip_prefix("!pyramid") {
                    args = args.trim();
                    if let Ok(n) = args.parse::<u8>() {
                        return self.pyramid(msg.sender().login(), msg.channel(), n).await;
                    }
                }

                if let Some(reply) = Phrase::get_reply(msg.text()) {
                    let user = msg.sender().login().to_owned();
                    if self.rate_limit.can_reply_to(user) {
                        self.client
                            .privmsg(msg.channel(), &format!("{reply}{}", self.smb.get()))
                            .send()
                            .await
                            .map_err(into_shuttle)?;
                    }
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

    async fn pyramid(
        &mut self,
        sender_login: &str,
        channel: &str,
        n: u8,
    ) -> Result<(), shuttle_runtime::Error> {
        use std::fmt::Write;

        fn privmsg(channel: &str, text: &str, out: &mut impl Write) {
            write!(out, "PRIVMSG {channel} :{text}\r\n").ok();
        }

        const ALLOW_LIST: &[&str] = &["mosscode", "vesdev"];
        if !ALLOW_LIST.contains(&sender_login) {
            return Ok(());
        }

        if !(1..=4).contains(&n) {
            return Ok(());
        }

        let mut batch = String::new();
        let mut line = Vec::new();

        // 6yb
        // 6yb 6yb
        // 6yb 6yb 6yb
        // 6yb 6yb
        // 6yb

        for _ in 0..n {
            line.push("6yb");
            privmsg(channel, &line.join(" "), &mut batch);
        }
        for _ in 0..n - 1 {
            line.pop();
            privmsg(channel, &line.join(" "), &mut batch);
        }

        self.client
            .send_raw(batch.as_str())
            .await
            .map_err(into_shuttle)
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
