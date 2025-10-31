//! Utility code to handle quic networking.

use {
    crate::connection_workers_scheduler::BindTarget,
    quinn::{
        crypto::rustls::QuicClientConfig, default_runtime, ClientConfig, Connection, Endpoint,
        EndpointConfig, IdleTimeout, TransportConfig,
    },
    solana_quic_definitions::{QUIC_KEEP_ALIVE, QUIC_MAX_TIMEOUT, QUIC_SEND_FAIRNESS},
    solana_streamer::nonblocking::quic::ALPN_TPU_PROTOCOL_ID,
    solana_tls_utils::tls_client_config_builder,
    std::sync::Arc,
};

pub mod error;

pub use {
    error::{IoErrorWithPartialEq, QuicError},
    solana_tls_utils::QuicClientCertificate,
};

pub(crate) fn create_client_config(client_certificate: &QuicClientCertificate) -> ClientConfig {
    let mut crypto = tls_client_config_builder()
        .with_client_auth_cert(
            vec![client_certificate.certificate.clone()],
            client_certificate.key.clone_key(),
        )
        .expect("Failed to set QUIC client certificates");
    crypto.enable_early_data = true;
    crypto.alpn_protocols = vec![ALPN_TPU_PROTOCOL_ID.to_vec()];

    let transport_config = {
        let mut res = TransportConfig::default();

        let timeout = IdleTimeout::try_from(Duration::from_millis(100)).unwrap();
        res.max_idle_timeout(Some(timeout));
        res.keep_alive_interval(Duration::from_millis(50));
        res.send_fairness(QUIC_SEND_FAIRNESS);

        res
    };

    let mut config = ClientConfig::new(Arc::new(QuicClientConfig::try_from(crypto).unwrap()));
    config.transport_config(Arc::new(transport_config));

    config
}

pub(crate) fn create_client_endpoint(
    bind: BindTarget,
    client_config: ClientConfig,
) -> Result<Endpoint, QuicError> {
    let mut endpoint = match bind {
        BindTarget::Address(bind_addr) => {
            Endpoint::client(bind_addr).map_err(IoErrorWithPartialEq::from)?
        }
        BindTarget::Socket(socket) => {
            let runtime = default_runtime()
                .ok_or_else(|| std::io::Error::other("no async runtime found"))
                .map_err(IoErrorWithPartialEq::from)?;
            Endpoint::new(EndpointConfig::default(), None, socket, runtime)
                .map_err(IoErrorWithPartialEq::from)?
        }
    };
    endpoint.set_default_client_config(client_config);
    Ok(endpoint)
}

pub(crate) async fn send_data_over_stream(
    connection: &Connection,
    data: &[u8],
) -> Result<(), QuicError> {
    let mut send_stream = connection.open_uni().await?;
    send_stream.write_all(data).await.map_err(QuicError::from)?;

    // Stream will be finished when dropped. Finishing here explicitly is a noop.
    Ok(())
}
