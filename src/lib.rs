//! # OpenTelemetry Logging with Tokio Tracing
//! 
//! This crate provides a convienent way to initialize the OpenTelemetry logger
//! with otlp endpoint. It uses the [`opentelemetry`] and [`tracing`]
//! crates to provide structured, context-aware logging for Rust applications.
//! 
//! Simply add the following to your `Cargo.toml`:
//! ```toml
//! [dependencies]
//! tracing = "0.1"
//! otlp-logger = "0.1"
//! ```
//! 
//! In your code initialize the logger with:
//! ```rust
//! 
//! fn main() {
//!   // Initialize the OpenTelemetry logger using environment variables
//!   otlp_logger::init();
//!   // ... your application code
//! 
//!   // and optionally call open telemetry logger shutdown to make sure all the 
//!   // data is sent to the configured endpoint before the application exits
//!   otlp_logger::shutdown();
//! }
//! ```
//! 
//! If the `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable is set, the
//! OpenTelemetry logger will be used. Otherwise, the logger will default to
//! only stdout.
//! 
//! The OpenTelemetry logger can be configured with the following environment
//! variables:
//!   - `OTEL_EXPORTER_OTLP_ENDPOINT`: The endpoint to send OTLP data to.
//!   - `OTEL_SERVICE_NAME`: The name of the service.
//!   - `OTEL_SERVICE_NAMESPACE`: The namespace of the service.
//!   - `OTEL_SERVICE_VERSION`: The version of the service.
//!   - `OTEL_SERVICE_INSTANCE_ID`: The instance ID of the service.
//!   - `OTEL_DEPLOYMENT_ENVIRONMENT`: The deployment environment of the service.
//! 
//! The OpenTelemetry logger can also be configured with the `OtlpConfig` struct, which
//! can be passed to the `init_with_config` function. The `OtlpConfig` struct can be built
//! with the `OtlpConfigBuilder` struct.
//! 
//! Once the logger is initialized, you can use the [`tracing`] macros to log
//! messages. For example:
//! ```rust
//! use tracing::{info, error};
//! 
//! fn main() {
//!    otlp_logger::init();
//!    info!("This is an info message");
//!    error!("This is an error message");
//! }
//! ```
//! 
//! Both traces and metrics are sent to the configured OTLP endpoint. The traces, 
//! metrics, and log level are configured via the RUST_LOG environment variable.
//! This behavior can be overridden by setting the `trace_level`, `metrics_level`, or
//! `stdout_level` fields in the `OtlpConfig` struct.
//! ```rust
//! use otlp_logger::{OtlpConfigBuilder, LevelFilter};
//! 
//! fn main() {
//!   let config = OtlpConfigBuilder::default()
//!                  .otlp_endpoint("http://localhost:4317".to_string())
//!                  .metrics_level(LevelFilter::TRACE)
//!                  .trace_level(LevelFilter::INFO)
//!                  .stdout_level(LevelFilter::ERROR)
//!                  .build()
//!                  .expect("failed to configure otlp-logger");
//! 
//!   otlp_logger::init_with_config(config);
//! 
//!   // ... your application code
//! 
//!   // shutdown the logger
//!   otlp_logger::shutdown();
//! }
//! ````
//! 
//! [`tracing`]: https://crates.io/crates/tracing
//! [`opentelemetry`]: https://crates.io/crates/opentelemetry
//!
use derive_builder::*;
use thiserror::Error;

use anyhow::{Context, Result};

use opentelemetry_otlp::OTEL_EXPORTER_OTLP_ENDPOINT;
use opentelemetry_sdk::propagation::TraceContextPropagator;

pub use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, *};

mod resource;
mod metrics;
mod trace;

use resource::*;
use metrics::*;
use trace::*;


#[derive(Default, Builder, Debug)]
#[builder(setter(into), default)]
pub struct OtlpConfig {    
    service_name: Option<String>,
    service_namespace: Option<String>,
    service_version: Option<String>,
    service_instant_id: Option<String>,
    deployment_environment: Option<String>,  
    otlp_endpoint: Option<String>,   
    trace_level: Option<LevelFilter>,   
    metrics_level: Option<LevelFilter>,
    stdout_level: Option<LevelFilter>,    
}

fn init_otel(config: &OtlpConfig) -> Result<()> {
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    let otlp_endpoint = config.otlp_endpoint.as_ref().context("OTLP endpoint not set")?;

    let resource = otel_resource(config);

    let tracer = otel_tracer(otlp_endpoint, resource.clone())?;
    let traces_layer = tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .with_filter(define_filter_level(config.trace_level));

    let meter = otel_meter(otlp_endpoint, resource)?;
    let metrics_layer = tracing_opentelemetry::MetricsLayer::new(meter)
        .with_filter(define_filter_level(config.metrics_level));

    let stdout_layer = fmt::Layer::default()
        .compact()
        .with_filter(define_filter_level(config.stdout_level));

    tracing_subscriber::registry()
        .with(traces_layer)
        .with(metrics_layer)
        .with(stdout_layer)
        .try_init()
        .context("Could not init tracing registry")?;

    Ok(())
}

fn define_filter_level(level: Option<LevelFilter>) -> EnvFilter {
    match level {
        Some(l) => EnvFilter::default().add_directive(l.into()),
        None => EnvFilter::from_default_env(),
    }
}

fn end_otel() {
    opentelemetry::global::shutdown_tracer_provider();
    opentelemetry::global::shutdown_logger_provider();
}

#[derive(Error, Debug)]
pub struct TryInitError {
    msg: String,
    source: anyhow::Error,
}

impl std::fmt::Display for TryInitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Error initializing OtlpLogger: {}", self.msg)
    }
}

pub fn try_init() -> Result<(), TryInitError> {
    let endpoint = std::env::var(OTEL_EXPORTER_OTLP_ENDPOINT).ok();
    let config = OtlpConfigBuilder::default()
        .otlp_endpoint(endpoint)
        .build()
        .map_err(|e| TryInitError {
            msg: "Failed to configure endpoint from environment".to_string(),
            source: e.into(),
        })?;
    init_with_config(config)
}

pub fn init_with_config(config: OtlpConfig) -> Result<(), TryInitError> {
    if config.otlp_endpoint.is_some() {
        init_otel(&config).map_err(|e| TryInitError {
            msg: "Failed to initialize OpenTelemetry".to_string(),
            source: e,
        })
    } else {
        tracing_subscriber::registry()
            .with(EnvFilter::from_default_env())
            .with(fmt::Layer::default().compact())
            .init();
        Ok(())
    }
}

pub fn init() {
    let endpoint = std::env::var(OTEL_EXPORTER_OTLP_ENDPOINT).ok();
    let config = OtlpConfigBuilder::default()
        .otlp_endpoint(endpoint)
        .build()
        .expect("failed to configure endpoint from environment");
    init_with_config(config).unwrap_or_else(|e| {
        panic!("Failed to initialize OpenTelemetry: {}", e);
    });
}

pub fn shutdown() {
    end_otel();
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_config_builder_all() {
        let config = OtlpConfigBuilder::default()
            .service_name("test-service".to_string())
            .service_namespace("test-namespace".to_string())
            .service_version("test-version".to_string())
            .service_instant_id("test-instant-id".to_string())
            .deployment_environment("test-environment".to_string())
            .otlp_endpoint(Some("http://localhost:4317".to_string()))
            .metrics_level(LevelFilter::INFO)
            .trace_level(LevelFilter::DEBUG)
            .stdout_level(LevelFilter::WARN)
            .build()
            .unwrap();

        assert_eq!(config.service_name, Some("test-service".to_string()));
        assert_eq!(config.service_namespace, Some("test-namespace".to_string()));
        assert_eq!(config.service_version, Some("test-version".to_string()));
        assert_eq!(config.service_instant_id, Some("test-instant-id".to_string()));
        assert_eq!(config.deployment_environment, Some("test-environment".to_string()));
        assert_eq!(config.otlp_endpoint, Some("http://localhost:4317".to_string()));        
        assert_eq!(config.metrics_level, Some(LevelFilter::INFO));
        assert_eq!(config.trace_level, Some(LevelFilter::DEBUG));
        assert_eq!(config.stdout_level, Some(LevelFilter::WARN));
    }

    #[test]
    fn test_config_builder_none() {
        let config = OtlpConfigBuilder::default()
            .build()
            .unwrap();

        assert_eq!(config.service_name, None);
        assert_eq!(config.service_namespace, None);
        assert_eq!(config.service_version, None);
        assert_eq!(config.service_instant_id, None);
        assert_eq!(config.deployment_environment, None);
        assert_eq!(config.otlp_endpoint, None);      
        assert_eq!(config.metrics_level, None);
        assert_eq!(config.trace_level, None);
        assert_eq!(config.stdout_level, None);  
    }

}