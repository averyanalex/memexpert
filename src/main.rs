#![feature(let_chains)]
#![feature(async_closure)]
#![feature(iter_intersperse)]

use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use storage::Storage;
use tokio::signal;
use tracing::*;
use tracing_subscriber::prelude::*;

mod bot;
mod control;
mod openai;
mod storage;
mod web;

pub fn add_dot_if_needed(text: &str) -> String {
    let last_char = text.chars().last().unwrap_or('.');
    if last_char == '.' || last_char == '!' || last_char == '?' {
        text.to_owned()
    } else {
        format!("{text}.")
    }
}

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
    let openai = Arc::new(openai::OpenAi::new());
    let storage = Storage::new(bot.clone(), openai.clone()).await?;

    tokio::select! {
        bot_res = bot::run_bot(storage.clone(), openai, bot) => bot_res,
        web_res = web::run_webserver(storage) => web_res,
        _ = signal::ctrl_c() => Ok(())
    }
}
