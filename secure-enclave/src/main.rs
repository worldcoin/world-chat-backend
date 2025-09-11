//! Run with
//!
//! ```not_rust
//! cargo run -p example-low-level-rustls
//! ```

use anyhow::Context;
use axum::{extract::Request, response::IntoResponse, routing::get, Router};
use hyper::{body::Incoming, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rcgen::{generate_simple_self_signed, CertifiedKey};
use std::{sync::Arc, time::Duration};
use tokio::time::sleep;
use tokio_rustls::{
    rustls::{
        pki_types::{CertificateDer, PrivateKeyDer},
        ServerConfig,
    },
    TlsAcceptor,
};
use tokio_vsock::{VsockAddr, VsockListener, VMADDR_CID_ANY};
use tower_service::Service;
use tracing::{error, info, warn};

const SERVER_PORT: u32 = 5000;
const MAX_RETRIES: u32 = 5;
const RETRY_DELAY_SECS: u64 = 2;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting secure enclave server...");

    // Install crypto provider once at startup
    tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install crypto provider"))?;

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("debug")),
        )
        .with_target(true) // Show module path
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_file(true)
        .with_line_number(true)
        .with_level(true)
        .pretty()
        .init();

    info!("Starting secure enclave server...");

    loop {
        if let Err(e) = run_server().await {
            error!("Failed to run server {e:?}");
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }
    }
}

async fn run_server() -> anyhow::Result<()> {
    // Generate TLS configuration with retry logic
    let rustls_config = rustls_server_config().await?;

    let tls_acceptor = TlsAcceptor::from(rustls_config);

    // Bind listener with retry logic

    let listener = VsockListener::bind(VsockAddr::new(VMADDR_CID_ANY, SERVER_PORT))
        .context("Failed to bind listener")?;

    info!("HTTPS server listening on port {SERVER_PORT}. To contact: curl -k https://localhost:{SERVER_PORT}");
    let app = Router::new().route("/", get(handler));

    loop {
        info!("Waiting for connection...");
        let tower_service = app.clone();
        let tls_acceptor = tls_acceptor.clone();

        // Wait for new tcp connection with error handling
        match listener.accept().await {
            Ok((cnx, addr)) => {
                info!("Accepted connection from {addr}");
                tokio::spawn(async move {
                    // Wait for tls handshake to happen
                    let Ok(stream) = tls_acceptor.accept(cnx).await else {
                        error!("Error during TLS handshake connection from {}", addr);
                        return;
                    };

                    // Hyper has its own `AsyncRead` and `AsyncWrite` traits and doesn't use tokio.
                    // `TokioIo` converts between them.
                    let stream = TokioIo::new(stream);

                    // Hyper also has its own `Service` trait and doesn't use tower. We can use
                    // `hyper::service::service_fn` to create a hyper `Service` that calls our app through
                    // `tower::Service::call`.
                    let hyper_service =
                        hyper::service::service_fn(move |request: Request<Incoming>| {
                            // We have to clone `tower_service` because hyper's `Service` uses `&self` whereas
                            // tower's `Service` requires `&mut self`.
                            //
                            // We don't need to call `poll_ready` since `Router` is always ready.
                            tower_service.clone().call(request)
                        });

                    let ret = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                        .serve_connection_with_upgrades(stream, hyper_service)
                        .await;

                    if let Err(err) = ret {
                        warn!("Error serving connection from {}: {}", addr, err);
                    }
                });
            }
            Err(e) => {
                error!(
                    "Failed to accept connection: {}. Continuing to listen...",
                    e
                );
                // Sleep briefly to avoid tight loop on persistent errors
                sleep(Duration::from_millis(100)).await;
                continue;
            }
        }
    }
}

async fn handler() -> impl IntoResponse {
    (StatusCode::OK, "Hello, World!")
}

async fn rustls_server_config() -> anyhow::Result<Arc<ServerConfig>> {
    // Generate fresh certificate with rcgen
    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let CertifiedKey { cert, signing_key } = generate_simple_self_signed(subject_alt_names)
        .context("Failed to generate self-signed certificate")?;

    // Convert to DER format for rustls
    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::try_from(signing_key.serialize_der())
        .map_err(|e| anyhow::anyhow!("Failed to serialize private key: {}", e))?;

    let mut config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .context("Failed to build server config with certificate")?;

    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    info!("Generated fresh self-signed certificate for localhost");

    Ok(Arc::new(config))
}
