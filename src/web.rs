use std::io::Cursor;
use std::str::FromStr;
use std::{fmt::Write, net::SocketAddr};

use anyhow::{anyhow, Result};
use askama::Template;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use axum_extra::{
    extract::{
        cookie::{Cookie, SameSite},
        CookieJar,
    },
    headers::Range,
    TypedHeader,
};
use axum_range::{KnownSize, Ranged};
use chrono::SecondsFormat;
use entities::{sea_orm_active_enums::MediaType, web_visits};
use include_dir::{include_dir, Dir};
use rand::{distributions::Alphanumeric, Rng};
use sea_orm::ActiveValue;
use tokio::net::TcpListener;
use tracing::*;

use crate::storage::Storage;

static ASSETS_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/assets");

pub async fn run_webserver(db: Storage) -> Result<()> {
    let app = Router::new()
        .route("/:path", get(assets))
        .route("/static/:file", get(file))
        .route("/:language/:slug", get(meme))
        .route("/", get(index))
        .route("/sitemap.xml", get(sitemap_xml))
        .route("/sitemap.txt", get(sitemap_txt))
        .with_state(AppState { db });

    let addr = SocketAddr::from_str("0.0.0.0:3000")?;
    let listener = TcpListener::bind(addr).await?;
    info!("listening at {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Clone)]
struct AppState {
    db: Storage,
}

async fn sitemap_xml(State(state): State<AppState>) -> Result<Response, AppError> {
    let memes = state.db.all_memes_with_translations().await?;

    let memes: Vec<_> = memes
        .into_iter()
        .map(|(m, trs)| SitemapMeme {
            slug: m.slug,
            lastmod: m
                .last_edition_time
                .and_utc()
                .to_rfc3339_opts(SecondsFormat::Secs, false),
            translations: trs
                .into_iter()
                .map(|tr| SitemapTranslation {
                    language: tr.language,
                })
                .collect(),
        })
        .collect();

    Ok((
        [(header::CONTENT_TYPE, "text/xml; charset=utf-8")],
        SitemapTemplate { memes },
    )
        .into_response())
}

async fn sitemap_txt(State(state): State<AppState>) -> Result<Response, AppError> {
    let memes = state.db.all_memes_with_translations().await?;
    let mut sitemap = String::new();
    for (meme, translations) in memes {
        for translation in translations {
            writeln!(
                &mut sitemap,
                "https://memexpert.xyz/{}/{}",
                translation.language, meme.slug
            )?;
        }
    }
    Ok((
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        sitemap,
    )
        .into_response())
}

async fn assets(Path(path): Path<String>) -> impl IntoResponse {
    let path = path.trim_start_matches('/');
    let mime_type = mime_guess::from_path(path).first_or_text_plain();

    match ASSETS_DIR.get_file(path) {
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
        Some(file) => Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime_type.as_ref()).unwrap(),
            )
            .header(
                header::CACHE_CONTROL,
                HeaderValue::from_static("max-age=86400"),
            )
            .body(Body::from(file.contents()))
            .unwrap(),
    }
}

async fn file(
    State(state): State<AppState>,
    Path(filename): Path<String>,
    range: Option<TypedHeader<Range>>,
) -> Result<Response, AppError> {
    let splitten = filename.split('.').collect_vec();
    let slug = splitten[0];

    let Some((meme, _)) = state.db.meme_with_translations_by_slug(slug).await? else {
        return Ok((StatusCode::NOT_FOUND, "meme not found").into_response());
    };

    let (tg_id, content_length) = if splitten.len() == 3 {
        (meme.thumb_tg_id, meme.thumb_content_length)
    } else {
        (meme.tg_id, meme.content_length)
    };

    let file = state
        .db
        .load_tg_file(&tg_id, content_length.try_into()?)
        .await?;
    let body = KnownSize::seek(Cursor::new(file)).await?;
    let range = range.map(|TypedHeader(range)| range);

    let headers = [
        (header::CACHE_CONTROL, "max-age=604800"),
        (header::CONTENT_TYPE, &meme.mime_type),
        (
            header::CONTENT_DISPOSITION,
            &format!("filename=\"{filename}\""),
        ),
    ];

    Ok((headers, Ranged::new(range, body)).into_response())
}

fn get_header(headers: &HeaderMap, name: HeaderName) -> Option<String> {
    if let Some(ua) = headers.get(name)
        && let Ok(ua) = ua.to_str()
    {
        Some(ua.to_owned())
    } else {
        None
    }
}

async fn meme(
    State(state): State<AppState>,
    Path((language, slug)): Path<(String, String)>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<Response, AppError> {
    if let Some((meme, translations)) = state.db.meme_with_translations_by_slug(&slug).await?
        && let Some(translation) = translations.into_iter().find(|tr| tr.language == language)
    {
        let mime_type: mime::Mime = meme.mime_type.parse()?;
        let locale = match language.as_str() {
            "en" => "en_US",
            "ru" => "ru_RU",
            _ => return Err(anyhow!("unknown language").into()),
        }
        .to_owned();
        let extension = match mime_type.subtype() {
            mime::JPEG => "jpg",
            mime::MP4 => "mp4",
            _ => return Err(anyhow!("unknown mime").into()),
        }
        .to_owned();

        let uid = if let Some(uid) = jar.get("uid")
            && uid.value().len() == 8
            && uid.value().chars().all(|c| c.is_alphanumeric())
        {
            uid.value().to_owned()
        } else {
            rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(8)
                .map(char::from)
                .collect()
        };
        let mut uid_cookie = Cookie::new("uid", uid.clone());
        uid_cookie.make_permanent();
        uid_cookie.set_same_site(SameSite::Strict);
        uid_cookie.set_secure(true);

        let visit = web_visits::ActiveModel {
            user_id: ActiveValue::set(uid),
            meme_id: ActiveValue::set(meme.id),
            language: ActiveValue::set(language.clone()),
            ip: ActiveValue::set(
                get_header(&headers, HeaderName::from_static("x-real-ip"))
                    .unwrap_or_else(|| "127.0.0.1".to_owned()),
            ),
            user_agent: ActiveValue::set(get_header(&headers, header::USER_AGENT)),
            referer: ActiveValue::set(get_header(&headers, header::REFERER)),

            ..Default::default()
        };
        state.db.create_web_visit(visit).await?;

        let headers = [(header::CONTENT_LANGUAGE, translation.language)];

        Ok((
            headers,
            jar.add(uid_cookie),
            MemeTemplate {
                id: meme.id,
                language,
                locale,
                filename: format!("{slug}.{extension}"),
                thumb_filename: format!("{slug}.thumb.jpg"),
                slug,
                title: translation.title,
                text: meme.text,
                caption: translation.caption,
                description: translation.description,
                mime_type: mime_type.to_string(),
                thumb_mime_type: meme.thumb_mime_type,
                is_mime_video: mime_type.type_() == mime::VIDEO,
                is_animation: meme.media_type == MediaType::Animation,
                duration: chrono::Duration::seconds(meme.duration.into()).to_string(),
                duration_secs: meme.duration,
                width: meme.width.try_into()?,
                height: meme.height.try_into()?,
                thumb_width: meme.thumb_width.try_into()?,
                thumb_height: meme.thumb_height.try_into()?,
                created_date: meme
                    .creation_time
                    .and_utc()
                    .to_rfc3339_opts(SecondsFormat::Secs, false),
                source: meme.source,
            },
        )
            .into_response())
    } else if let Some(meme_id) = state.db.get_slug_redirect(&slug).await? {
        Ok((Redirect::permanent(&format!("/{language}/{meme_id}"))).into_response())
    } else {
        Ok((StatusCode::NOT_FOUND, "meme not found").into_response())
    }
}

async fn index() -> Result<Response, AppError> {
    let headers = [(header::CONTENT_LANGUAGE, "ru")];

    Ok((
        headers,
        IndexTemplate {
            language: "ru".to_string(),
        },
    )
        .into_response())
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    language: String,
}

#[derive(Template)]
#[template(path = "meme.html")]
struct MemeTemplate {
    id: i32,
    language: String,
    locale: String,
    title: String,
    slug: String,
    filename: String,
    thumb_filename: String,
    text: Option<String>,
    is_mime_video: bool,
    is_animation: bool,
    mime_type: String,
    thumb_mime_type: String,
    caption: String,
    description: String,
    width: u32,
    height: u32,
    thumb_width: u32,
    thumb_height: u32,
    duration: String,
    duration_secs: i32,
    created_date: String,
    source: Option<String>,
}

#[derive(Template)]
#[template(path = "sitemap.xml")]
struct SitemapTemplate {
    memes: Vec<SitemapMeme>,
}

struct SitemapMeme {
    slug: String,
    lastmod: String,
    translations: Vec<SitemapTranslation>,
}

struct SitemapTranslation {
    language: String,
}

struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
