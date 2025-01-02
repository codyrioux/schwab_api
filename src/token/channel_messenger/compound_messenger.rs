//! A messenger that uses standard input/output.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::{AuthContext, ChannelMessenger};
use crate::error::Error;

#[derive(Debug)]
pub struct CompoundMessenger<CM0: ChannelMessenger, CM1: ChannelMessenger> {
    select: AtomicUsize,
    default: CM0,
    other: CM1,
}

impl<CM0: ChannelMessenger, CM1: ChannelMessenger> CompoundMessenger<CM0, CM1> {
    pub fn new(default: CM0, other: CM1) -> Self {
        Self {
            select: AtomicUsize::new(0),
            default,
            other,
        }
    }
}

impl<CM0: ChannelMessenger, CM1: ChannelMessenger> ChannelMessenger
    for CompoundMessenger<CM0, CM1>
{
    async fn with_context(&mut self, context: AuthContext) -> Result<(), Error> {
        self.default.with_context(context.clone()).await?;
        self.other.with_context(context).await?;

        Ok(())
    }

    async fn send_auth_message(&self) -> Result<(), Error> {
        loop {
            let result = match self.select.load(Ordering::Acquire) {
                0 => self.default.send_auth_message().await,
                1 => self.other.send_auth_message().await,
                _ => {
                    return Err(Error::ChannelMessenger(
                        "No Messengers available to send".to_string(),
                    ))
                }
            };

            match result {
                Ok(()) => return Ok(()),
                Err(e) => {
                    println!("error:{e}, select next messenger");
                    self.select.fetch_add(1, Ordering::AcqRel);
                    continue;
                }
            }
        }
    }

    async fn receive_auth_message(&self) -> Result<String, Error> {
        match self.select.load(Ordering::Acquire) {
            0 => self.default.receive_auth_message().await,
            1 => self.other.receive_auth_message().await,
            _ => Err(Error::ChannelMessenger(
                "No Messengers receive successfully".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use oauth2::CsrfToken;
    use std::path::PathBuf;

    use crate::token::channel_messenger::{
        local_server::LocalServerMessenger, stdio_messenger::StdioMessenger,
    };

    #[tokio::test]
    #[ignore = "Testing manually for compound verification. Should be --nocapture"]
    async fn test_compound_messenger() {
        let context = AuthContext {
            auth_url: Some(
                "https://127.0.0.1:8081/?state=CSRF&code=code"
                    .parse()
                    .unwrap(),
            ),
            csrf: Some(CsrfToken::new("CSRF".to_string())),
            redirect_url: Some("https://127.0.0.1:8081".parse().unwrap()),
        };

        let certs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/certs");
        let mut messenger = CompoundMessenger::new(
            LocalServerMessenger::new(&certs_dir).await,
            StdioMessenger::new(),
        );

        messenger.with_context(context).await.unwrap();
        messenger.send_auth_message().await.unwrap();

        // if in stdio, you should input https://127.0.0.1:8081/?state=CSRF&code=code
        assert_eq!("code", messenger.receive_auth_message().await.unwrap());
    }
}
