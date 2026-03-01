use serde::{Deserialize, Serialize};
use libp2p::request_response::{self, ProtocolSupport};
use async_trait::async_trait;
use futures::prelude::*;
use std::io;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Text { context: String, timestamp: i64 },

    KeyExchange { public_key: Vec<u8> },
    EncryptedText { ciphertext: Vec<u8>, timestamp: i64 },

    Ping,
    Pong,
}

#[derive(Debug, Clone)]
pub struct MessageProtocol;

impl MessageProtocol {
    pub const PROTOCOL_NAME: &'static str = "/videocalls/message/1.0.0";
}

impl AsRef<str> for MessageProtocol {
    fn as_ref(&self) -> &str {
        Self::PROTOCOL_NAME
    }
}

#[derive(Clone, Default)]
pub struct MessageCodec;

#[async_trait]
impl request_response::Codec for MessageCodec {
    type Protocol = MessageProtocol;
    type Request = Message;
    type Response = Message;

    async fn read_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Request>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut len_bytes = [0u8; 4];
        io.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        let mut data = vec![0u8; len];
        io.read_exact(&mut data).await?;

        bincode::deserialize(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> io::Result<Self::Response>
    where
        T: AsyncRead + Unpin + Send,
    {
        let mut len_bytes = [0u8; 4];
        io.read_exact(&mut len_bytes).await?;
        let len = u32::from_be_bytes(len_bytes) as usize;

        let mut data = vec![0u8; len];
        io.read_exact(&mut data).await?;

        bincode::deserialize(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let data = bincode::serialize(&req)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let len = data.len() as u32;
        io.write_all(&len.to_be_bytes()).await?;

        io.write_all(&data).await?;
        io.flush().await?;

        Ok(())
    }

    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> io::Result<()>
    where
        T: AsyncWrite + Unpin + Send,
    {
        let data = bincode::serialize(&res)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let len = data.len() as u32;
        io.write_all(&len.to_be_bytes()).await?;
        io.write_all(&data).await?;
        io.flush().await?;

        Ok(())
    }
}

pub fn create_behaviour() -> request_response::Behaviour<MessageCodec> {
    let protocols = std::iter::once((MessageProtocol, ProtocolSupport::Full));
    let cfg = request_response::Config::default();

    request_response::Behaviour::new(protocols, cfg)
}