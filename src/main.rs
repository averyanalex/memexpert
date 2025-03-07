#![feature(let_chains)]
#![feature(iter_intersperse)]

use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use storage::Storage;
use tokio::signal;
use tracing::*;
use tracing_subscriber::prelude::*;

mod ai;
mod bot;
mod control;
mod storage;
mod web;

pub fn ensure_ends_with_punctuation(text: &str) -> String {
    let last_char = text.chars().last().unwrap_or('.');
    if last_char.is_ascii_punctuation() {
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

pub struct AppState_ {
    bot: bot::Bot,
    ai: Arc<ai::Ai>,
    storage: storage::Storage,
}

pub type AppState = Arc<AppState_>;

async fn _main() -> Result<()> {
    let bot = bot::new_bot();
    let ai = Arc::new(ai::Ai::new());
    let storage = Storage::new(bot.clone(), ai.clone()).await?;

    let app_state = Arc::new(AppState_ { bot, ai, storage });

    tokio::select! {
        bot_res = bot::run_bot(app_state.clone()) => bot_res,
        web_res = web::run_webserver(app_state) => web_res,
        _ = signal::ctrl_c() => Ok(())
    }
}
