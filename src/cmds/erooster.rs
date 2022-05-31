//! Erooster Mail Server
//!
//! Erooster is a rust native imap server build on modern solutions.
//! The goal being easy to setup, use and maintain for smaller mail servers
//! while being also fast and efficient.
//!
#![feature(string_remove_matches)]
#![deny(unsafe_code)]
#![warn(
    clippy::cognitive_complexity,
    clippy::branches_sharing_code,
    clippy::imprecise_flops,
    clippy::missing_const_for_fn,
    clippy::mutex_integer,
    clippy::path_buf_push_overwrite,
    clippy::redundant_pub_crate,
    clippy::pedantic,
    clippy::dbg_macro,
    clippy::todo,
    clippy::fallible_impl_from,
    clippy::filetype_is_file,
    clippy::suboptimal_flops,
    clippy::fn_to_numeric_cast_any,
    clippy::if_then_some_else_none,
    clippy::imprecise_flops,
    clippy::lossy_float_literal,
    clippy::panic_in_result_fn,
    clippy::clone_on_ref_ptr
)]
#![warn(missing_docs)]
#![allow(clippy::missing_panics_doc)]

use clap::Parser;
use color_eyre::eyre::Result;
use erooster::{
    backend::{database::get_database, storage::get_storage},
    panic_handler::EroosterPanicMessage,
};
use std::borrow::Cow;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, default_value = "./config.yml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let builder = color_eyre::config::HookBuilder::default().panic_message(EroosterPanicMessage);
    let (panic_hook, eyre_hook) = builder.into_hooks();
    eyre_hook.install()?;

    let args = Args::parse();
    info!("Starting ERooster Server");
    let config = erooster::get_config(args.config).await?;

    let mut _guard;
    if config.sentry {
        tracing_subscriber::Registry::default()
            .with(sentry::integrations::tracing::layer())
            .with(tracing_subscriber::fmt::Layer::default())
            .init();
        info!("Sentry logging is enabled. Change the config to disable it.");

        _guard = sentry::init((
            "https://49e511ff807e45ffa19be1c63cfda26c@o105177.ingest.sentry.io/6458648",
            sentry::ClientOptions {
                release: Some(Cow::Owned(format!(
                    "{}@{}:{}",
                    env!("CARGO_PKG_NAME"),
                    env!("CARGO_PKG_VERSION"),
                    env!("VERGEN_GIT_SHA_SHORT")
                ))),
                traces_sample_rate: 0.2,
                ..Default::default()
            },
        ));
    } else {
        info!("Sentry logging is disabled. Change the config to enable it.");
        tracing_subscriber::fmt::init();
    }
    let next = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        eprintln!("{}", panic_hook.panic_report(panic_info));
        next(panic_info);
    }));

    let database = Arc::new(get_database(Arc::clone(&config)).await?);
    let storage = Arc::new(get_storage(Arc::clone(&database)));
    erooster::imap_servers::start(
        Arc::clone(&config),
        Arc::clone(&database),
        Arc::clone(&storage),
    )?;
    // We do need the let here to make sure that the runner is bound to the lifetime of main.
    let _runner = erooster::smtp_servers::start(config, database, storage).await?;

    match signal::ctrl_c().await {
        Ok(()) => {}
        Err(err) => {
            error!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
        }
    }
    Ok(())
}
