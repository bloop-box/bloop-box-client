/*
use std::io;
use std::net::SocketAddr;
use std::task::Poll;

use native_tls::TlsConnector;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_native_tls::TlsStream;

use crate::config::ConnectionConfig;

struct Client {
    stream: TlsStream<TcpStream>,
}

impl Client {
    pub async fn connect(config : ConnectionConfig) -> io::Result<Client> {
        let stream = TcpStream::connect((config.host, config.port)).await?;
        let tls_connector = TlsConnector::builder().build()?;
        let tls_connector = tokio_native_tls::TlsConnector::from(tls_connector);
        let mut stream = tls_connector.connect(&config.host, stream).await?;

        let header: &[u8; 2] = &[0x00, config.secret.len().into()];
        stream.write_all(header).await?;
        stream.write_all(config.secret.as_bytes()).await?;

        let mut buffer = &[u8; 1];
        stream.read_exact(buffer).await?;

        if buffer[0] == 0x00 {
            return Err();
        }

        return Ok(Client { stream });
    }

    pub async fn boop(&self, uid: [u8; 4]) -> io::Result<()> {
        stream.write_all([0x00]).await?;
        stream.write_all(uid).await?;

        let mut header = &[u8; 1];
        stream.read_exact(header).await?;

        if header[0] == 0x00 {
            Ok(());
        }

        let mut achievements = &[u8; 1];
        stream.read_exact(achievements).await?;

        Ok(());
    }
}
*/
