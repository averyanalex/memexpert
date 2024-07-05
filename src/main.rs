#![feature(let_chains)]

use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use storage::Storage;
use tracing::*;
use tracing_subscriber::prelude::*;

mod bot;
mod control;
mod ms_models;
mod storage;
mod web;
mod yandex;

fn main() -> Result<()> {
    std::env::set_var("RUST_BACKTRACE", "1");

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer().with_filter(
                tracing_subscriber::filter::LevelFilter::from_str(
                    &std::env::var("RUST_LOG").unwrap_or_else(|_| String::from("info")),
                )
                .unwrap_or(tracing_subscriber::filter::LevelFilter::INFO),
            ),
        )
        .with(
            sentry::integrations::tracing::layer().event_filter(|md| match *md.level() {
                Level::TRACE => sentry::integrations::tracing::EventFilter::Ignore,
                _ => sentry::integrations::tracing::EventFilter::Breadcrumb,
            }),
        )
        .try_init()
        .unwrap();

    let _sentry_guard = match std::env::var("SENTRY_DSN") {
        Ok(d) => {
            let guard = sentry::init((
                d,
                sentry::ClientOptions {
                    release: sentry::release_name!(),
                    default_integrations: true,
                    attach_stacktrace: true,
                    send_default_pii: true,
                    max_breadcrumbs: 300,
                    // auto_session_tracking: true,
                    ..Default::default()
                },
            ));
            Some(guard)
        }
        Err(e) => {
            warn!("can't get SENTRY_DSN: {:?}", e);
            None
        }
    };

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(_main())
}

async fn _main() -> Result<()> {
    let bot = teloxide::Bot::from_env();
    let yandex = Arc::new(yandex::Yandex::new()?);
    let db = Storage::new(bot.clone(), yandex.clone()).await?;

    let (bot_res, web_res) = tokio::join!(
        bot::run_bot(db.clone(), yandex, bot.clone()),
        web::run_webserver(db)
    );
    bot_res?;
    web_res?;

    Ok(())
}
