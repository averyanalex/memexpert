use std::io::Cursor;
use std::str::FromStr;
use std::{fmt::Write, net::SocketAddr};

use anyhow::{anyhow, Result};
use askama::Template;

use axum::{
    body::{self, Body},
    extract::{Path, Request, State},
    http::{header, HeaderMap, HeaderName, HeaderValue, StatusCode},
    middleware,
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
use entities::{memes, translations};
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
        .layer(middleware::from_fn(minificator))
        .with_state(AppState { db });

    let addr = SocketAddr::from_str("0.0.0.0:3000")?;
    let listener = TcpListener::bind(addr).await?;
    info!("listening at {addr}");

    Ok(axum::serve(listener, app).await?)
}

async fn minificator(request: Request, next: middleware::Next) -> Response {
    let response = next.run(request).await;

    if let Some(content_type) = response.headers().get(header::CONTENT_TYPE)
        && let Ok(content_type) = content_type.to_str()
        && content_type.starts_with("text/html")
    {
        let (mut res_parts, res_body) = response.into_parts();

        if let Ok(body) = body::to_bytes(res_body, 10_000_000).await {
            let mut cfg = minify_html::Cfg::spec_compliant();
            cfg.minify_css = true;
            cfg.minify_js = true;

            let minified = minify_html::minify(&body, &cfg);

            res_parts.headers.remove(header::TRANSFER_ENCODING);
            res_parts.headers.remove(header::CONTENT_LENGTH);

            Response::from_parts(res_parts, Body::from(minified))
        } else {
            (
                StatusCode::BAD_GATEWAY,
                "error reading body for minification",
            )
                .into_response()
        }
    } else {
        response
    }
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
            lastmod: m
                .last_edition_time
                .and_utc()
                .to_rfc3339_opts(SecondsFormat::Secs, false),
            m,
            trs,
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
    let splitten: Vec<_> = filename.split('.').collect();
    let slug = splitten[0];

    Ok(
        if let Some((meme, _)) = state.db.load_meme_with_translations_by_slug(slug).await? {
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
            (headers, Ranged::new(range, body)).into_response()
        } else if let Some(new_slug) = state.db.get_slug_redirect(slug).await? {
            let new_filename: String = [new_slug.as_str()]
                .into_iter()
                .chain(splitten.into_iter().skip(1))
                .intersperse(".")
                .collect();
            Redirect::permanent(&new_filename).into_response()
        } else {
            (StatusCode::NOT_FOUND, "meme not found").into_response()
        },
    )
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

fn memes_to_gallery(memes: &[memes::Model]) -> Vec<GalleryImage> {
    memes
        .iter()
        .map(|m| GalleryImage {
            filename: format!("{}.thumb.jpg", m.slug),
            width: m.thumb_width,
            height: m.thumb_height,
            href: format!("/ru/{}", m.slug),
        })
        .collect()
}

async fn meme(
    State(state): State<AppState>,
    Path((language, slug)): Path<(String, String)>,
    headers: HeaderMap,
    jar: CookieJar,
) -> Result<Response, AppError> {
    Ok(
        if let Some((meme, translations)) =
            state.db.load_meme_with_translations_by_slug(&slug).await?
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
            state.db.save_web_visit(visit).await?;

            let similar_memes = state.db.similar_memes(meme.id, 50).await?;

            let headers = [(header::CONTENT_LANGUAGE, translation.language)];

            (
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
                    gallery: memes_to_gallery(&similar_memes),
                },
            )
                .into_response()
        } else if let Some(new_slug) = state.db.get_slug_redirect(&slug).await? {
            Redirect::permanent(&new_slug).into_response()
        } else {
            (StatusCode::NOT_FOUND, "meme not found").into_response()
        },
    )
}

async fn index(State(state): State<AppState>) -> Result<Response, AppError> {
    let headers = [(header::CONTENT_LANGUAGE, "ru")];

    let popular_memes = state.db.popular_memes(50).await?;

    Ok((
        headers,
        IndexTemplate {
            language: "ru".to_string(),
            gallery: memes_to_gallery(&popular_memes),
        },
    )
        .into_response())
}

struct GalleryImage {
    filename: String,
    width: i32,
    height: i32,
    href: String,
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    language: String,
    gallery: Vec<GalleryImage>,
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
    gallery: Vec<GalleryImage>,
}

#[derive(Template)]
#[template(path = "sitemap.xml")]
struct SitemapTemplate {
    memes: Vec<SitemapMeme>,
}

struct SitemapMeme {
    m: memes::Model,
    lastmod: String,
    trs: Vec<translations::Model>,
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
